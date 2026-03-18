import os
import signal
from pathlib import Path

from helpers.status_file import read_status
from helpers.utils import wait_until
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

    def a_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(a_running, timeout=1.0)

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

    def b_running():
        try:
            status = read_status(run_dir)
            return status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(b_running, timeout=2.0)

    status = read_status(run_dir)

    assert status.is_running("a")
    assert status.is_running("b")


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

    def both_running():
        try:
            status = read_status(run_dir)
            return status.is_running("a") and status.is_running("b")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(both_running, timeout=1.0)

    status = read_status(run_dir)
    pid_b = int(status.get("b").pid_or_reason)

    config_path.write_text(
        """
[services.a]
command = "/bin/sleep"
args = ["10"]
"""
    )

    os.kill(proc.pid, signal.SIGHUP)

    def b_removed():
        try:
            status = read_status(run_dir)
            status.get("b")
            return False
        except KeyError:
            return True
        except FileNotFoundError:
            return False

    wait_until(b_removed, timeout=2.0)

    def b_dead():
        return not Path(f"/proc/{pid_b}").exists()

    wait_until(b_dead, timeout=3.0)

    assert not Path(f"/proc/{pid_b}").exists()

    status = read_status(run_dir)
    assert status.is_running("a")


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

    def is_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_running, timeout=1.0)

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

    def new_pid_running():
        try:
            status = read_status(run_dir)
            line = status.get("test")
            if line.state != STATE_RUNNING:
                return False
            return int(line.pid_or_reason) != old_pid
        except (FileNotFoundError, KeyError, ValueError):
            return False

    wait_until(new_pid_running, timeout=3.0)

    status = read_status(run_dir)
    line = status.get("test")
    new_pid = int(line.pid_or_reason)

    assert line.state == STATE_RUNNING
    assert new_pid != old_pid

    wait_until(lambda: not Path(f"/proc/{old_pid}").exists(), timeout=3.0)

    assert Path(f"/proc/{new_pid}").exists()
