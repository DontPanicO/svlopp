use std::collections::HashMap;
use std::ffi::CString;
use std::io;

use rustix::{
    fs::{open, OFlags},
    process::{kill_process, wait, Pid, Signal, WaitOptions, WaitStatus},
    stdio::{dup2_stderr, dup2_stdin, dup2_stdout},
};
use serde::Deserialize;

use crate::{
    signalfd::{set_thread_signal_mask, SigSet},
    utils::is_crash_signal,
};

/// Process exit reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExitReason {
    /// Process exited with a status code
    Exited(i32),
    /// Process signaled
    Signaled(i32),
}

impl ExitReason {
    pub fn from_wait_status(status: WaitStatus) -> Option<Self> {
        if status.exited() {
            Some(Self::Exited(status.exit_status().unwrap_or(-1)))
        } else if status.signaled() {
            Some(Self::Signaled(status.terminating_signal().unwrap()))
        } else {
            None
        }
    }
}

/// Service stop reason. It differs from `ExitReason` by being
/// specific to supervisor logic and abstraction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServiceStopReason {
    /// Service has never started
    #[default]
    NeverStarted,
    /// Service has been gracefully terminated by
    /// the supervisor
    SupervisorTerminated,
    /// Service successfully completed (i.e.
    /// exited with code == 0)
    Success,
    /// Service completed with an error (i.e.
    /// exited with code != 0)
    Error(i32),
    /// Service terminated due to a fault signal
    Crashed(i32),
    /// Service terminated due to any other signal.
    /// Notice that "any other signal" may be handled
    /// by the child process, which may call `exit`,
    /// leading to either `Self::Success` or
    /// `Self::Error(code)`
    Killed(i32),
}

impl ServiceStopReason {
    pub fn from_exit_reason_and_service_state(
        exit_reason: ExitReason,
        svc_state: ServiceState,
    ) -> Self {
        match exit_reason {
            ExitReason::Exited(code) => {
                if matches!(svc_state, ServiceState::Stopping) {
                    Self::SupervisorTerminated
                } else if code == 0 {
                    Self::Success
                } else {
                    Self::Error(code)
                }
            }
            ExitReason::Signaled(sig) => {
                if is_crash_signal(sig) {
                    Self::Crashed(sig)
                } else if matches!(svc_state, ServiceState::Stopping) {
                    Self::SupervisorTerminated
                } else {
                    Self::Killed(sig)
                }
            }
        }
    }
}

/// All possible states in which a service
/// can be at any moment
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServiceState {
    /// The service is stopped.
    /// This is the initial state for all
    /// services.
    Stopped(ServiceStopReason),
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

impl Default for ServiceState {
    fn default() -> ServiceState {
        ServiceState::Stopped(ServiceStopReason::NeverStarted)
    }
}

/// Service configuration
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ServiceConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

impl ServiceConfig {
    fn build_svc_argv(&self) -> io::Result<Vec<CString>> {
        let mut argv = Vec::with_capacity(self.args.len() + 1);
        argv.push(CString::new(self.command.as_str()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "command contains NUL byte",
            )
        })?);
        for arg in &self.args {
            argv.push(CString::new(arg.as_str()).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "argument contains NUL byte",
                )
            })?);
        }
        Ok(argv)
    }
}

/// The content of the services config file.
///
/// As of now, we are working with a single toml file
/// to define service configs. This struct is used
/// to deserialized a `HashMap<String, ServiceConfig>` from that
/// file, where the string represent the service name.
#[repr(transparent)]
#[derive(Debug, Deserialize)]
pub struct ServiceConfigData {
    pub services: HashMap<String, ServiceConfig>,
}

impl ServiceConfigData {
    #[inline(always)]
    pub fn from_config_file(path: &str) -> io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| io::Error::other(e.message()))
    }
}

/// Generate progressive service ids.
///
/// Service ids are `u64`, but we want to support
/// up to just 65336 services for now hence this
/// holds an `u16`.
#[repr(transparent)]
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceIdGen(u16);

impl ServiceIdGen {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns an `Option` with the currently holded
    /// value - zero extended to 64-bit - and increment
    /// it by one. If incrementing causes an overflo and increment
    /// it by one. If incrementing causes an overflow
    /// it returns `None`
    #[inline(always)]
    pub fn nextval(&mut self) -> Option<u64> {
        let value = self.0 as u64;
        self.0 = self.0.checked_add(1)?;
        Some(value)
    }
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
    pub config: ServiceConfig,
    pub argv: Vec<CString>,
    pub pid: Option<Pid>,
    pub state: ServiceState,
}

