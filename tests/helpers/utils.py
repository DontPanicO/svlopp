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
