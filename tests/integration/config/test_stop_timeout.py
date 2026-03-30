# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.


from constants import (
    CONFIG_FILE_NAME,
    REASON_SIGNALED,
    REASON_SUPERVISOR_TERMINATED,
    STATE_STOPPED,
    STOP_OPCODE,
)
from helpers.utils import wait_until
from helpers.status_file import read_status
from helpers.control_fifo import send_control_op


def test_stop_timeout_triggers_sigkill(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.test]
command = "/bin/bash"
args = ["-c", "trap '' SIGTERM; while true; do :; done"]
stop_timeout_ms = 1000
"""
    )

    _ = svlopp_proc(config_path)

    def is_test_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_running, timeout=1.0)

    status = read_status(run_dir)
    test = status.get("test")
    send_control_op(run_dir, STOP_OPCODE, test.service_id)

    def is_test_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_stopped, timeout=3.0)

    status = read_status(run_dir)

    test = status.get("test")
    assert test.state == STATE_STOPPED
    assert test.pid_or_reason == f"{REASON_SUPERVISOR_TERMINATED}({REASON_SIGNALED}(9))"
