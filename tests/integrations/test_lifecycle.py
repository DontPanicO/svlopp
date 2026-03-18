import os
import signal
from pathlib import Path

from helpers.status_file import read_status
from helpers.utils import is_zombie, wait_until
from constants import (
    STATE_RUNNING,
    STATE_STOPPED,
    REASON_EXITED,
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

    def service_running():
        try:
            status = read_status(run_dir)
            return status.is_running("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(service_running, timeout=1.0)

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
