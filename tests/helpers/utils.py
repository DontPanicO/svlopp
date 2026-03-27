# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from pathlib import Path
import time


def wait_until(cond, timeout=1.0, interval=0.01):
    start = time.time()
    while time.time() - start < timeout:
        if cond():
            return True
        time.sleep(interval)
    raise TimeoutError("condition not met within timeout")


def is_zombie(pid: int) -> bool:
    try:
        with open(f"/proc/{pid}/stat", "r") as f:
            stat = f.read()
        state = stat.split()[2]
        return state == "Z"
    except FileNotFoundError:
        return False


def pid_exists(pid: int) -> bool:
    return Path(f"/proc/{pid}").exists()
