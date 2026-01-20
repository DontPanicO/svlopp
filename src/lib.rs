// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod service;
pub mod signalfd;
pub mod timerfd;
pub mod utils;

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
