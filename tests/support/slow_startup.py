#!/usr/bin/env python3
import fcntl
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    binary_dir = root / "bin"
    binary_dir.mkdir()
    pid_dir = root / "pids"
    pid_dir.mkdir()
    args_dir = root / "args"
    args_dir.mkdir()
    fake = binary_dir / "virsh"
    fake.write_text("#!/bin/sh\nprintf '%s' \"$$\" > \"$VT_PID_DIR/$$\"\nprintf '%s' \"$*\" > \"$VT_ARGS_DIR/$$\"\n: > \"$VT_STARTED\"\nwhile [ ! -f \"$VT_RELEASE\" ]; do /bin/sleep 0.05; done\nprintf ' Id Name State\\n'\n", encoding="utf-8")
    fake.chmod(0o755)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
    env = os.environ.copy()
    env.update(TERM="xterm-256color", PATH=str(binary_dir) + os.pathsep + env.get("PATH", ""),
               VT_STARTED=str(root / "started"), VT_RELEASE=str(root / "release"),
               VT_PID_DIR=str(pid_dir), VT_ARGS_DIR=str(args_dir),
               XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory)
    child = subprocess.Popen([sys.argv[1], "--connect", "test:///default"], stdin=slave, stdout=slave, stderr=slave, env=env)
    try:
        deadline = time.monotonic() + 2
        output = b""
        while time.monotonic() < deadline and b"virsh-tui" not in output:
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                chunk = os.read(master, 65536)
                output += chunk
                if b"\x1b[6n" in chunk:
                    os.write(master, b"\x1b[1;1R")
        assert b"virsh-tui" in output, "first frame waited for the fake backend"
        assert not (root / "release").exists(), "backend was released before the frame"
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and not (root / "started").exists():
            time.sleep(0.02)
        assert (root / "started").exists(), "fake backend did not start"
        os.write(master, b":connect test:///vt-test-next\r")
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if any("test:///vt-test-next" in path.read_text() for path in args_dir.iterdir()):
                break
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                chunk = os.read(master, 65536)
                if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
        assert any("test:///vt-test-next" in path.read_text() for path in args_dir.iterdir()), "connect waited for backend"
    finally:
        child.send_signal(signal.SIGTERM)
        try:
            child.wait(timeout=6)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
        if sys.platform.startswith("linux"):
            def alive(pid):
                try:
                    text = Path(f"/proc/{pid}/stat").read_text()
                    return text[text.rfind(")") + 2] != "Z"
                except (FileNotFoundError, ProcessLookupError):
                    return False
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline and any(alive(p.name) for p in pid_dir.iterdir()):
                time.sleep(0.02)
            assert not any(alive(p.name) for p in pid_dir.iterdir()), "backend read survived signal shutdown"
        os.close(master)
        os.close(slave)
