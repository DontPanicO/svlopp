// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{
    io,
    os::fd::{BorrowedFd, FromRawFd, OwnedFd},
};

use bitflags::bitflags;

use crate::utils::cvt;

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub(crate) struct SignalfdFlags: u32 {
        /// `SFD_NONBLOCK`
        const NONBLOCK = libc::SFD_NONBLOCK.cast_unsigned();

        /// `SFD_CLOEXEC`
        const CLOEXEC = libc::SFD_CLOEXEC.cast_unsigned();
    }
}

#[repr(transparent)]
#[derive(Debug, Clone)]
pub(crate) struct SigSet {
    raw: libc::sigset_t,
}

impl SigSet {
    #[inline(always)]
    pub(crate) fn empty() -> rustix::io::Result<Self> {
        unsafe {
            let mut raw = std::mem::zeroed();
            cvt(libc::sigemptyset(&mut raw))?;
            Ok(Self { raw })
        }
    }

    /// Create a new `SigSet` from the current
    /// thread signal mask
    #[inline(always)]
    pub(crate) fn current() -> rustix::io::Result<Self> {
        unsafe {
            let mut raw = std::mem::zeroed();
            cvt(libc::sigprocmask(
                libc::SIG_SETMASK,
                std::ptr::null(),
                &mut raw,
            ))?;
            Ok(Self { raw })
        }
    }

    #[inline(always)]
    pub(crate) fn add(&mut self, signal: i32) -> rustix::io::Result<()> {
        unsafe { cvt(libc::sigaddset(&mut self.raw, signal))? };
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn as_ptr(&self) -> *const libc::sigset_t {
        &self.raw
    }
}

pub(crate) fn block_thread_signals(sigset: &SigSet) -> rustix::io::Result<()> {
    unsafe {
        cvt(libc::sigprocmask(
            libc::SIG_BLOCK,
            sigset.as_ptr(),
            std::ptr::null_mut(),
        ))?;
    }
    Ok(())
}

pub(crate) fn set_thread_signal_mask(sigset: &SigSet) -> rustix::io::Result<()> {
    unsafe {
        cvt(libc::sigprocmask(
            libc::SIG_SETMASK,
            sigset.as_ptr(),
            std::ptr::null_mut(),
        ))?;
    }
    Ok(())
}

/// TODO: we're hardcoding fd to be -1, causing `signalfd` to only ask for
/// a new file descriptor
pub(crate) fn signalfd(sigset: &SigSet, flags: SignalfdFlags) -> rustix::io::Result<OwnedFd> {
    unsafe {
        let fd = cvt(libc::signalfd(-1, sigset.as_ptr(), flags.bits() as _))?;
        Ok(OwnedFd::from_raw_fd(fd))
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub(crate) struct SignalfdSiginfo {
    raw: libc::signalfd_siginfo,
}

#[allow(dead_code)]
impl SignalfdSiginfo {
    /// Return the underlying ssi_signo (the
    /// signal number)
    #[inline(always)]
    pub(crate) const fn signal(&self) -> u32 {
        self.raw.ssi_signo
    }

    /// Return the underlying ssi_code (the
    /// signal code)
    #[inline(always)]
    pub(crate) const fn code(&self) -> i32 {
        self.raw.ssi_code
    }

    /// Return the underlying ssi_pid (the
    /// sender PID)
    #[inline(always)]
    pub(crate) const fn pid(&self) -> u32 {
        self.raw.ssi_pid
    }

    /// Return the underlying ssi_uid (the
    /// actual sender UID)
    #[inline(always)]
    pub(crate) const fn uid(&self) -> u32 {
        self.raw.ssi_uid
    }

    /// Create an empty `SignalfdSiginfo`.
    /// # Safety
    /// an empty `SignalfdSiginfo` contains uninitialized
    /// data and can only be used to get a mutable pointer
    /// with `as_mut_ptr`. This is intended for usage with
    /// `libc::read`, to which the mutable pointer is passed.
    /// Any other method call before a successful call to
    /// `libc::read` is UB.
    #[inline(always)]
    pub(crate) fn empty() -> Self {
        Self {
            raw: unsafe { std::mem::zeroed() },
        }
    }

    #[inline(always)]
    pub(crate) fn as_mut_ptr(&mut self) -> *mut libc::signalfd_siginfo {
        &mut self.raw
    }
}

/// Read up to `buf.len()` `SignalfdSiginfo` from `fd` and
/// return the number of items read. Return `Ok(0)` if reading
/// from `fd` would block (`WOULDBLOCK`).
///
/// N.B. this may not drain the fd. This is fine as we're
/// using level-triggered epoll, which will fire again on
/// the next call to wait
pub(crate) fn read_signalfd_batch(
    fd: BorrowedFd<'_>,
    buf: &mut [SignalfdSiginfo],
) -> rustix::io::Result<usize> {
    if buf.is_empty() {
        return Ok(0);
    }
    let bytes_buf = unsafe {
        std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, std::mem::size_of_val(buf))
    };
    match rustix::io::read(fd, bytes_buf) {
        Ok(0) => Ok(0),
        Ok(sz) if sz % std::mem::size_of::<SignalfdSiginfo>() == 0 => {
            Ok(sz / std::mem::size_of::<SignalfdSiginfo>())
        }
        Ok(_) => Err(rustix::io::Errno::IO),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
        Err(e) => Err(e),
    }
}
