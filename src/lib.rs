pub mod service;
pub mod signalfd;
pub mod timerfd;

pub trait IsRetCode: Copy {
    fn is_error(self) -> bool;
}

impl IsRetCode for i32 {
    #[inline(always)]
    fn is_error(self) -> bool {
        self == -1
    }
}

impl IsRetCode for isize {
    #[inline(always)]
    fn is_error(self) -> bool {
        self == -1
    }
}

pub fn cvt<T: IsRetCode>(ret: T) -> rustix::io::Result<T> {
    if ret.is_error() {
        let errno = unsafe { *libc::__errno_location() };
        Err(rustix::io::Errno::from_raw_os_error(errno))
    } else {
        Ok(ret)
    }
}
