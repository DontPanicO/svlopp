import subprocess
from pathlib import Path

import pytest

from constants import RUN_DIR_NAME, SVLOPP_BINARY_PATH


@pytest.fixture
def run_dir(tmp_path: Path) -> Path:
    return tmp_path / RUN_DIR_NAME


@pytest.fixture
def svlopp_bin() -> Path:
    return Path(SVLOPP_BINARY_PATH)


@pytest.fixture
def svlopp_proc(svlopp_bin: Path, run_dir: Path):
    procs = []

    def _run(config_path):
        proc = subprocess.Popen(
            [svlopp_bin, "--run-dir", str(run_dir), str(config_path)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        procs.append(proc)
        return proc

    yield _run

    for p in procs:
        p.terminate()
        try:
            p.wait(timeout=2)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait()
