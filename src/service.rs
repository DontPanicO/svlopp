// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::ffi::CString;
use std::fmt;
use std::io;
use std::os::fd::AsFd;
use std::os::fd::BorrowedFd;
use std::path::Path;
use std::path::PathBuf;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use rustix::{
    fs::{Mode, OFlags, open},
    process::{Pid, Signal, WaitOptions, WaitStatus, chdir, kill_process, wait},
    stdio::{dup2_stderr, dup2_stdin, dup2_stdout},
};
use serde::Deserialize;

use crate::control::ControlOp;
use crate::logging::LogLevel;
use crate::svlogg;
use crate::utils::cvt;
use crate::{
    signalfd::{SigSet, set_thread_signal_mask},
    utils::is_crash_signal,
};

/// Default graceful shutdown timeout in milliseconds
const DEFAULT_STOP_TIMEOUT_MS: u64 = 5000;

fn default_stop_timeout_ms() -> u64 {
    DEFAULT_STOP_TIMEOUT_MS
}

/// Process exit reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ExitReason {
    /// Process exited with a status code
    Exited(i32),
    /// Process signaled
    Signaled(i32),
}

impl std::fmt::Display for ExitReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exited(i) => write!(f, "exited({})", i),
            Self::Signaled(i) => write!(f, "signaled({})", i),
        }
    }
}

impl ExitReason {
    pub(crate) fn from_wait_status(status: WaitStatus) -> Option<Self> {
        if status.exited() {
            Some(Self::Exited(
                status.exit_status().expect("`exited()` returned `true`"),
            ))
        } else if status.signaled() {
            Some(Self::Signaled(
                status
                    .terminating_signal()
                    .expect("`signaled()` returned `true`"),
            ))
        } else {
            None
        }
    }
}

/// Service stop reason. It differs from `ExitReason` by being
/// specific to supervisor logic and abstraction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ServiceStopReason {
    /// Service has never started
    #[default]
    NeverStarted,
    /// Service has been gracefully terminated by
    /// the supervisor
    SupervisorTerminated(ExitReason),
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

impl fmt::Display for ServiceStopReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeverStarted => write!(f, "never_started"),
            Self::SupervisorTerminated(er) => write!(f, "supervisor_terminated({})", er),
            Self::Success => write!(f, "success"),
            Self::Error(e) => write!(f, "error({})", e),
            Self::Crashed(s) => write!(f, "crashed({})", s),
            Self::Killed(s) => write!(f, "killed({})", s),
        }
    }
}

impl ServiceStopReason {
    pub(crate) fn from_exit_reason_and_service_state(
        exit_reason: ExitReason,
        svc_state: ServiceState,
    ) -> Self {
        match exit_reason {
            ExitReason::Exited(code) => {
                if matches!(svc_state, ServiceState::Stopping(_, _)) {
                    Self::SupervisorTerminated(exit_reason)
                } else if code == 0 {
                    Self::Success
                } else {
                    Self::Error(code)
                }
            }
            ExitReason::Signaled(sig) => {
                if is_crash_signal(sig) {
                    Self::Crashed(sig)
                } else if matches!(svc_state, ServiceState::Stopping(_, _)) {
                    Self::SupervisorTerminated(exit_reason)
                } else {
                    Self::Killed(sig)
                }
            }
        }
    }
}

/// All possible states in which a service
/// can be at any moment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ServiceState {
    /// The service is stopped.
    /// This is the initial state for all
    /// services
    Stopped(ServiceStopReason),
    /// The service has been started and
    /// is now running
    Running(Pid),
    /// The service is stopping.
    /// Services are put in this state after
    /// a stop request, which can be issued
    /// from different actors and in
    /// different forms
    Stopping(Pid, Instant),
}

impl Default for ServiceState {
    fn default() -> ServiceState {
        ServiceState::Stopped(ServiceStopReason::NeverStarted)
    }
}

impl fmt::Display for ServiceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped(r) => write!(f, "stopped {}", r),
            Self::Running(p) => write!(f, "running {}", p.as_raw_nonzero()),
            Self::Stopping(p, _) => write!(f, "stopping {}", p.as_raw_nonzero()),
        }
    }
}

/// Signals allowed for graceful service termination.
///
/// The names mirror the traditional POSIX `SIG*` names so that the
/// configuration can use familiar values such as `SIGTERM` or `SIGQUIT`.
#[derive(Debug, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum StopSignal {
    #[default]
    SigTerm,
    SigInt,
    SigQuit,
    SigHup,
    SigUsr1,
    SigUsr2,
}

