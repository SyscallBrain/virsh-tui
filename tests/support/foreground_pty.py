#!/usr/bin/env python3
"""Exercise foreground process-group handoff in an isolated pseudo-terminal."""
import os
import pty
import select
import signal
import sys
import time
from pathlib import Path

binary, helper, marker = sys.argv[1:]
pid, master = pty.fork()
if pid == 0:
    env = os.environ.copy()
    env["VT_FOREGROUND_PTY_CHILD"] = helper
    env["VT_FOREGROUND_PTY_MARKER"] = marker
    os.execve(binary, [binary, "--exact", "app::external::tests::foreground_terminal_is_given_to_child_and_restored", "--nocapture"], env)

output = bytearray()
sent = False
status = None
deadline = time.monotonic() + 8
try:
    while time.monotonic() < deadline:
        if not sent and Path(marker).exists():
            os.write(master, b"x\n")
            sent = True
        ready, _, _ = select.select([master], [], [], 0.02)
        if ready:
            try:
                output.extend(os.read(master, 65536))
            except OSError:
                pass
        finished, child_status = os.waitpid(pid, os.WNOHANG)
        if finished:
            status = child_status
            break
    if status is None:
        os.kill(pid, signal.SIGKILL)
        _, status = os.waitpid(pid, 0)
        raise AssertionError("foreground PTY test timed out")
    if not sent or not os.WIFEXITED(status) or os.WEXITSTATUS(status) != 0:
        raise AssertionError(f"foreground PTY test failed: {bytes(output)!r}")
finally:
    os.close(master)
