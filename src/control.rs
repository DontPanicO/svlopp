// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    io,
    os::fd::{BorrowedFd, OwnedFd},
    path::Path,
};

use rustix::fs::{CWD, Mode, OFlags, mkfifoat, open};

const OP_STOP: u8 = 0x41;
const OP_START: u8 = 0x42;
const OP_RESTART: u8 = 0x43;
const WIRE_COMMAND_SIZE: usize = 256;

/// Create (or reuse) the control fifo at `path` and return the read and
/// write ends.
///
/// A write end must be kept open to prevent the read end from receiving
/// `EOF` when no other writers are open
pub fn create_control_fifo(path: &Path) -> io::Result<(OwnedFd, OwnedFd)> {
    match mkfifoat(CWD, path, Mode::from_bits_truncate(0o600)) {
        Ok(()) => {}
        Err(e) if e == rustix::io::Errno::EXIST => {}
        Err(e) => return Err(e.into()),
    };
    let read_end_fd = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )?;
    let write_end_fd = open(
        path,
        OFlags::WRONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )?;
    Ok((read_end_fd, write_end_fd))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlProtocolError {
    InvalidOp(u8),
    InvalidNameLen(u8),
    InvalidUtf8,
    PartialFrame(usize),
}

impl std::fmt::Display for ControlProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOp(op) => write!(f, "invalid opcode: 0x{:02x}", op),
            Self::InvalidNameLen(len) => {
                write!(f, "invalid service name length: {}", len)
            }
            Self::InvalidUtf8 => write!(f, "service name is not valid UTF-8"),
            Self::PartialFrame(n) => {
                write!(f, "parital control frame ({} bytes)", n)
            }
        }
    }
}

/// Control errors encoding.
///
/// Control errors fall into two categories:
/// - `Io`: kernel level I/O failures
/// - `Invalid`: protocol level validation failures
#[derive(Debug)]
pub enum ControlError {
    Io(io::Error),
    InvalidCommand(ControlProtocolError),
}

impl From<io::Error> for ControlError {
    #[inline(always)]
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Control operations
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ControlOp {
    Stop = OP_STOP,
    Start = OP_START,
    Restart = OP_RESTART,
}

/// Command wire-format representation
///
/// TODO: We currently use the service name (as a string), because
/// writers have no way of knowing the internal service id.
/// Once a status file is maintained in tmpfs, we can switch to a
/// service id based protocol and delegate the `name -> id` lookup
/// to the writer
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct WireControlCommand {
    pub op: u8,
    pub name_len: u8,
    pub name: [u8; WIRE_COMMAND_SIZE - 2],
}

impl WireControlCommand {
    #[inline(always)]
    pub fn empty() -> Self {
        Self {
            op: 0,
            name_len: 0,
            name: [0u8; WIRE_COMMAND_SIZE - 2],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlCommand<'a> {
    pub op: ControlOp,
    pub name: &'a str,
}

impl<'a> TryFrom<&'a WireControlCommand> for ControlCommand<'a> {
    type Error = ControlProtocolError;

    fn try_from(value: &'a WireControlCommand) -> Result<Self, Self::Error> {
        let op = match value.op {
            OP_STOP => ControlOp::Stop,
            OP_START => ControlOp::Start,
            OP_RESTART => ControlOp::Restart,
            _ => {
                return Err(ControlProtocolError::InvalidOp(value.op));
            }
        };
        if value.name_len as usize > value.name.len() {
            return Err(ControlProtocolError::InvalidNameLen(value.name_len));
        }
        let name = str::from_utf8(&value.name[..value.name_len as usize])
            .map_err(|_| ControlProtocolError::InvalidUtf8)?;
        Ok(ControlCommand::new(op, name))
    }
}

impl<'a> ControlCommand<'a> {
    #[inline(always)]
    pub fn new(op: ControlOp, name: &'a str) -> Self {
        Self { op, name }
    }
}

/// Read a command from `fd`.
///
/// We only return the command when exactly `WIRE_COMMAND_SIZE` bytes
/// were read, so every byte of the `repr(C)` struct is initialized
/// regardless of what the writer sent. Semantic validation (opcode,
/// name_len, UTF-8) is deferred to `TryFrom`
pub fn read_control_command(
    fd: BorrowedFd<'_>,
) -> Result<Option<WireControlCommand>, ControlError> {
    let mut cmd = WireControlCommand {
        op: 0,
        name_len: 0,
        name: [0u8; WIRE_COMMAND_SIZE - 2],
    };
    let buf = unsafe {
        std::slice::from_raw_parts_mut(
            &mut cmd as *mut WireControlCommand as *mut u8,
            WIRE_COMMAND_SIZE,
        )
    };
    match rustix::io::read(fd, buf) {
        Ok(n) if n == WIRE_COMMAND_SIZE => Ok(Some(cmd)),
        Ok(0) => Ok(None), // should never happen as we keep the write end open
        Ok(n) => Err(ControlError::InvalidCommand(
            ControlProtocolError::PartialFrame(n),
        )),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(ControlError::Io(e.into())),
    }
}

/// Read commands from `fd` into `buf` and returns the number of
/// `WireControlCommand` read.
///
/// Only accept reads that are exact multiples of `WIRE_COMMAND_SIZE`, so
/// every `WireControlCommand` in the returned slice is fully initialized.
/// Semantic validation (opcode, name_len, UTF-8) is deferred to `TryFrom`
pub fn read_control_commands_batch(
    fd: BorrowedFd<'_>,
    buf: &mut [WireControlCommand],
) -> Result<usize, ControlError> {
    if buf.is_empty() {
        return Ok(0);
    }
    let bytes_buf = unsafe {
        std::slice::from_raw_parts_mut(
            buf.as_mut_ptr() as *mut u8,
            buf.len() * WIRE_COMMAND_SIZE,
        )
    };
    match rustix::io::read(fd, bytes_buf) {
        Ok(n) if n % WIRE_COMMAND_SIZE == 0 => Ok(n / WIRE_COMMAND_SIZE),
        Ok(0) => Ok(0), // should never happen as we keep the write end open
        Ok(n) => Err(ControlError::InvalidCommand(
            ControlProtocolError::PartialFrame(n),
        )),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
        Err(e) => Err(ControlError::Io(e.into())),
    }
}
