use rustix::{
    io,
    time::{
        timerfd_create, timerfd_settime, Itimerspec, TimerfdFlags,
        TimerfdTimerFlags, Timespec,
    },
};
use std::os::fd::{OwnedFd, RawFd};

/// Conver a C like error code to a Rust error
#[inline(always)]
fn cvt(ret: i32) -> io::Result<i32> {
    if ret == -1 {
        let errno = unsafe { *libc::__errno_location() };
        Err(io::Errno::from_raw_os_error(errno))
    } else {
        Ok(ret)
    }
}

/// Create a new sigset and add signals to it
fn make_sigset(signals: &[i32]) -> io::Result<libc::sigset_t> {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        cvt(libc::sigemptyset(&mut set))?;
        for signal in signals {
            cvt(libc::sigaddset(&mut set, *signal))?;
        }
        Ok(set)
    }
}

/// Block signals in `sigset` for the current thread.
/// This is required when using `signalfd` to prevent
/// standard signal delivery
fn block_signals(sigset: &libc::sigset_t) -> io::Result<()> {
    unsafe {
        cvt(libc::sigprocmask(
            libc::SIG_BLOCK,
            sigset as *const _,
            std::ptr::null_mut(),
        ))?;
        Ok(())
    }
}

/// Create a signalfd associated with the signal set `sigset`.
fn create_signalfd(sigset: &libc::sigset_t) -> io::Result<RawFd> {
    unsafe {
        cvt(libc::signalfd(
            -1,
            sigset as *const _,
            libc::SFD_CLOEXEC | libc::SFD_NONBLOCK,
        ))
    }
}

fn create_timerfd_1s_periodic() -> io::Result<OwnedFd> {
    let fd = timerfd_create(
        rustix::time::TimerfdClockId::Monotonic,
        TimerfdFlags::CLOEXEC.union(TimerfdFlags::NONBLOCK),
    )?;
    let new_value = Itimerspec {
        it_interval: Timespec {
            tv_sec: 1,
            tv_nsec: 1,
        },
        it_value: Timespec {
            tv_sec: 1,
            tv_nsec: 1,
        },
    };
    timerfd_settime(&fd, TimerfdTimerFlags::empty(), &new_value)?;
    Ok(fd)
}

fn main() {
    println!("Hello, world!");
}
