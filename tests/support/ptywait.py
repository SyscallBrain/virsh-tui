"""Wait for a child on a pty while draining what it writes.

macOS does not let a process finish exiting while its pty still holds output
that nobody has read: it stays in the exiting state, even after SIGKILL. A
plain `child.wait()` that stops reading the master therefore blocks forever
once the child writes on its way out (restoring the terminal, for instance).
Linux does not block there, which is why only macOS hung.
"""
import os
import select
import signal
import subprocess
import time


def wait(child, master, timeout):
    """`child.wait(timeout)`, reading and discarding `master` meanwhile."""
    deadline = time.monotonic() + timeout
    while child.poll() is None:
        left = deadline - time.monotonic()
        if left <= 0:
            raise subprocess.TimeoutExpired(child.args, timeout)
        ready, _, _ = select.select([master], [], [], min(left, 0.05))
        if ready:
            try:
                os.read(master, 65536)
            except OSError:
                pass
    return child.returncode


def kill(child, master):
    """SIGKILL the child if it is still running and reap it, without hanging."""
    if child.poll() is None:
        child.kill()
        wait(child, master, 6)


def stop(child, master, timeout=6):
    """SIGTERM the child if it is still running, then SIGKILL it after `timeout`."""
    if child.poll() is not None:
        return
    child.send_signal(signal.SIGTERM)
    try:
        wait(child, master, timeout)
    except subprocess.TimeoutExpired:
        kill(child, master)
