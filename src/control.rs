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
const WIRE_COMMAND_SIZE: usize = 9;

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
    PartialFrame(usize),
}

impl std::fmt::Display for ControlProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOp(op) => write!(f, "invalid opcode: 0x{:02x}", op),
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
#[derive(Debug, Clone, Copy)]
pub struct ControlCommand {
    pub op: ControlOp,
    pub service_id: u64,
}

impl ControlCommand {
    #[inline(always)]
    pub fn new(op: ControlOp, service_id: u64) -> Self {
        Self { op, service_id }
    }
}

/// Read a command from `fd`.
///
/// TODO: with 9-byte frames, a signle pipe buffer can hold thousands of
/// commands. Currently we read one command per epoll wake (not losing them
/// since it's level-triggered): if that becomes a bottleneck we could
/// add a `read_control_command_batch` function to consume more commands
/// per iteration
pub fn read_control_command(
    fd: BorrowedFd<'_>,
) -> Result<Option<ControlCommand>, ControlError> {
    let mut buf = [0u8; WIRE_COMMAND_SIZE];
    match rustix::io::read(fd, &mut buf) {
        Ok(n) if n == WIRE_COMMAND_SIZE => {
            let op = match buf[0] {
                OP_STOP => ControlOp::Stop,
                OP_START => ControlOp::Start,
                OP_RESTART => ControlOp::Restart,
                other => {
                    return Err(ControlError::InvalidCommand(
                        ControlProtocolError::InvalidOp(other),
                    ));
                }
            };
            let mut svc_id_bytes = [0u8; 8];
            svc_id_bytes.copy_from_slice(&buf[1..9]);
            Ok(Some(ControlCommand::new(
                op,
                u64::from_le_bytes(svc_id_bytes),
            )))
        }
        Ok(0) => Ok(None), // should never happen as we keep the write end open
        Ok(n) => Err(ControlError::InvalidCommand(
            ControlProtocolError::PartialFrame(n),
        )),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(ControlError::Io(e.into())),
    }
}
