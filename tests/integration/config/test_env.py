# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from constants import CONFIG_FILE_NAME
from helpers.utils import wait_until
from helpers.status_file import read_status


def test_env_happy_path(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"
    foo = "BAR"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "/bin/echo $FOO > {output_file_path}"]

[services.test.env]
FOO = "{foo}"
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
    assert content == foo


def test_env_var_override(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"
    home = "/home/doesnotexists"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "/bin/echo $HOME > {output_file_path}"]

[services.test.env]
HOME = "{home}"
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
    assert content == home


def test_empty_env_replaces_parent(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "/bin/echo $HOME > {output_file_path}"]

[services.test.env]
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
    assert content == ""
