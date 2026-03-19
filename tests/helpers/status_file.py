# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

from dataclasses import dataclass
from pathlib import Path
from typing import Self

from constants import STATE_RUNNING, STATE_STOPPED, STATUS_FILE_NAME


@dataclass
class StatusLine:
    service_name: str
    service_id: int
    state: str
    pid_or_reason: str

    def __repr__(self) -> str:
        return (
            f"StatusLine(id={self.service_id}, "
            f"name={self.service_name}, "
            f"state={self.state}, "
            f"extra={self.pid_or_reason})"
        )


class StatusFile:
    def __init__(self, lines: list[StatusLine]):
        self.lines = lines

    @classmethod
    def from_path(cls, path: Path) -> Self:
        lines = []

        with open(path, "r") as f:
            for raw in f:
                raw = raw.strip()
                if not raw:
                    continue

                parts = raw.split()
                if len(parts) < 4:
                    raise ValueError(f"invalid status line: {raw}")

                service_name = parts[0]
                try:
                    service_id = int(parts[1])
                except ValueError:
                    raise ValueError(
                        f"invalid service_id in status line: {raw}"
                    ) from None
                state = parts[2]
                pid_or_reason = parts[3]

                lines.append(
                    StatusLine(
                        service_name,
                        service_id,
                        state,
                        pid_or_reason,
                    )
                )

        return cls(lines)

    def get(self, service_name: str) -> StatusLine:
        for line in self.lines:
            if line.service_name == service_name:
                return line
        raise KeyError(service_name)

    def is_running(self, service_name: str) -> bool:
        return self.get(service_name).state == STATE_RUNNING

    def is_stopped(self, service_name: str) -> bool:
        return self.get(service_name).state == STATE_STOPPED


def read_status(run_dir: Path) -> StatusFile:
    return StatusFile.from_path(run_dir / STATUS_FILE_NAME)
