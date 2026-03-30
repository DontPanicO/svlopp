# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import os
import signal
from constants import CONFIG_FILE_NAME
from helpers.status_file import read_status
from helpers.utils import wait_until


def test_default_stop_signal(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/bash"
args = ["-c", "trap 'echo SIGTERM > {output_file_path}; exit 0' SIGTERM; while true; do :; done"]
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

    os.kill(proc.pid, signal.SIGTERM)

    wait_until(
        lambda: output_file_path.exists()
        and output_file_path.read_text().strip() != "",
        timeout=6.0,
    )

    content = output_file_path.read_text().strip()
    assert content == "SIGTERM"


def test_stop_signal_happy_path(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/bash"
args = ["-c", "trap 'echo SIGINT > {output_file_path}; exit 0' SIGINT; while true; do sleep :; done"]
stop_signal = "SIGINT"
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

    os.kill(proc.pid, signal.SIGTERM)

    wait_until(
        lambda: output_file_path.exists() and output_file_path.read_text().strip(),
        timeout=6.0,
    )

    content = output_file_path.read_text().strip()
    assert content == "SIGINT"
