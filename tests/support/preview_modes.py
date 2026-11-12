#!/usr/bin/env python3
"""Exercise preview modes with a real TUI and fake executables only."""
import fcntl
import os
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path

binary, mode = sys.argv[1:]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    binaries = root / "bin"
    binaries.mkdir()
    fake = """#!/usr/bin/env python3
import os, sys
from pathlib import Path
args = sys.argv[1:]
if args[:1] == ['-c']: args = args[2:]
program = Path(sys.argv[0]).name
with open(os.environ['VT_LOG'], 'a') as log:
    log.write(program + ' ' + ' '.join(args) + '\\n')
if program == 'virsh' and args[:1] == ['domstats']:
    print("Domain: 'vt-test-a'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
elif program == 'virsh' and args[:1] == ['dumpxml']:
    print('<domain><name>vt-test-a</name><vcpu>1</vcpu><memory>1048576</memory><currentMemory>1048576</currentMemory><devices/></domain>')
"""
    for name in ("virsh", "virt-install", "ip"):
        path = binaries / name
        path.write_text(fake, encoding="utf-8")
        path.chmod(0o755)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
    env = os.environ.copy()
    env.update(TERM="xterm-256color", PATH=str(binaries) + os.pathsep + env.get("PATH", ""),
               XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory,
               VT_LOG=str(root / "commands"))
    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
    child = subprocess.Popen([binary, "--connect", "test:///default", "--demo" if mode == "demo" else "--dry-run"],
                             stdin=slave, stdout=slave, stderr=slave, env=env, preexec_fn=controlling_terminal)
    output = bytearray()
    def pump():
        ready, _, _ = select.select([master], [], [], 0.04)
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
        clean = re.sub(rb"\x1b\[[0-9;?]*[A-Za-z]", b"", bytes(output))
        raise AssertionError(f"{mode}: {message}; terminal={clean[:1500]!r}")
    try:
        wait_for(lambda: (b"arch-dev" if mode == "demo" else b"vt-test-a") in output,
                 "dashboard did not load")
        commands = [
            (b":start vt-test-a\r", b"Started vt-test-a"),
            (b":desc vt-test-a description\r", b"Described vt-test-a"),
            (b":attach-hostdev vt-test-a pci_test\r", b"Attached pci_test"),
            (b":attach-iso vt-test-a hdb /tmp/vt-test.iso\r", b"vt-test.iso"),
            (b":dominfo vt-test-a\r", b"dominfo vt-test-a"),
        ]
        for index, (command, expected) in enumerate(commands):
            output.clear()
            os.write(master, command)
            deadline = time.monotonic() + 0.2
            while time.monotonic() < deadline:
                pump()
            output.clear()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 173 + index % 2, 0, 0))
            wait_for(lambda: expected in output, f"preview missing for {command!r}")
        os.write(master, b"q")
        wait_for(lambda: child.poll() is not None, "application did not quit")
        assert child.returncode == 0
        lines = (root / "commands").read_text().splitlines() if (root / "commands").exists() else []
        if mode == "demo":
            assert not lines, f"demo started external programs: {lines!r}"
        else:
            forbidden = {"start", "define", "attach-device", "change-media", "nodedev-dumpxml", "dominfo"}
            for line in lines:
                words = line.split()
                assert not (words[0] == "virt-install" or
                            (words[0] == "virsh" and len(words) > 1 and words[1] in forbidden)), lines
    finally:
        if child.poll() is None:
            child.send_signal(signal.SIGTERM)
            try:
                child.wait(timeout=6)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        os.close(master)
        os.close(slave)
