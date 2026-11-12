#!/usr/bin/env python3
"""Exercise XML validation retry with an isolated fake backend and editor."""
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

mode = sys.argv[2] if len(sys.argv) > 2 else "domain"
network = mode == "network"
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    binaries = root / "bin"
    binaries.mkdir()
    virsh = binaries / "virsh"
    virsh.write_text("""#!/usr/bin/env python3
import os,sys
from pathlib import Path
args = sys.argv[1:]
if args[:1] == ['-c']: args = args[2:]
cmd = args[0] if args else ''
if cmd == 'domstats':
    print("Domain: 'vt-test-vm'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
elif cmd == 'dumpxml':
    if '--security-info' in args:
        Path(os.environ['VT_SECURE_READ']).write_text('secure')
    print('<domain><name>vt-test-vm</name><vcpu>1</vcpu><memory>1048576</memory><currentMemory>1048576</currentMemory><devices/></domain>')
elif cmd == 'net-list':
    print(' Name State Autostart Persistent\\n--------------------------------\\n vt-test-net active yes yes')
elif cmd == 'net-dumpxml':
    print('<network><name>vt-test-net</name></network>')
elif cmd in ('define', 'net-define'):
    calls = Path(os.environ['VT_DEFINE_CALLS'])
    with calls.open('a', encoding='utf-8') as f: f.write(args[1] + '\\n')
    if len(calls.read_text().splitlines()) == 1:
        print('validation failed intentionally', file=sys.stderr)
        sys.exit(1)
""", encoding="utf-8")
    virsh.chmod(0o755)
    install = binaries / "virt-install"
    install.write_text("""#!/usr/bin/env python3
import os,time
from pathlib import Path
if os.environ.get('VT_DELAY_EDITOR'):
    Path(os.environ['VT_READY']).write_text('ready')
    while not Path(os.environ['VT_RELEASE']).exists(): time.sleep(.02)
print('<domain><name>vt-test-new</name></domain>')
""", encoding="utf-8")
    install.chmod(0o755)
    editor = root / "editor.py"
    editor.write_text("""import os,sys
from pathlib import Path
calls = Path(os.environ['VT_EDITOR_CALLS'])
with calls.open('a', encoding='utf-8') as f: f.write('edit\\n')
count = len(calls.read_text().splitlines())
path = Path(sys.argv[1])
text = path.read_text()
if count == 1:
    closing = '</network>' if '</network>' in text else '</domain>'
    path.write_text(text.replace(closing, '<description>first</description>' + closing))
else:
    assert 'validation failed intentionally' in text
    Path(os.environ['VT_RETRY_SEEN']).write_text('yes')
    path.write_text(text.replace('first', 'second'))
""", encoding="utf-8")
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
    env = os.environ.copy()
    env.update(TERM="xterm-256color", PATH=str(binaries) + os.pathsep + env.get("PATH", ""),
               XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory,
               EDITOR=f"python3 {editor}", VISUAL="", VT_EDITOR_CALLS=str(root / "editor-calls"),
               VT_DEFINE_CALLS=str(root / "define-calls"), VT_RETRY_SEEN=str(root / "retry-seen"),
               VT_READY=str(root / "ready"), VT_RELEASE=str(root / "release"),
               VT_SECURE_READ=str(root / "secure-read"))
    if mode == "wizard-delay": env["VT_DELAY_EDITOR"] = "1"
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
                if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
        assert b"vt-test-vm" in output, "fake domain did not appear"
        if network:
            os.write(master, b"3")
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline and b"vt-test-net" not in output:
                ready, _, _ = select.select([master], [], [], 0.05)
                if ready:
                    chunk = os.read(master, 65536)
                    output.extend(chunk)
                    if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
            assert b"vt-test-net" in output, "fake network did not appear"
        if mode in ("wizard", "wizard-delay"):
            os.write(master, b" n")
            time.sleep(0.3)
        edit_key = b"\x05" if mode in ("wizard", "wizard-delay") else b"e"
        os.write(master, edit_key)
        if mode == "wizard-delay":
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline and not (root / "ready").exists():
                ready, _, _ = select.select([master], [], [], 0.05)
                if ready:
                    chunk = os.read(master, 65536)
                    if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
            assert (root / "ready").exists(), "wizard XML preparation did not start"
            os.write(master, b"zzzz")
            drawn = bytearray()
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline and drawn.count(b"z") < 4:
                ready, _, _ = select.select([master], [], [], 0.05)
                if ready:
                    chunk = os.read(master, 65536)
                    drawn.extend(chunk)
                    if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
            assert drawn.count(b"z") >= 4, "wizard did not redraw during preparation"
            (root / "release").write_text('release')
            deadline = time.monotonic() + 0.7
            while time.monotonic() < deadline:
                ready, _, _ = select.select([master], [], [], 0.05)
                if ready:
                    chunk = os.read(master, 65536)
                    if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
            assert not (root / "editor-calls").exists(), "stale XML opened the editor"
            assert not (root / "define-calls").exists(), "stale XML was defined"
            sys.exit(0)
        deadline = time.monotonic() + 8
        next_key = time.monotonic() + 0.3
        while time.monotonic() < deadline and not (root / "retry-seen").exists():
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                chunk = os.read(master, 65536)
                if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
            if not (root / "editor-calls").exists() and time.monotonic() >= next_key:
                os.write(master, edit_key)
                next_key = time.monotonic() + 0.3
        assert (root / "retry-seen").exists(), "validation error did not reopen the editor"
        if mode == "domain":
            assert (root / "secure-read").exists(), "domain editor did not request security information"
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                chunk = os.read(master, 65536)
                if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
            if (root / "define-calls").exists() and len((root / "define-calls").read_text().splitlines()) >= 2:
                break
        paths = (root / "define-calls").read_text().splitlines()
        assert len(paths) == 2, f"expected two validation attempts, got {len(paths)}"
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and any(Path(path).exists() for path in paths):
            time.sleep(0.02)
        assert not any(Path(path).exists() for path in paths), "validation XML file leaked"
    finally:
        if child.poll() is None:
            child.send_signal(signal.SIGTERM)
            try: child.wait(timeout=6)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        os.close(master)
        os.close(slave)
