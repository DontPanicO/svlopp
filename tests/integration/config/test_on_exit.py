# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from constants import CONFIG_FILE_NAME
from helpers.status_file import read_status
from helpers.utils import wait_until


def test_on_exit_restart(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"

    config_path.write_text(
        f"""
[services.test]
command = "/bin/sh"
args = ["-c", "date +%s%N >> {output_file_path}"]
on_exit = "Restart"
"""
    )

    _ = svlopp_proc(config_path)

    def has_test_run_multiple_times():
        try:
            return len(output_file_path.read_text().strip().splitlines()) > 1
        except FileNotFoundError:
            return False

    wait_until(has_test_run_multiple_times, timeout=2.0)

    lines = output_file_path.read_text().strip().splitlines()

    assert len(lines) > 1
    assert len(set(lines)) == len(lines)


# this tests that the default on_exit behavior is `None` (i.e. do
# nothing).
# The on_exit operations are executed in the timerfd path. The timerfd
# fires once per second, but solely relying on a `time.sleep(1)`
# is not robust enough. For this reason we add a second service
# with `on_exit = "Restart"` so that after it's restarted we know
# that the timerfd has fired and we can make the assertion against
# the actual service to be tested.
def test_on_exit_none_default(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"
    marker_file_path = tmp_path / "marker"

    config_path.write_text(
        f"""
[services.a]
command = "/bin/sh"
args = ["-c", "date +%s%N >> {output_file_path}"]

[services.b]
command = "/bin/sh"
args = ["-c", "echo tick >> {marker_file_path}"]
on_exit = "Restart"
"""
    )

    _ = svlopp_proc(config_path)

    def has_b_run_multiple_times():
        try:
            return len(marker_file_path.read_text().splitlines()) > 1
        except FileNotFoundError:
            return False

    wait_until(has_b_run_multiple_times, timeout=3.0)

    lines = output_file_path.read_text().strip().splitlines()
    status = read_status(run_dir)

    assert len(lines) == 1
    assert status.is_stopped("a")


def test_on_exit_remove(tmp_path, run_dir, svlopp_proc):
    config_path = tmp_path / CONFIG_FILE_NAME
    output_file_path = tmp_path / "output"
    marker_file_path = tmp_path / "marker"

    config_path.write_text(
        f"""
[services.a]
command = "/bin/sh"
args = ["-c", "date +%s%N >> {output_file_path}"]
on_exit = "Remove"

[services.b]
command = "/bin/sh"
args = ["-c", "echo tick >> {marker_file_path}"]
on_exit = "Restart"
"""
    )

    _ = svlopp_proc(config_path)

    def has_b_run_multiple_times():
        try:
            return len(marker_file_path.read_text().splitlines()) > 1
        except FileNotFoundError:
            return False

    wait_until(has_b_run_multiple_times, timeout=3.0)

    lines = output_file_path.read_text().strip().splitlines()
    status = read_status(run_dir)

    assert len(lines) == 1
    assert not status.has("a")
