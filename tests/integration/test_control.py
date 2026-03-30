# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from helpers.status_file import read_status
from helpers.utils import pid_exists, wait_until
from constants import (
    CONFIG_FILE_NAME,
    REASON_SUPERVISOR_TERMINATED,
    RESTART_OPCODE,
    START_OPCDOE,
    STATE_RUNNING,
    STATE_STOPPED,
    STOP_OPCODE,
)
from helpers.control_fifo import send_control_op


def test_control_stop(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.test]
command = "/bin/sleep"
args = ["10"]
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
    test_pid = int(test.pid_or_reason)

    send_control_op(run_dir, STOP_OPCODE, test.service_id)

    def is_test_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_stopped, timeout=5.0)

    status = read_status(run_dir)

    test = status.get("test")
    assert test.state == STATE_STOPPED
    assert test.pid_or_reason == REASON_SUPERVISOR_TERMINATED
    assert not pid_exists(test_pid)


def test_control_start(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.test]
command = "/bin/sleep"
args = ["10"]
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
    old_test_pid = int(test.pid_or_reason)

    send_control_op(run_dir, STOP_OPCODE, test.service_id)

    def is_test_dead():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test") and not pid_exists(old_test_pid)
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_dead, timeout=5.0)

    status = read_status(run_dir)
    test = status.get("test")

    send_control_op(run_dir, START_OPCDOE, test.service_id)

    wait_until(is_test_running, timeout=5.0)

    status = read_status(run_dir)

    test = status.get("test")
    assert test.state == STATE_RUNNING
    assert pid_exists(int(test.pid_or_reason))
    assert not pid_exists(old_test_pid)


def test_control_restart(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.test]
command = "/bin/sleep"
args = ["10"]
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
    old_test_pid = int(test.pid_or_reason)

    send_control_op(run_dir, RESTART_OPCODE, test.service_id)

    def is_test_restarted():
        try:
            status = read_status(run_dir)
            test = status.get("test")
            return test.state == STATE_RUNNING and test.pid_or_reason != str(
                old_test_pid
            )
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_restarted, timeout=5.0)

    status = read_status(run_dir)

    test = status.get("test")
    assert test.state == STATE_RUNNING
    assert pid_exists(int(test.pid_or_reason))
    assert not pid_exists(old_test_pid)