impl From<StopSignal> for Signal {
    fn from(value: StopSignal) -> Self {
        match value {
            StopSignal::SigTerm => Signal::TERM,
            StopSignal::SigInt => Signal::INT,
            StopSignal::SigQuit => Signal::QUIT,
            StopSignal::SigHup => Signal::HUP,
            StopSignal::SigUsr1 => Signal::USR1,
            StopSignal::SigUsr2 => Signal::USR2,
        }
    }
}

/// User and group identifiers for a service process
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UserGroup {
    pub(crate) uid: u32,
    pub(crate) gid: u32,
}

/// Service configuration
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct ServiceConfig {
    /// Path to the binary or binary name if in `PATH`
    pub(crate) command: String,
    /// Binary arguments
    #[serde(default)]
    pub(crate) args: Vec<String>,
    /// Optional environment to replace the parent one
    #[serde(default)]
    pub(crate) env: Option<HashMap<String, String>>,
    /// Optional working directory for the service process.
    /// If `None` the service inherits the current working directory
    #[serde(default)]
    pub(crate) working_directory: Option<PathBuf>,
    /// Optional file to redirect the service `stdout` and
    /// `stderr` to.
    /// If `None` they are piped to `/dev/null`
    #[serde(default)]
    pub(crate) log_file_path: Option<PathBuf>,
    /// Optional `uid` and `gid` for the service process.
    #[serde(default)]
    pub(crate) user_group: Option<UserGroup>,
    /// Fallback pending action to take.
    /// This allows to define restart behavior (e.g.
    /// if a service exits, restart it), but only
    /// as a fallback to `Service::pending_action` as
    /// reload takes precedence.
    /// This is ignored if a service is stopped with
    /// reason `ServiceStopReason::SupervisorTerminated`
    #[serde(rename = "on_exit")]
    #[serde(default)]
    pub(crate) fallback_pending_action: ServicePendingAction,
    /// Signal sent to a service process to gracefully stop it.
    /// Valid options are: `SIGTERM` (default), `SIGINT`, `SIGQUIT`,
    /// `SIGHUP`, `SIGUSR1`, `SIGUSR2`.
    #[serde(default)]
    pub(crate) stop_signal: StopSignal,
    /// Timeout in milliseconds between `stop_signal` and `SIGKILL` when
    /// stopping the service. Defaults to 5000
    #[serde(default = "default_stop_timeout_ms")]
    pub(crate) stop_timeout_ms: u64,
}

impl ServiceConfig {
    fn build_svc_argv(&self) -> io::Result<Vec<CString>> {
        let mut argv = Vec::with_capacity(self.args.len() + 1);
        argv.push(CString::new(self.command.as_str())?);
        for arg in &self.args {
            argv.push(CString::new(arg.as_str())?);
        }
        Ok(argv)
    }

    fn build_svc_envp(&self) -> io::Result<Option<Vec<CString>>> {
        match &self.env {
            None => Ok(None),
            Some(map) => {
                let mut envp = Vec::with_capacity(map.len());
                for (key, value) in map {
                    // TODO: maybe validate that it doesn't contain `=`?
                    envp.push(CString::new(format!("{key}={value}"))?);
                }
                Ok(Some(envp))
            }
        }
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
pub(crate) struct ServiceConfigData {
    pub(crate) services: HashMap<String, ServiceConfig>,
}

impl ServiceConfigData {
    #[inline(always)]
    pub(crate) fn from_config_file(path: &Path) -> io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| io::Error::other(e.message()))
    }
}

/// Generate progressive service ids.
#[repr(transparent)]
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ServiceIdGen(u64);

impl ServiceIdGen {
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub(crate) fn nextval(&mut self) -> Option<u64> {
        let value = self.0;
        self.0 = self.0.checked_add(1)?;
        Some(value)
    }
}

/// Pending action to be executed when a service stops.
///
/// Used during reload to defer actions for services that are
/// still running
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub(crate) enum ServicePendingAction {
    /// No pending action to perform
    #[default]
    None,
    /// Service have to be started again
    Restart,
    /// Service has to be removed
    Remove,
}

impl ServicePendingAction {
    #[inline(always)]
    pub(crate) fn is_none(&self) -> bool {
        matches!(self, ServicePendingAction::None)
    }
}

/// A minimal service representation.
#[derive(Debug, Clone)]
pub(crate) struct Service {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) config: ServiceConfig,
    pub(crate) argv: Vec<CString>,
    pub(crate) envp: Option<Vec<CString>>,
    pub(crate) state: ServiceState,
    pub(crate) pending_action: ServicePendingAction,
}

