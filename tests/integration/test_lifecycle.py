# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import signal

from helpers.status_file import read_status
from helpers.utils import is_zombie, wait_until, pid_exists
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

    def is_test_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_running, timeout=1.0)

    status = read_status(run_dir)

    test = status.get("test")
    assert test.state == STATE_RUNNING
    assert pid_exists(int(test.pid_or_reason))


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

    def is_test_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_running, timeout=1.0)

    status = read_status(run_dir)
    old_test_pid = int(status.get("test").pid_or_reason)

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
    assert test.pid_or_reason.startswith(REASON_EXITED)
    assert not pid_exists(old_test_pid)


def test_service_start_fail_missing_binary(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/bin/this_does_not_exist"
"""
    )

    _ = svlopp_proc(config_path)

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
    assert test.pid_or_reason == f"{REASON_EXITED}(127)"


def test_service_start_fail_missing_permission(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/etc/shadow"
"""
    )

    _ = svlopp_proc(config_path)

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
    assert test.pid_or_reason == f"{REASON_EXITED}(127)"


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
    assert test.pid_or_reason == f"{REASON_EXITED}(111)"


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

    def is_test_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_running, timeout=1.0)

    status = read_status(run_dir)
    test_pid = int(status.get("test").pid_or_reason)

    os.kill(test_pid, signal.SIGKILL)

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
    assert test.pid_or_reason == f"{REASON_SIGNALED}(9)"
    assert not pid_exists(test_pid)


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

    def is_test_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_running, timeout=1.0)

    status = read_status(run_dir)
    test_pid = int(status.get("test").pid_or_reason)

    os.kill(test_pid, signal.SIGSEGV)

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
    assert test.pid_or_reason == f"{REASON_SIGNALED}(11)"
    assert not pid_exists(test_pid)


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
    assert test.pid_or_reason == f"{REASON_EXITED}(1)"


def test_service_success(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    config_path.write_text(
        """
[services.test]
command = "/bin/true"
"""
    )

    _ = svlopp_proc(config_path)

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
    assert test.pid_or_reason == f"{REASON_EXITED}(0)"


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

    def is_test_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_running, timeout=1.0)

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

    def all_services_dead():
        return all(not pid_exists(pid) for pid in pids)

    wait_until(all_services_dead, timeout=3.0)

    for pid in pids:
        assert not pid_exists(pid)
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

    def all_services_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a") and status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(all_services_running, timeout=1.0)

    status = read_status(run_dir)

    a = status.get("a")
    b = status.get("b")
    assert a.state == STATE_RUNNING
    assert b.state == STATE_RUNNING
    assert pid_exists(int(a.pid_or_reason))
    assert pid_exists(int(b.pid_or_reason))


def test_services_do_not_interfere(tmp_path, run_dir, svlopp_proc):
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

    def is_short_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("short")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_short_stopped, timeout=3.0)

    status = read_status(run_dir)

    long = status.get("long")
    short = status.get("short")
    assert long.state == STATE_RUNNING
    assert short.state == STATE_STOPPED
    assert pid_exists(int(long.pid_or_reason))
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

    def all_services_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a") and status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(all_services_running, timeout=1.0)

    status = read_status(run_dir)
    a_pid = int(status.get("a").pid_or_reason)

    os.kill(a_pid, signal.SIGKILL)

    def is_a_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("a")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_a_stopped, timeout=3.0)

    status = read_status(run_dir)

    a = status.get("a")
    b = status.get("b")
    assert a.state == STATE_STOPPED
    assert b.state == STATE_RUNNING
    assert a.pid_or_reason == f"{REASON_SIGNALED}(9)"
    assert not pid_exists(a_pid)
    assert pid_exists(int(b.pid_or_reason))
