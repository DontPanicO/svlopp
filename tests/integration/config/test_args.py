# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from constants import CONFIG_FILE_NAME
from helpers.utils import wait_until
from helpers.status_file import read_status


def test_args_happy_path(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"

    first = "hello"
    second = "world"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "/bin/echo {first} {second} > {output_file_path}"]
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
    assert content == f"{first} {second}"