impl Service {
    #[inline(always)]
    pub(crate) fn new(id: u64, name: String, config: ServiceConfig) -> io::Result<Self> {
        let argv = config.build_svc_argv()?;
        let envp = config.build_svc_envp()?;
        Ok(Self {
            id,
            name,
            config,
            argv,
            envp,
            state: ServiceState::Stopped(ServiceStopReason::NeverStarted),
            pending_action: ServicePendingAction::None,
        })
    }

    #[inline(always)]
    pub(crate) fn pid(&self) -> Option<Pid> {
        match self.state {
            ServiceState::Running(p) => Some(p),
            ServiceState::Stopping(p, _) => Some(p),
            _ => None,
        }
    }

    #[inline(always)]
    pub(crate) fn is_stopped(&self) -> bool {
        matches!(self.state, ServiceState::Stopped(_))
    }

    #[inline(always)]
    pub(crate) fn working_directory(&self) -> Option<&Path> {
        self.config.working_directory.as_deref()
    }

    #[inline(always)]
    pub(crate) fn log_file_path(&self) -> Option<&Path> {
        self.config.log_file_path.as_deref()
    }

    #[inline(always)]
    pub(crate) fn user_group(&self) -> Option<UserGroup> {
        self.config.user_group
    }

    #[inline(always)]
    pub(crate) fn fallback_pending_action(&self) -> ServicePendingAction {
        self.config.fallback_pending_action
    }

    #[inline(always)]
    pub(crate) fn stop_signal(&self) -> Signal {
        self.config.stop_signal.into()
    }

    #[inline(always)]
    pub(crate) fn stop_timeout(&self) -> Duration {
        Duration::from_millis(self.config.stop_timeout_ms)
    }

    /// Update the service config and rebuild argv
    #[inline(always)]
    pub(crate) fn update_config(&mut self, config: ServiceConfig) -> io::Result<()> {
        self.argv = config.build_svc_argv()?;
        self.envp = config.build_svc_envp()?;
        self.config = config;
        Ok(())
    }

    /// Returns the `ServicePendingAction` and leave `ServicePendingAction::None`
    /// in its place
    #[inline(always)]
    pub(crate) fn take_pending_action(&mut self) -> ServicePendingAction {
        std::mem::replace(&mut self.pending_action, ServicePendingAction::None)
    }

    #[inline(always)]
    pub(crate) fn format_status_line(&self, w: &mut impl fmt::Write) -> fmt::Result {
        write!(w, "{} {} {}", self.name, self.id, self.state)
    }
}

/// Configure standard file descriptors.
/// processes.
///
/// Invariants:
/// - `devnull_fd` must be an open fd referring to `/dev/null`, opened for read-write
/// - `log_fd`, if present, must be open for write
/// - Both fds must remain valid across fork and must not have been closed in the child
///
/// It is intended to be called in the child arm of a fork, before execvp, so it must
/// not perform any non async-signal-safe operation
fn setup_child_stdio(devnull_fd: BorrowedFd, log_fd: Option<BorrowedFd>) -> rustix::io::Result<()> {
    dup2_stdin(devnull_fd)?;
    match log_fd {
        Some(fd) => {
            dup2_stdout(fd)?;
            dup2_stderr(fd)?;
        }
        None => {
            dup2_stdout(devnull_fd)?;
            dup2_stderr(devnull_fd)?;
        }
    }
    Ok(())
}

