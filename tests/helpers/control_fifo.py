# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from pathlib import Path


def send_control_op(run_dir: Path, opcode: int, service_id: int):
    fifo_path = run_dir / "control"
    payload = bytes([opcode]) + service_id.to_bytes(8, "little")
    with open(fifo_path, "wb", buffering=0) as fh:
        fh.write(payload)
