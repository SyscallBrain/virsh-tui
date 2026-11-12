#!/usr/bin/env python3
"""Signal a real TUI while a fake foreground editor and descendant run."""
import fcntl
import os
import pty
import select
import signal
import subprocess
import struct
import sys
import tempfile
import termios
import time
from pathlib import Path

with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    binaries = root / "bin"
    binaries.mkdir()
    virsh = binaries / "virsh"
    virsh.write_text("""#!/usr/bin/env python3
import sys
args = sys.argv[1:]
if args[:1] == ['-c']:
    args = args[2:]
cmd = args[0] if args else ''
if cmd == 'domstats':
    print("Domain: 'vt-test-vm'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
elif cmd == 'dumpxml':
    print('<domain><name>vt-test-vm</name><vcpu>1</vcpu><memory>1048576</memory><currentMemory>1048576</currentMemory><devices/></domain>')
""", encoding="utf-8")
    virsh.chmod(0o755)
    editor = root / "editor.py"
    editor.write_text("""import os, signal, sys, time
from pathlib import Path
signal.signal(signal.SIGINT, signal.SIG_IGN)
if os.fork() == 0:
    signal.alarm(8)
    time.sleep(4.5)
    Path(os.environ['VT_SURVIVED']).write_text('survived')
    os._exit(0)
signal.alarm(8)
Path(os.environ['VT_EDITOR_READY']).write_text('ready')
time.sleep(30)
""", encoding="utf-8")
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
    before = termios.tcgetattr(slave)
    env = os.environ.copy()
    env.update(TERM="xterm-256color", PATH=str(binaries) + os.pathsep + env.get("PATH", ""),
               XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory,
               EDITOR=f"python3 {editor}", VISUAL="", VT_EDITOR_READY=str(root / "ready"),
               VT_SURVIVED=str(root / "survived"))
    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
    child = subprocess.Popen([sys.argv[1], "--connect", "test:///default"], stdin=slave, stdout=slave,
                             stderr=slave, env=env, preexec_fn=controlling_terminal)
    try:
        output = bytearray()
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline and b"vt-test-vm" not in output:
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                chunk = os.read(master, 65536)
                output.extend(chunk)
                if b"\x1b[6n" in chunk:
                    os.write(master, b"\x1b[1;1R")
        assert b"vt-test-vm" in output, "fake domain did not appear"
        os.write(master, b"e")
        next_key = time.monotonic() + 0.3
        deadline = time.monotonic() + 7
        while time.monotonic() < deadline and not (root / "ready").exists():
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                chunk = os.read(master, 65536)
                if b"\x1b[6n" in chunk:
                    os.write(master, b"\x1b[1;1R")
            if time.monotonic() >= next_key:
                os.write(master, b"e")
                next_key = time.monotonic() + 0.3
        assert (root / "ready").exists(), "foreground editor did not start"
        child.send_signal(signal.SIGTERM)
        assert child.wait(timeout=7) == 143, "unexpected signal exit status"
        assert termios.tcgetattr(slave) == before, "terminal attributes were not restored"
        time.sleep(1.7)
        assert not (root / "survived").exists(), "editor descendant survived shutdown"
    finally:
        if child.poll() is None:
            child.kill()
            child.wait()
        os.close(master)
        os.close(slave)
