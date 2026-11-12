#!/usr/bin/env python3
import os
import pty
import select
import signal
import subprocess
import sys
import tempfile
import termios
import time

master, slave = pty.openpty()
before = termios.tcgetattr(slave)
with tempfile.TemporaryDirectory() as directory:
    env = os.environ.copy()
    env.update(TERM="xterm-256color", XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory)
    child = subprocess.Popen([sys.argv[1], "--demo"], stdin=slave, stdout=slave, stderr=slave, env=env)
    try:
        deadline = time.monotonic() + 5
        output = b""
        while time.monotonic() < deadline and b"\x1b[?1049h" not in output:
            ready, _, _ = select.select([master], [], [], 0.1)
            if ready:
                chunk = os.read(master, 65536)
                output += chunk
                if b"\x1b[6n" in chunk:
                    os.write(master, b"\x1b[1;1R")
        if b"\x1b[?1049h" not in output:
            raise AssertionError("alternate screen not entered")
        # Allow the runtime to register its signal hooks after terminal setup.
        time.sleep(0.3)
        child.send_signal(signal.SIGTERM)
        code = child.wait(timeout=6)
        after = termios.tcgetattr(slave)
        assert code == 143, f"unexpected exit status: {code}"
        assert after == before, "terminal attributes were not restored"
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
os.close(master)
os.close(slave)
