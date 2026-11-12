#!/usr/bin/env python3
"""Check signal shutdown with a held fake command or background read."""
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

binary, mode, signal_name = sys.argv[1:]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    bin_dir = root / "bin"
    bin_dir.mkdir()
    marker = root / "held-pid"
    virsh = bin_dir / "virsh"
    virsh.write_text("""#!/usr/bin/env python3
import os, signal, sys, time
from pathlib import Path
args = sys.argv[1:]
if args[:1] == ['-c']: args = args[2:]
command = args[0] if args else ''
if command == ('domstats' if os.environ['VT_MODE'] == 'read' else 'start'):
    signal.alarm(12)
    if os.environ['VT_MODE'] == 'read' and os.fork() == 0:
        time.sleep(1.2)
        Path(os.environ['VT_SURVIVOR']).write_text('survived')
        sys.exit(0)
    Path(os.environ['VT_MARKER']).write_text(str(os.getpid()))
    while True: time.sleep(0.05)
if command == 'domstats':
    print("Domain: 'vt-test-a'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
""", encoding="utf-8")
    virsh.chmod(0o755)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
    before = termios.tcgetattr(slave)
    env = os.environ.copy()
    env.update(TERM="xterm-256color", PATH=str(bin_dir) + os.pathsep + env.get("PATH", ""),
               XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory,
               VT_MARKER=str(marker), VT_MODE=mode, VT_SURVIVOR=str(root / "survived"))
    child = subprocess.Popen([binary, "--connect", "test:///default"], stdin=slave, stdout=slave,
                             stderr=slave, env=env)
    output = bytearray()
    def pump():
        ready, _, _ = select.select([master], [], [], 0.05)
        if ready:
            try:
                data = os.read(master, 65536)
            except OSError:
                return
            output.extend(data)
            if b"\x1b[6n" in data:
                os.write(master, b"\x1b[1;1R")
    def wait_for(predicate, message):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if predicate():
                return
            pump()
        raise AssertionError(message)
    try:
        if mode == "job":
            wait_for(lambda: b"vt-test-a" in output, "dashboard did not load")
            os.write(master, b":start vt-test-a\r")
        wait_for(marker.exists, f"{mode} process did not start")
        held_pid = int(marker.read_text())
        requested_signal = getattr(signal, signal_name)
        child.send_signal(requested_signal)
        deadline = time.monotonic() + 8
        while child.poll() is None and time.monotonic() < deadline:
            pump()
        assert child.poll() == 128 + requested_signal, f"unexpected exit status: {child.poll()}"
        assert termios.tcgetattr(slave) == before, "terminal attributes were not restored"
        try:
            os.kill(held_pid, 0)
        except ProcessLookupError:
            pass
        else:
            raise AssertionError(f"held {mode} process survived shutdown")
        if mode == "read":
            time.sleep(1.4)
            assert not (root / "survived").exists(), "held read descendant survived shutdown"
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
        os.close(master)
        os.close(slave)
