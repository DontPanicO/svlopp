use std::collections::HashMap;
use std::ffi::CString;
use std::io;

use rustix::process::Pid;

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

/// Start a new service.
///
/// a successful call to `fork` return `0` in the child process
/// and the child pid in the parent. Negative values (`-1`)
/// indicate an error.
/// `execvp` is used as we don't know the exact lenght of `argv`
/// and of course we want it to check for the executable in path
pub fn start_service(svc: &mut Service) -> io::Result<()> {
    match unsafe { libc::fork() } {
        0 => {
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
