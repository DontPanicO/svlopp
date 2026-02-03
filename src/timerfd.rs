// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::os::fd::{BorrowedFd, OwnedFd};

use rustix::time::{
    Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec,
    timerfd_create, timerfd_settime,
};

pub fn create_timerfd_1s_periodic() -> rustix::io::Result<OwnedFd> {
    let fd = timerfd_create(
        TimerfdClockId::Monotonic,
        TimerfdFlags::CLOEXEC | TimerfdFlags::NONBLOCK,
    )?;
    let new_value = Itimerspec {
        it_interval: Timespec {
            tv_sec: 1,
            tv_nsec: 0,
        },
        it_value: Timespec {
            tv_sec: 1,
            tv_nsec: 0,
        },
    };
    timerfd_settime(&fd, TimerfdTimerFlags::empty(), &new_value)?;
    Ok(fd)
}

pub fn read_timerfd(fd: BorrowedFd<'_>) -> rustix::io::Result<u64> {
    let mut buf = [0u8; 8];
    let n = rustix::io::read(fd, &mut buf)?;
    if n != 8 {
        return Err(rustix::io::Errno::IO);
    }
    Ok(u64::from_ne_bytes(buf))
}
