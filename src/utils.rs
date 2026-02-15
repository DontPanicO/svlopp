// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::{io, os::fd::BorrowedFd};

pub trait RetCode: Copy {
    fn is_error(self) -> bool;
}

impl RetCode for i32 {
    #[inline(always)]
    fn is_error(self) -> bool {
        self == -1
    }
}

impl RetCode for isize {
    #[inline(always)]
    fn is_error(self) -> bool {
        self == -1
    }
}

pub fn cvt<T: RetCode>(ret: T) -> rustix::io::Result<T> {
    if ret.is_error() {
        let errno = unsafe { *libc::__errno_location() };
        Err(rustix::io::Errno::from_raw_os_error(errno))
    } else {
        Ok(ret)
    }
}

#[inline(always)]
pub fn is_crash_signal(sig: i32) -> bool {
    matches!(
        sig,
        libc::SIGSEGV
            | libc::SIGABRT
            | libc::SIGFPE
            | libc::SIGILL
            | libc::SIGBUS
    )
}

#[inline(always)]
pub fn write_all(fd: BorrowedFd<'_>, mut buf: &[u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let n = rustix::io::write(fd, buf)?;
        buf = &buf[n..];
    }
    Ok(())
}
