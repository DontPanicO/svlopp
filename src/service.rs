use std::collections::HashMap;
use std::ffi::CString;
use std::io;

use rustix::{
    fs::{open, OFlags},
    process::{wait, Pid, WaitOptions},
    stdio::{dup2_stderr, dup2_stdin, dup2_stdout},
};

/// All possible states in which a service
/// can be at any moment
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServiceState {
    /// The service is stopped.
    /// This is the initial state for all
    /// services.
    #[default]
    Stopped,
    /// The service is starting.
    /// This is the state in which a stopped
    /// service is put after receiving a
    /// startup request.
    Starting,
    /// The serivce has been started and
    /// is now running.
    Running,
    /// The service is stopping.
    /// Services are put in this state after
    /// a stop request, which can be issued
    /// from different actors and in
    /// different forms.
    Stopping,
}

/// A minimal service representation.
/// TODO: whether we expect `pid` to be `Some`
/// it's strictly related to the service state.
/// We might want to couple this in some way
/// (e.g. include the `pid` in the state instead
/// of having it as a separate parameter)
#[derive(Debug, Clone)]
pub struct Service {
    pub id: u64,
    pub name: String,
    pub argv: Vec<CString>,
    pub pid: Option<Pid>,
    pub state: ServiceState,
}

impl Service {
    #[inline(always)]
    pub fn new(id: u64, name: String, argv: Vec<CString>) -> Self {
        Self {
            id,
            name,
            argv,
            pid: None,
            state: ServiceState::Stopped,
        }
    }

    #[inline(always)]
    pub fn set_pid(&mut self, pid: Pid) {
        self.pid = Some(pid);
    }

    /// TODO: we might want logic to enforce some contract (e.g.
    /// a state machine) instead of letting the caller set
    /// an arbitrary value for state
    #[inline(always)]
    pub fn set_state(&mut self, state: ServiceState) {
        self.state = state;
    }
}

/// Redirect stdio fds to /dev/null.
///
/// Used to avoid polluting the main process output with the one of its
/// children
fn redirect_stdio_to_devnull() -> rustix::io::Result<()> {
    let fd = open("/dev/null", OFlags::RDWR, rustix::fs::Mode::empty())?;
    dup2_stdin(&fd)?;
    dup2_stdout(&fd)?;
    dup2_stderr(&fd)?;
    Ok(())
}

/// Start a new service.
///
/// a successful call to `fork` return `0` in the child process
/// and the child pid in the parent. Negative values (`-1`)
/// indicate an error.
/// `execvp` is used as we don't know the exact lenght of `argv`
/// and of course we want it to check for the executable in path
///
/// TODO: Currently we're redirecting `/dev/std*` to dev null
/// in the child processes, but we have to decide what to do
/// with it
pub fn start_service(svc: &mut Service) -> io::Result<()> {
    match unsafe { libc::fork() } {
        0 => {
            redirect_stdio_to_devnull()?;
            let argv: Vec<*const libc::c_char> = svc
                .argv
                .iter()
                .map(|s| s.as_ptr())
                .chain(Some(std::ptr::null()))
                .collect();

            unsafe {
                libc::execvp(argv[0], argv.as_ptr());
                libc::_exit(127);
            };
        }
        raw if raw > 0 => {
            // safe as we just check that the pid is > 0
            let pid = unsafe { Pid::from_raw_unchecked(raw) };
            svc.pid = Some(pid);
            svc.state = ServiceState::Running;
            Ok(())
        }
        _ => Err(io::Error::last_os_error()),
    }
}

/// The services registry.
///
/// Holds all the services in the form of
/// two hashmaps:
/// 1. `service_id -> service` to lookup
///    services fast via their id.
/// 2. `pid -> service_id` to get a service_id
///    from a pid.
///
/// Services are loaded into `service_id -> service` as
/// soon as they're discovered (e.g. when deserializing
/// from config files) and pid association are inserted
/// in `pid -> service_id` after the child process has
/// successfully started.
#[derive(Debug, Clone, Default)]
pub struct ServiceRegistry {
    /// `service_id -> service`
    services_map: HashMap<u64, Service>,
    /// `pid -> service_id`
    pids_map: HashMap<Pid, u64>,
}

impl ServiceRegistry {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a new service in the `service_id -> service` map
    #[inline(always)]
    pub fn insert_service(&mut self, svc: Service) {
        self.services_map.insert(svc.id, svc);
    }

    /// Get a mutable reference to the service corresponding to
    /// `svc_id` if it exists in the `service_id -> service` map
    #[inline(always)]
    pub fn service_mut(&mut self, svc_id: u64) -> Option<&mut Service> {
        self.services_map.get_mut(&svc_id)
    }

    /// Insert a new pid in the `pid -> service_id` map.
    /// The caller is responsible to maintain the consistency and must
    /// make sure to have properly updated the `service.pid` to
    /// `Some(pid)` for the `Service` corresponding to `svc_id`
    #[inline(always)]
    pub fn register_pid(&mut self, pid: Pid, svc_id: u64) {
        self.pids_map.insert(pid, svc_id);
    }

    /// Remove `pid` from the `pid -> service_id` map if exists and
    /// return a mutable reference to the correspondind `Service` in
    /// the `service_id -> service` map
    #[inline(always)]
    pub fn take_by_pid(&mut self, pid: Pid) -> Option<&mut Service> {
        let svc_id = self.pids_map.remove(&pid)?;
        self.services_map.get_mut(&svc_id)
    }
}

/// Used to generate progressive service ids.
///
/// Service ids are `u64`, but we want to support
/// up to 65536 services hence this holds an
/// `u16`
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceIdGen(u16);

impl ServiceIdGen {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the currently holded value - zero extended
    /// to 64-bit - and increment it by one
    #[inline(always)]
    pub fn nextval(&mut self) -> Option<u64> {
        let value = self.0 as u64;
        self.0 = self.0.checked_add(1)?;
        Some(value)
    }
}

/// SIGCHLD handler
///
/// **N.B.** `rustix::process::wait` correspond to `waitpid(-1, ...)`, the syscall
/// used - with that particular value as pid - to wait for *any* child process and
/// shall not be confused with a call to `wait`.
/// While `waitpid(-1, ...)` is equivalent to `wait` in the fact that it blocks
/// until status informtion are available for *any* child process, `waitpid` enable
/// the caller to specify options. Here we're using `WNOHANG` to avoid actually blocking
/// if no status information is available immediately when calling. In this way
/// `waitpid(-1, ...)` differs completely from `wait`
pub fn handle_sigchld(registry: &mut ServiceRegistry) -> io::Result<()> {
    loop {
        match wait(WaitOptions::NOHANG) {
            Ok(Some((pid, status))) => {
                if let Some(svc) = registry.take_by_pid(pid) {
                    svc.pid = None;
                    svc.state = ServiceState::Stopped;
                    if status.exited() {
                        eprintln!(
                            "service '{}' exited normally with code {}",
                            svc.name,
                            status.exit_status().unwrap_or(-1)
                        );
                    } else if status.signaled() {
                        eprintln!(
                            "service '{}' terminated by signal {}",
                            svc.name,
                            status.terminating_signal().unwrap()
                        )
                    } else {
                        eprintln!(
                            "service '{}' exited with status {:?}",
                            svc.name, status
                        )
                    }
                } else {
                    eprintln!("`waitpid` got unknown pid: {}", pid);
                }
            }
            Ok(None) => break, // no more childs ready
            Err(rustix::io::Errno::CHILD) => break, // no child
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
