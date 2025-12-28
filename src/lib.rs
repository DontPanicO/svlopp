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

/// The status of the supervisor. When a shutdown is requested
/// the supervior may not stop immediately since it has to
/// take care of any alive child process. For this reason
/// we don't break the loop immediately and instead we want to
/// set a flag (e.g. the state) to indicate that we want to
/// break it as soon as all child processes has been terminated.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum SupervisorState {
    #[default]
    Running,
    ShutdownRequested,
}
