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
