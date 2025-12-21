use std::ffi::CString;

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
    /// Services are put in this state when
    /// after a stop request, which can be
    /// issued from different actors and in
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
