pub mod service;
pub mod signalfd;
pub mod timerfd;
pub mod utils;

use std::io;

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

/// The set of services config
#[derive(Debug, Deserialize)]
pub struct ServiceConfigData {
    #[serde(rename = "service")]
    pub services: Vec<service::ServiceConfig>,
}

impl ServiceConfigData {
    #[inline(always)]
    pub fn from_config_file(path: &str) -> io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| io::Error::other(e.message()))
    }
}