impl Service {
    #[inline(always)]
    pub fn new(
        id: u64,
        name: String,
        config: ServiceConfig,
    ) -> io::Result<Self> {
        let argv = config.build_svc_argv()?;
        Ok(Self {
            id,
            name,
            config,
            argv,
            pid: None,
            state: ServiceState::Stopped(ServiceStopReason::NeverStarted),
        })
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

    #[inline(always)]
    pub fn is_stopped(&self) -> bool {
        matches!(self.state, ServiceState::Stopped(_))
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
pub fn start_service(svc: &mut Service, sigset: &SigSet) -> io::Result<()> {
    match unsafe { libc::fork() } {
        0 => {
            set_thread_signal_mask(sigset)?;
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
            // safe as we just checked that the pid is > 0
            let pid = unsafe { Pid::from_raw_unchecked(raw) };
            svc.pid = Some(pid);
            svc.state = ServiceState::Running;
            Ok(())
        }
        _ => Err(io::Error::last_os_error()),
    }
}

/// Stop a service by calling `kill(pid, SIGTERM)` and marks it as
/// stopping by setting state to `ServiceState::Stopping`.
pub fn stop_service(svc: &mut Service) -> io::Result<()> {
    if svc.state != ServiceState::Running {
        return Ok(());
    }
    let pid = match svc.pid {
        Some(p) => p,
        None => {
            eprintln!("service '{}' is running but has no pid", svc.name,);
            return Ok(());
        }
    };
    kill_process(pid, Signal::TERM)?;
    svc.state = ServiceState::Stopping;
    Ok(())
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

    /// Get a shared reference to the service correspondig to
    /// `svc_id` if it exists in the `service_id -> service` map
    #[inline(always)]
    pub fn service(&self, svc_id: u64) -> Option<&Service> {
        self.services_map.get(&svc_id)
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

    /// Get a shared reference to the service corresponding to pid.
    #[inline(always)]
    pub fn get_by_pid(&self, pid: Pid) -> Option<&Service> {
        let svc_id = self.pids_map.get(&pid)?;
        self.services_map.get(svc_id)
    }

    /// Remove `pid` from the `pid -> service_id` map if exists and
    /// return a mutable reference to the correspondind `Service` in
    /// the `service_id -> service` map
    #[inline(always)]
    pub fn take_by_pid(&mut self, pid: Pid) -> Option<&mut Service> {
        let svc_id = self.pids_map.remove(&pid)?;
        self.services_map.get_mut(&svc_id)
    }

    #[inline(always)]
    pub fn services(
        &self,
    ) -> std::collections::hash_map::Values<'_, u64, Service> {
        self.services_map.values()
    }

    #[inline(always)]
    pub fn services_mut(
        &mut self,
    ) -> std::collections::hash_map::ValuesMut<'_, u64, Service> {
        self.services_map.values_mut()
    }
}

/// Parse the configuration file and apply changes
///
/// For new services (not present in the registry, but present in the new
/// config), insert it in the registry and starts it.
///
/// For removed services (present in the registry, but not present in the
/// new config):
/// * If the service state is `ServiceState::Stopped(_)`: remove the service
///   from the registry immediately.
/// * If `ServiceState::Running`: call `stop_service` and mark for removal so
///   that the SIGCHLD handler can remove it once stopped.
/// * If `ServiceState::Stopping`: just mark for removal.
///
/// For changed services (present in both the registry and the new config
/// but with different configurations):
/// * If the service state is `ServiceState::Stopped(_)`: update the config,
///   rebuild argv, then start it.
/// * If `ServiceState::Running`: call `stop_service`, store new config and
///   mark for restart so that when the SIGCHLD handler reaps the process it
///   can restart it.
/// * if `ServiceState::Stopping`: just store the new config and mark for
///   restart.
pub fn reload_services(
    registry: &mut ServiceRegistry,
    cfg_path: &str,
) -> io::Result<()> {
    let service_configs = ServiceConfigData::from_config_file(cfg_path)?;
    let mut service_ids = HashMap::new();
    for svc in registry.services() {
        service_ids.insert(svc.name.clone(), svc.id);
    }
    for name in service_ids.keys() {
        if !service_configs.services.contains_key(name) {
            eprintln!("removig service {}", name);
        }
    }

    for (name, cfg) in service_configs.services.iter() {
        match service_ids.get(name) {
            None => eprintln!("adding new service {}", name),
            Some(&svc_id) => {
                let svc = registry.service(svc_id).unwrap();
                if svc.config != *cfg {
                    eprintln!("updating service {}", name);
                }
            }
        }
    }
    Ok(())
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
                if let Some(exit_reason) = ExitReason::from_wait_status(status)
                {
                    match registry.take_by_pid(pid) {
                        Some(svc) => {
                            svc.pid = None;
                            let stop_reason = ServiceStopReason::from_exit_reason_and_service_state(exit_reason, svc.state);
                            svc.state = ServiceState::Stopped(stop_reason);
                            eprintln!(
                                "service '{}' exit: {:?}",
                                svc.name, exit_reason,
                            )
                        }
                        None => eprintln!("`waitpid` got unknown pid: {}", pid),
                    }
                } else {
                    match registry.get_by_pid(pid) {
                        Some(svc) => {
                            eprintln!(
                                "service '{}' exited with status {:?}",
                                svc.name, status
                            )
                        }
                        None => eprintln!("`waitpid` got unknown pid: {}", pid),
                    }
                }
            }
            Ok(None) => break, // no more childs ready
            Err(rustix::io::Errno::CHILD) => break, // no child
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
