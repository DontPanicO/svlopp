# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import signal
from pathlib import Path

from helpers.status_file import read_status
from helpers.utils import wait_until
from constants import (
    STATE_STOPPED,
    REASON_SIGNALED,
    REASON_EXITED,
    CONFIG_FILE_NAME,
)


def test_service_signaled(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "sleep"
args = ["10"]
"""
    )

    _ = svlopp_proc(config_path)

    def is_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_running, timeout=1.0)

    status = read_status(run_dir)
    line = status.get("test")
    pid = int(line.pid_or_reason)

    os.kill(pid, signal.SIGKILL)

    def is_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_stopped, timeout=3.0)

    status = read_status(run_dir)
    line = status.get("test")

    assert line.state == STATE_STOPPED
    assert line.pid_or_reason.startswith(REASON_SIGNALED)
    assert line.pid_or_reason.split("(")[1].rstrip(")") == "9"
    assert not Path(f"/proc/{pid}").exists()


# crashed and killed are currently both represented as "signaled(...)"
# in the status file. This test will become stricter if that changes.
def test_service_crashed(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "sleep"
args = ["10"]
"""
    )

    _ = svlopp_proc(config_path)

    def is_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_running, timeout=1.0)

    status = read_status(run_dir)
    line = status.get("test")

    pid = int(line.pid_or_reason)

    os.kill(pid, signal.SIGSEGV)

    def is_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_stopped, timeout=3.0)

    status = read_status(run_dir)
    line = status.get("test")

    assert line.state == STATE_STOPPED
    assert line.pid_or_reason.startswith(REASON_SIGNALED)
    assert line.pid_or_reason.split("(")[1].rstrip(")") == "11"
    assert not Path(f"/proc/{pid}").exists()


def test_service_error(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/bin/sh"
args = ["-c", "exit 1"]
"""
    )

    _ = svlopp_proc(config_path)

    def is_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_stopped, timeout=3.0)

    status = read_status(run_dir)
    line = status.get("test")

    assert line.state == STATE_STOPPED
    assert line.pid_or_reason.startswith(REASON_EXITED)
    assert line.pid_or_reason.split("(")[1].rstrip(")") == "1"


def test_service_success(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/bin/true"
"""
    )

    _ = svlopp_proc(config_path)

    def is_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_stopped, timeout=3.0)

    status = read_status(run_dir)
    line = status.get("test")

    assert line.state == STATE_STOPPED
    assert line.pid_or_reason.startswith(REASON_EXITED)
    assert line.pid_or_reason.split("(")[1].rstrip(")") == "0"
