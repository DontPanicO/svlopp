use std::{fmt, sync::atomic::AtomicU8, sync::atomic::Ordering};

use crate::utils::timestamp;

static LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[repr(u8)]
pub(crate) enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}

pub(crate) fn set_log_level(level: LogLevel) {
    LOG_LEVEL.store(level as u8, Ordering::Relaxed);
}

pub(crate) fn log_inner(level: LogLevel, msg: fmt::Arguments<'_>) {
    if level as u8 > LOG_LEVEL.load(Ordering::Relaxed) {
        return;
    }
    let (secs, nsecs) = timestamp();
    eprintln!("[{}.{}][{:?}] {}", secs, nsecs, level, msg);
}

#[macro_export]
macro_rules! svlogg {
    ($lvl:expr, $($arg:tt)*) => {
        $crate::logging::log_inner($lvl, format_args!($($arg)*))
    };
}
