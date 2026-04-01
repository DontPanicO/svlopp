# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import signal

from helpers.status_file import read_status
from helpers.utils import wait_until, pid_exists
from constants import (
    STATE_RUNNING,
    CONFIG_FILE_NAME,
)


def test_reload_add_service(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.a]
command = "/bin/sleep"
args = ["10"]
"""
    )

    proc = svlopp_proc(config_path)

    def is_a_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_a_running, timeout=1.0)

    config_path.write_text(
        """
[services.a]
command = "/bin/sleep"
args = ["10"]

[services.b]
command = "/bin/sleep"
args = ["10"]
"""
    )

    os.kill(proc.pid, signal.SIGHUP)

    def is_b_running():
        try:
            status = read_status(run_dir)
            return status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_b_running, timeout=2.0)

    status = read_status(run_dir)
    a = status.get("a")
    b = status.get("b")

    assert a.state == STATE_RUNNING
    assert b.state == STATE_RUNNING
    assert pid_exists(int(a.pid_or_reason))
    assert pid_exists(int(b.pid_or_reason))


def test_reload_remove_service(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.a]
command = "/bin/sleep"
args = ["10"]

[services.b]
command = "/bin/sleep"
args = ["10"]
"""
    )

    proc = svlopp_proc(config_path)

    def all_services_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a") and status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(all_services_running, timeout=1.0)

    status = read_status(run_dir)
    b_pid = int(status.get("b").pid_or_reason)

    config_path.write_text(
        """
[services.a]
command = "/bin/sleep"
args = ["10"]
"""
    )

    os.kill(proc.pid, signal.SIGHUP)

    def is_b_removed():
        try:
            status = read_status(run_dir)
            return not status.has("b")
        except FileNotFoundError:
            return False

    wait_until(is_b_removed, timeout=3.0)

    def is_b_dead():
        return not pid_exists(b_pid)

    wait_until(is_b_dead, timeout=3.0)

    status = read_status(run_dir)
    a = status.get("a")

    assert not status.has("b")
    assert a.state == STATE_RUNNING
    assert pid_exists(int(a.pid_or_reason))
    assert not pid_exists(b_pid)


def test_reload_change_service(tmp_path, run_dir, svlopp_proc):
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

    status = read_status(run_dir)
    old_pid = int(status.get("test").pid_or_reason)

    config_path.write_text(
        """
[services.test]
command = "/bin/sleep"
args = ["20"]
"""
    )

    os.kill(proc.pid, signal.SIGHUP)

    def is_test_restarted():
        try:
            status = read_status(run_dir)
            line = status.get("test")
            if line.state != STATE_RUNNING:
                return False
            return int(line.pid_or_reason) != old_pid
        except (FileNotFoundError, KeyError, ValueError):
            return False

    wait_until(is_test_restarted, timeout=3.0)

    status = read_status(run_dir)
    test = status.get("test")

    assert test.state == STATE_RUNNING
    assert int(test.pid_or_reason) != old_pid
    assert not pid_exists(old_pid)
    assert pid_exists(int(test.pid_or_reason))


def test_reload_add_remove(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.a]
command = "sleep"
args = ["10"]
"""
    )

    proc = svlopp_proc(config_path)

    def is_a_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_a_running, timeout=1.0)

    status = read_status(run_dir)
    a_pid = int(status.get("a").pid_or_reason)

    config_path.write_text(
        """
[services.b]
command = "sleep"
args = ["10"]
"""
    )

    os.kill(proc.pid, signal.SIGHUP)

    def reconciled():
        try:
            status = read_status(run_dir)
            return status.is_running("b") and not status.has("a")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(reconciled, timeout=5.0)

    status = read_status(run_dir)

    b = status.get("b")
    assert not status.has("a")
    assert b.state == STATE_RUNNING
    assert pid_exists(int(b.pid_or_reason))
    assert not pid_exists(a_pid)


def test_reload_add_remove_change(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.a]
command = "sleep"
args = ["10"]

[services.b]
command = "sleep"
args = ["12"]
"""
    )

    proc = svlopp_proc(config_path)

    def all_services_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a") and status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(all_services_running, timeout=1.0)

    status = read_status(run_dir)
    old_a_pid = int(status.get("a").pid_or_reason)
    b_pid = int(status.get("b").pid_or_reason)

    config_path.write_text(
        """
[services.a]
command = "sleep"
args = ["12"]

[services.c]
command = "sleep"
args = ["15"]
"""
    )

    os.kill(proc.pid, signal.SIGHUP)

    def reconciled():
        try:
            status = read_status(run_dir)
            a = status.get("a")
            return (
                a.state == STATE_RUNNING
                and status.is_running("c")
                and not status.has("b")
                and int(a.pid_or_reason) != old_a_pid
            )
        except (FileNotFoundError, KeyError, ValueError):
            return False

    wait_until(reconciled, timeout=5.0)

    status = read_status(run_dir)
    a = status.get("a")
    c = status.get("c")

    assert a.state == STATE_RUNNING
    assert c.state == STATE_RUNNING
    assert pid_exists(int(a.pid_or_reason))
    assert pid_exists(int(c.pid_or_reason))
    assert not pid_exists(old_a_pid)
    assert not pid_exists(b_pid)


# this tests that an unchanged service definition doesn't cause the service
# to restart after a reload.
# However, blindly checking that the service process has the same PID
# immediately after sending SIGHUP is not enough as it doesn't guarantee
# svlopp has completely handled the reload and more importantly restart actions
# are deferred after reaping and enforced in the timerfd path (which fires
# once per second).
# For that reason, we add a second service to the config, modify it before
# reload and check that its pid has changed to determine if the reload operation
# has completed. Only then we make assertions against the first service (the one
# that's actually the subject of this test).
def test_reload_unchanged_keeps_running(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME

    config_path.write_text(
        """
[services.a]
command = "/bin/sleep"
args = ["10"]

[services.b]
command = "/bin/sleep"
args = ["10"]
"""
    )

    proc = svlopp_proc(config_path)

    def all_services_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a") and status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(all_services_running, timeout=1.0)

    status = read_status(run_dir)
    a_pid = int(status.get("a").pid_or_reason)
    b_pid = int(status.get("b").pid_or_reason)

    config_path.write_text(
        """
[services.a]
command = "/bin/sleep"
args = ["10"]

[services.b]
command = "/bin/sleep"
args = ["12"]
"""
    )

    os.kill(proc.pid, signal.SIGHUP)

    def has_b_restarted():
        try:
            status = read_status(run_dir)
            b = status.get("b")
            return b.state == STATE_RUNNING and int(b.pid_or_reason) != b_pid
        except (FileNotFoundError, KeyError):
            return False

    wait_until(has_b_restarted, timeout=5.0)

    status = read_status(run_dir)
    a = status.get("a")

    assert a.state == STATE_RUNNING
    assert pid_exists(int(a.pid_or_reason))
    assert int(a.pid_or_reason) == a_pid
