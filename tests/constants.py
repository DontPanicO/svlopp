# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

SVLOPP_BINARY_PATH = "./target/debug/svlopp"

CONFIG_FILE_NAME = "services.toml"
RUN_DIR_NAME = "svlopp"
STATUS_FILE_NAME = "status"
CONTROL_FIFO_NAME = "control"

STATE_RUNNING = "running"
STATE_STOPPING = "stopping"
STATE_STOPPED = "stopped"

REASON_EXITED = "exited"
REASON_SIGNALED = "signaled"
REASON_SUPERVISOR_TERMINATED = "supervisor_terminated"

STOP_OPCODE = 0x41
START_OPCDOE = 0x42
RESTART_OPCODE = 0x43
