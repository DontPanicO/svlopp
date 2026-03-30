# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.


from constants import CONFIG_FILE_NAME, REASON_ERROR, STATE_STOPPED
from helpers.utils import wait_until
from helpers.status_file import read_status


def test_working_directory_happy_path(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"
    working_directory = tmp_path / "working"
    working_directory.mkdir()

    config_path.write_text(
        f"""
[services.test]    
command = "/bin/sh"
args = ["-c", "pwd > {output_file_path}"]
working_directory = "{working_directory}"
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

    assert output_file_path.exists()
    content = output_file_path.read_text().strip()
    assert content == f"{working_directory}"


def test_working_directory_missing_fails(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"
    working_directory = tmp_path / "working"

    config_path.write_text(
        f"""
[services.test]    
command = "/bin/sh"
args = ["-c", "pwd > {output_file_path}"]
working_directory = "{working_directory}"
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
    assert test.pid_or_reason == f"{REASON_ERROR}(111)"
    assert not working_directory.exists()
    assert not output_file_path.exists()