fn child_exec(
    svc: &Service,
    sigset: &SigSet,
    devnull_fd: BorrowedFd,
    log_fd: Option<BorrowedFd>,
) -> ! {
    if set_thread_signal_mask(sigset).is_err() {
        unsafe { libc::_exit(111) }
    }
    if let Some(ug) = svc.user_group() {
        unsafe {
            if cvt(libc::setgid(ug.gid)).is_err() {
                libc::_exit(111)
            }
            if cvt(libc::setuid(ug.uid)).is_err() {
                libc::_exit(111)
            }
        }
    }
    if let Some(cwd) = svc.working_directory()
        && chdir(cwd).is_err()
    {
        unsafe { libc::_exit(111) }
    }
    if setup_child_stdio(devnull_fd, log_fd).is_err() {
        unsafe { libc::_exit(111) }
    }
    let argv: Vec<*const libc::c_char> = svc
        .argv
        .iter()
        .map(|s| s.as_ptr())
        .chain(std::iter::once(std::ptr::null()))
        .collect();

    unsafe {
        match &svc.envp {
            None => {
                libc::execvp(argv[0], argv.as_ptr());
            }
            Some(env) => {
                let envp: Vec<*const libc::c_char> = env
                    .iter()
                    .map(|s| s.as_ptr())
                    .chain(std::iter::once(std::ptr::null()))
                    .collect();
                libc::execvpe(argv[0], argv.as_ptr(), envp.as_ptr());
            }
        }
        libc::_exit(127);
    }
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
pub(crate) fn start_service(svc: &mut Service, sigset: &SigSet) -> io::Result<()> {
    let devnull_fd = open("/dev/null", OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())?;
    let log_fd = svc
        .log_file_path()
        .map(|p| {
            open(
                p,
                OFlags::WRONLY | OFlags::CREATE | OFlags::APPEND | OFlags::CLOEXEC,
                Mode::from_bits_truncate(0o644),
            )
        })
        .transpose()?;
    match unsafe { libc::fork() } {
        0 => child_exec(
            svc,
            sigset,
            devnull_fd.as_fd(),
            log_fd.as_ref().map(|fd| fd.as_fd()),
        ),
        raw if raw > 0 => {
            // safe as we just checked that the pid is > 0
            let pid = unsafe { Pid::from_raw_unchecked(raw) };
            svc.state = ServiceState::Running(pid);
            Ok(())
        }
        _ => Err(io::Error::last_os_error()),
    }
}

/// Stop a service by sending the configured stop signal and marks it as
/// stopping by setting state to `ServiceState::Stopping`.
/// This is a state transition: it only acts on `ServiceState::Running`
/// services and is a no-op for any other state
pub(crate) fn stop_service(svc: &mut Service) -> io::Result<()> {
    match svc.state {
        ServiceState::Running(p) => {
            kill_process(p, svc.stop_signal())?;
            svc.state = ServiceState::Stopping(p, Instant::now() + svc.stop_timeout());
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Send `SIGKILL` to the given process.
///
/// This is pure mechanism and has no state awareness. The caller is
/// responsible for maintaining the invariants and performing any
/// required state transitions
pub(crate) fn force_kill_service_process(pid: Pid) -> io::Result<()> {
    kill_process(pid, Signal::KILL)?;
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
/// been successfully forked (in the parent).
#[derive(Debug, Clone, Default)]
pub(crate) struct ServiceRegistry {
    /// `service_id -> service`
    services_map: HashMap<u64, Service>,
    /// `pid -> service_id`
    pids_map: HashMap<Pid, u64>,
}

impl ServiceRegistry {
    #[inline(always)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Insert a new service in the `service_id -> service` map
    #[inline(always)]
    pub(crate) fn insert_service(&mut self, svc: Service) {
        self.services_map.insert(svc.id, svc);
    }

    /// Get a shared reference to the service correspondig to
    /// `svc_id` if it exists in the `service_id -> service` map
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn service(&self, svc_id: u64) -> Option<&Service> {
        self.services_map.get(&svc_id)
    }

    /// Get a mutable reference to the service corresponding to
    /// `svc_id` if it exists in the `service_id -> service` map
    #[inline(always)]
    pub(crate) fn service_mut(&mut self, svc_id: u64) -> Option<&mut Service> {
        self.services_map.get_mut(&svc_id)
    }

    /// Insert a new pid in the `pid -> service_id` map.
    /// The caller is responsible to maintain the consistency and must
    /// make sure to have properly updated the `service.pid` to
    /// `Some(pid)` for the `Service` corresponding to `svc_id`
    #[inline(always)]
    pub(crate) fn register_pid(&mut self, pid: Pid, svc_id: u64) {
        self.pids_map.insert(pid, svc_id);
    }

    /// Get a shared reference to the service corresponding to pid.
    #[inline(always)]
    pub(crate) fn get_by_pid(&self, pid: Pid) -> Option<&Service> {
        let svc_id = self.pids_map.get(&pid)?;
        self.services_map.get(svc_id)
    }

    /// Remove `pid` from the `pid -> service_id` map if exists and
    /// return a mutable reference to the correspondind `Service` in
    /// the `service_id -> service` map
    #[inline(always)]
    pub(crate) fn take_by_pid(&mut self, pid: Pid) -> Option<&mut Service> {
        let svc_id = self.pids_map.remove(&pid)?;
        self.services_map.get_mut(&svc_id)
    }

    #[inline(always)]
    pub(crate) fn services(&self) -> std::collections::hash_map::Values<'_, u64, Service> {
        self.services_map.values()
    }

    #[inline(always)]
    pub(crate) fn services_mut(
        &mut self,
    ) -> std::collections::hash_map::ValuesMut<'_, u64, Service> {
        self.services_map.values_mut()
    }

    #[inline(always)]
    pub(crate) fn remove_service(&mut self, svc_id: u64) -> Option<Service> {
        self.services_map.remove(&svc_id)
    }

    /// Execute a closure with mutable access to both `services_map` and
    /// `pids_map`.
    ///
    /// This exists to allow for use cases like iterating through services
    /// to restart or remove them while also keep the pid index in sync,
    /// without exposing the maps publicly
    #[inline(always)]
    pub(crate) fn with_maps_mut<R>(
        &mut self,
        f: impl FnOnce(&mut HashMap<u64, Service>, &mut HashMap<Pid, u64>) -> R,
    ) -> R {
        f(&mut self.services_map, &mut self.pids_map)
    }

    pub(crate) fn format_status(&self, w: &mut impl fmt::Write) -> fmt::Result {
        for svc in self.services_map.values() {
            svc.format_status_line(w)?;
            w.write_char('\n')?;
        }
        Ok(())
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
///   that it can be removed once the process has been reaped.
/// * If `ServiceState::Stopping(_)`: just mark for removal.
///
/// For changed services (present in both the registry and the new config
/// but with different configurations):
/// * If the service state is `ServiceState::Stopped(_)`: update the config,
///   rebuild argv, then start it.
/// * If `ServiceState::Running`: call `stop_service`, store new config and
///   mark for restart so that it can be restarted after the process has been
///   reaped.
/// * if `ServiceState::Stopping(_)`: just store the new config and mark for
///   restart.
pub(crate) fn reload_services(
    registry: &mut ServiceRegistry,
    cfg_path: &Path,
    id_gen: &mut ServiceIdGen,
    sigset: &SigSet,
) -> io::Result<()> {
    let service_configs = ServiceConfigData::from_config_file(cfg_path)?;
    let mut service_ids = HashMap::new();
    for svc in registry.services() {
        service_ids.insert(svc.name.clone(), svc.id);
    }
    for (name, &svc_id) in service_ids.iter() {
        if !service_configs.services.contains_key(name)
            && let Some(svc) = registry.service_mut(svc_id)
        {
            match svc.state {
                ServiceState::Stopped(_) => {
                    svlogg!(LogLevel::Info, "removing stopped service '{}'", name);
                    svc.pending_action = ServicePendingAction::None;
                    let _ = registry.remove_service(svc_id);
                }
                ServiceState::Stopping(_, _) => {
                    svlogg!(LogLevel::Info, "stopping service '{}' for removal", name);
                    svc.pending_action = ServicePendingAction::Remove;
                }
                ServiceState::Running(_) => {
                    svlogg!(LogLevel::Info, "stopping service '{}' for removal", name);
                    svc.pending_action = ServicePendingAction::Remove;
                    stop_service(svc)?;
                }
            }
        }
    }

    for (name, cfg) in service_configs.services {
        match service_ids.get(&name) {
            None => {
                svlogg!(LogLevel::Debug, "adding new service '{}'", name);
                let svc_id = id_gen
                    .nextval()
                    .ok_or_else(|| io::Error::other("service id overflow"))?;
                let mut svc = Service::new(svc_id, name, cfg)?;
                start_service(&mut svc, sigset)?;
                let svc_pid = svc.pid().expect("running service must have a pid");
                svlogg!(
                    LogLevel::Info,
                    "started new service '{}' with pid {}",
                    svc.name,
                    svc_pid
                );
                registry.insert_service(svc);
                registry.register_pid(svc_pid, svc_id);
            }
            Some(&svc_id) => {
                if let Some(svc) = registry.service_mut(svc_id)
                    && (svc.config != cfg)
                {
                    svlogg!(LogLevel::Debug, "config changed for service {}", name);
                    // Update the config now so that when the process is eventually
                    // restarted, it uses the new definition. The currently running
                    // process continues with the old config until it exits.
                    svc.update_config(cfg)?;
                    match svc.state {
                        ServiceState::Stopped(_) => {
                            svlogg!(
                                LogLevel::Info,
                                "service '{}' was stopped, starting with new config",
                                name
                            );
                            start_service(svc, sigset)?;
                            let svc_pid = svc.pid().expect("running service must have a pid");
                            registry.register_pid(svc_pid, svc_id);
                        }
                        ServiceState::Stopping(_, _) => {
                            svlogg!(LogLevel::Info, "service '{}' will be restarted", name);
                            svc.pending_action = ServicePendingAction::Restart;
                        }
                        ServiceState::Running(_) => {
                            svlogg!(LogLevel::Info, "service '{}' will be restarted", name);
                            svc.pending_action = ServicePendingAction::Restart;
                            stop_service(svc)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Apply a control operation,
///
/// Control operations are treated as *requests*, meaning they must:
/// - Respect the current state.
/// - Never override an existing pending action.
/// - Never act on a stopped service whose pending action has yet to
///   be processed.
///
/// In particular:
/// - `Stop`: stops a service *only* if it is running. Never clears a
///   pending action. This is safe, as any pending action will be applied
///   after the service process is reaped.
/// - `Start`: starts a service *only* if it is stopped *and* has no
///   pending action. Never sets/clears a pending action.
/// - `Restart`: if the service is stopped *and* has no pending action,
///   starts it. If it is running *and* has no pending action, stops it
///   and sets `pending_action = ServicePendingAction::Restart`. Does
///   nothing otherwise.
pub(crate) fn apply_control_op(
    registry: &mut ServiceRegistry,
    svc_id: u64,
    op: ControlOp,
    sigset: &SigSet,
) -> io::Result<()> {
    if let Some(svc) = registry.service_mut(svc_id) {
        let svc_id = svc.id;
        match op {
            ControlOp::Stop => {
                if matches!(svc.state, ServiceState::Running(_)) {
                    svlogg!(LogLevel::Info, "stopping service '{}'", svc.name);
                    stop_service(svc)?;
                }
            }
            ControlOp::Start => {
                if matches!(svc.state, ServiceState::Stopped(_)) && svc.pending_action.is_none() {
                    start_service(svc, sigset)?;
                    let svc_pid = svc.pid().expect("running service must have a pid");
                    svlogg!(
                        LogLevel::Info,
                        "started service '{}' with pid {}",
                        svc.name,
                        svc_pid,
                    );
                    registry.register_pid(svc_pid, svc_id);
                }
            }
            ControlOp::Restart => match svc.state {
                ServiceState::Stopped(_) if svc.pending_action.is_none() => {
                    start_service(svc, sigset)?;
                    let svc_pid = svc.pid().expect("running service must have a pid");
                    svlogg!(
                        LogLevel::Info,
                        "started service '{}' with pid {}",
                        svc.name,
                        svc_pid
                    );
                    registry.register_pid(svc_pid, svc_id);
                }
                ServiceState::Running(_) if svc.pending_action.is_none() => {
                    svlogg!(LogLevel::Info, "service '{}' will be restarted", svc.name);
                    svc.pending_action = ServicePendingAction::Restart;
                    stop_service(svc)?;
                }
                _ => {}
            },
        }
    } else {
        svlogg!(LogLevel::Warn, "unkown service id: {}", svc_id);
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
pub(crate) fn handle_sigchld(registry: &mut ServiceRegistry) -> io::Result<()> {
    loop {
        match wait(WaitOptions::NOHANG) {
            Ok(Some((pid, status))) => {
                if let Some(exit_reason) = ExitReason::from_wait_status(status) {
                    match registry.take_by_pid(pid) {
                        Some(svc) => {
                            let stop_reason = ServiceStopReason::from_exit_reason_and_service_state(
                                exit_reason,
                                svc.state,
                            );
                            debug_assert!(
                                !matches!(stop_reason, ServiceStopReason::NeverStarted),
                                "reaped service '{}' that was never started",
                                svc.name
                            );
                            svc.state = ServiceState::Stopped(stop_reason);
                            svlogg!(
                                LogLevel::Info,
                                "service '{}' exited: {:?}",
                                svc.name,
                                exit_reason,
                            );
                        }
                        None => svlogg!(
                            LogLevel::Info,
                            "reaped unknown pid {} (likely adopted descendant)",
                            pid
                        ),
                    }
                } else {
                    match registry.get_by_pid(pid) {
                        Some(svc) => {
                            svlogg!(
                                LogLevel::Warn,
                                "service '{}' exited with status {:?}",
                                svc.name,
                                status
                            )
                        }
                        None => svlogg!(LogLevel::Warn, "`waitpid` got unknown pid: {}", pid),
                    }
                }
            }
            Ok(None) => break,                      // no more childs ready
            Err(rustix::io::Errno::CHILD) => break, // no child
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
