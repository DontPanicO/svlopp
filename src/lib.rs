pub mod service;
pub mod signalfd;
pub mod timerfd;
pub mod utils;

use serde::Deserialize;

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

/// The service config file
#[derive(Debug, Deserialize)]
pub struct ServicesConfigFile {
    pub service: Vec<service::ServiceConfig>,
}
