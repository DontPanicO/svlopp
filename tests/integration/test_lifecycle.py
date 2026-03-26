# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import signal
from pathlib import Path

from helpers.status_file import read_status
from helpers.utils import is_zombie, wait_until
from constants import (
    STATE_RUNNING,
    STATE_STOPPED,
    REASON_EXITED,
    REASON_SIGNALED,
    CONFIG_FILE_NAME,
)


def test_service_starts(tmp_path, run_dir, svlopp_proc):
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

    assert line.state == STATE_RUNNING
    assert line.pid_or_reason.isdigit()
    assert Path(f"/proc/{line.pid_or_reason}").exists()


def test_service_exits(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/bin/sleep"
args = ["1"]
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


def test_service_start_fail_missing_binary(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/bin/this_does_not_exist"
"""
    )

    _ = svlopp_proc(config_path)

    def is_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except Exception:
            return False

    wait_until(is_stopped, timeout=3.0)

    status = read_status(run_dir)
    line = status.get("test")

    assert line.state == STATE_STOPPED
    assert line.pid_or_reason == f"{REASON_EXITED}(127)"


def test_service_start_fail_missing_permission(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/etc/shadow"
"""
    )

    _ = svlopp_proc(config_path)

    def is_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except Exception:
            return False

    wait_until(is_stopped, timeout=3.0)

    status = read_status(run_dir)
    line = status.get("test")

    assert line.state == STATE_STOPPED
    assert line.pid_or_reason == f"{REASON_EXITED}(127)"


def test_service_start_fail_working_dir_does_not_exist(
    tmp_path,
    run_dir,
    svlopp_proc,
):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "sleep"
args = ["10"]
working_directory = "/does/not/exist"
"""
    )

    _ = svlopp_proc(config_path)

    def is_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except Exception:
            return False

    wait_until(is_stopped, timeout=3.0)

    status = read_status(run_dir)
    line = status.get("test")

    assert line.state == STATE_STOPPED
    assert line.pid_or_reason == f"{REASON_EXITED}(111)"


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
    assert line.pid_or_reason == f"{REASON_SIGNALED}(9)"
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
    assert line.pid_or_reason == f"{REASON_SIGNALED}(11)"
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
    assert line.pid_or_reason == f"{REASON_EXITED}(1)"


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
    assert line.pid_or_reason == f"{REASON_EXITED}(0)"


def test_graceful_shutdown(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/bin/sleep"
args = ["10"]
"""
    )

    proc = svlopp_proc(config_path)

    def is_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_running, timeout=1.0)

    # Since svlopp deletes the run dir before exiting
    # we can't rely on the status file to check if
    # services has been stopped. We should grab the pids
    # first and check if they are still there after sending
    # `SIGTERM`. This is a bit fragile as it doesn't account
    # for pid recycling, but it's fine for now
    status = read_status(run_dir)
    pids = [
        int(line.pid_or_reason) for line in status.lines if line.state != STATE_STOPPED
    ]

    os.kill(proc.pid, signal.SIGTERM)

    def all_dead():
        return all(not Path(f"/proc/{pid}").exists() for pid in pids)

    wait_until(all_dead, timeout=3.0)

    for pid in pids:
        assert not Path(f"/proc/{pid}").exists()
    # svlopp has not been reaped yet, so it still exists
    # under `/proc`. We check stats to assert that's
    # zombie
    assert is_zombie(proc.pid)


def test_multiple_services_start(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.a]
command = "sleep"
args = ["10"]

[services.b]
command = "sleep"
args = ["10"]
"""
    )

    _ = svlopp_proc(config_path)

    def all_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a") and status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(all_running, timeout=1.0)

    status = read_status(run_dir)

    assert status.is_running("a")
    assert status.is_running("b")


def test_multiple_services_independent_lifecycle(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.long]
command = "sleep"
args = ["10"]

[services.short]
command = "/bin/true"
"""
    )

    _ = svlopp_proc(config_path)

    def short_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("short")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(short_stopped, timeout=3.0)

    status = read_status(run_dir)

    assert status.is_running("long")
    short = status.get("short")
    assert short.state == STATE_STOPPED
    assert short.pid_or_reason == f"{REASON_EXITED}(0)"


def test_multiple_services_kill_one(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.a]
command = "sleep"
args = ["10"]

[services.b]
command = "sleep"
args = ["10"]
"""
    )

    _ = svlopp_proc(config_path)

    def all_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a") and status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(all_running, timeout=1.0)

    status = read_status(run_dir)
    pid_a = int(status.get("a").pid_or_reason)

    os.kill(pid_a, signal.SIGKILL)

    def a_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("a")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(a_stopped, timeout=3.0)

    status = read_status(run_dir)

    assert status.is_running("b")
    a = status.get("a")
    assert a.state == STATE_STOPPED
    assert a.pid_or_reason == f"{REASON_SIGNALED}(9)"
