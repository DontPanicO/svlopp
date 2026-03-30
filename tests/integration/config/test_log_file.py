# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import signal

from constants import CONFIG_FILE_NAME
from helpers.utils import wait_until
from helpers.status_file import read_status


def test_log_file_happy_path(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    log_file_path = tmp_path / "test_log"
    msg = "hello world"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "echo '{msg}'"]
log_file_path = "{log_file_path}"
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

    assert log_file_path.exists()
    content = log_file_path.read_text().strip()
    assert content == msg


def test_log_file_append(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    log_file_path = tmp_path / "test_log"
    first_msg = "hello"
    second_msg = "world"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "echo {first_msg}"]
log_file_path = "{log_file_path}"
"""
    )

    proc = svlopp_proc(config_path)

    def is_test_stopped():
        try:
            status = read_status(run_dir)
            return status.is_stopped("test")
        except (FileNotFoundError, KeyError):
            return False

    wait_until(is_test_stopped, timeout=3.0)

    assert log_file_path.exists()

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "echo {second_msg}"]
log_file_path = "{log_file_path}"
"""
    )

    os.kill(proc.pid, signal.SIGHUP)

    # the `test` service is very short lived so the
    # `stopped -> running -> stopped` transition may happen before we
    # read the status file content, thus:
    # ```
    # wait_until(is_test_running)
    # wait_until(is_test_stopped)
    # ```
    # may get stuck on `wait_until(is_test_running)` and time out.
    #
    # But relying on its short lifetime and just checking for
    # `wait_until(is_test_stopped)` is just as poorly robust.
    #
    # Using a fixed sleep (e.g. `time.sleep(0.3)`) works but is still
    # not enough.
    #
    # The only solution I have been able to come up with is to check
    # the side effect of the service (e.g. adding a line to the log file).
    def has_second_msg():
        try:
            content = log_file_path.read_text().strip().splitlines()
            return len(content) > 1
        except FileNotFoundError:
            return False

    wait_until(has_second_msg, timeout=3.0)
    wait_until(is_test_stopped, timeout=3.0)

    assert log_file_path.exists()
    content = log_file_path.read_text().strip().splitlines()
    assert len(content) == 2
    assert content == [first_msg, second_msg]


def test_log_file_captures_stdout_and_stderr(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    log_file_path = tmp_path / "test_log"
    stdout_msg = "hello"
    stderr_msg = "error"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "echo {stdout_msg}; echo {stderr_msg} >&2"]
log_file_path = "{log_file_path}"
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

    assert log_file_path.exists()
    content = log_file_path.read_text().strip().splitlines()
    assert stdout_msg in content
    assert stderr_msg in content
