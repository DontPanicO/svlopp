use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

use bitflags::bitflags;
use rustix::io;

use crate::utils::cvt;

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub struct SignalfdFlags: u32 {
        /// `SFD_NONBLOCK`
        const NONBLOCK = libc::SFD_NONBLOCK.cast_unsigned();

        /// `SFD_CLOEXEC`
        const CLOEXEC = libc::SFD_CLOEXEC.cast_unsigned();
    }
}

#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct SigSet {
    raw: libc::sigset_t,
}

impl SigSet {
    #[inline(always)]
    pub fn empty() -> io::Result<Self> {
        unsafe {
            let mut raw = std::mem::zeroed();
            cvt(libc::sigemptyset(&mut raw))?;
            Ok(Self { raw })
        }
    }

    /// Create a new `SigSet` from the current
    /// thread signal mask
    #[inline(always)]
    pub fn current() -> io::Result<Self> {
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
    pub fn add(&mut self, signal: i32) -> io::Result<()> {
        unsafe { cvt(libc::sigaddset(&mut self.raw, signal))? };
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn as_ptr(&self) -> *const libc::sigset_t {
        &self.raw
    }
}

pub fn block_thread_signals(sigset: &SigSet) -> io::Result<()> {
    unsafe {
        cvt(libc::sigprocmask(
            libc::SIG_BLOCK,
            sigset.as_ptr(),
            std::ptr::null_mut(),
        ))?;
    }
    Ok(())
}

pub fn set_thread_signal_mask(sigset: &SigSet) -> io::Result<()> {
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
pub fn signalfd(sigset: &SigSet, flags: SignalfdFlags) -> io::Result<OwnedFd> {
    unsafe {
        let fd = cvt(libc::signalfd(-1, sigset.as_ptr(), flags.bits() as _))?;
        Ok(OwnedFd::from_raw_fd(fd))
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct SignalfdSiginfo {
    raw: libc::signalfd_siginfo,
}

impl SignalfdSiginfo {
    /// Return the underlying ssi_signo (the
    /// signal number)
    #[inline(always)]
    pub const fn signal(&self) -> u32 {
        self.raw.ssi_signo
    }

    /// Return the underlying ssi_code (the
    /// signal code)
    #[inline(always)]
    pub const fn code(&self) -> i32 {
        self.raw.ssi_code
    }

    /// Return the underlying ssi_pid (the
    /// sender PID)
    #[inline(always)]
    pub const fn pid(&self) -> u32 {
        self.raw.ssi_pid
    }

    /// Return the underlying ssi_uid (the
    /// actual sender UID)
    #[inline(always)]
    pub const fn uid(&self) -> u32 {
        self.raw.ssi_uid
    }

    /// Create an empty `SignalfdSiginfo`.
    /// # Safety
    /// an empty `SignalfdSiginfo` contains uninitialized
    /// data and can only be used to get a mutable pointer
    /// with `as_mut_ptr`. This is intented for usage with
    /// `libc::read`, to which the mutable pointer is passed.
    /// Any other method call before a successfull call to
    /// `libc::read` is UB.
    #[inline(always)]
    pub unsafe fn emtpy() -> Self {
        Self {
            raw: unsafe { std::mem::zeroed() },
        }
    }

    #[inline(always)]
    pub(crate) fn as_mut_ptr(&mut self) -> *mut libc::signalfd_siginfo {
        &mut self.raw
    }
}

pub fn read_signalfd(
    fd: BorrowedFd<'_>,
) -> io::Result<Option<SignalfdSiginfo>> {
    let mut info = unsafe { SignalfdSiginfo::emtpy() };
    let n = unsafe {
        libc::read(
            fd.as_raw_fd(),
            info.as_mut_ptr() as *mut libc::c_void,
            std::mem::size_of::<libc::signalfd_siginfo>(),
        )
    };

    match cvt(n) {
        Ok(0) => Ok(None),
        Ok(sz)
            if sz as usize == std::mem::size_of::<libc::signalfd_siginfo>() =>
        {
            Ok(Some(info))
        }
        Ok(_) => Err(io::Errno::IO),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn read_signalfd_all(
    fd: BorrowedFd<'_>,
) -> io::Result<Vec<SignalfdSiginfo>> {
    let mut out = Vec::new();
    while let Some(info) = read_signalfd(fd)? {
        out.push(info);
    }
    Ok(out)
}
