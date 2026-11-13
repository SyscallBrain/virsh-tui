#!/usr/bin/env python3
"""Exercise delayed backend reads through the production terminal loop."""
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

binary, mode = sys.argv[1:]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    binaries = root / "bin"
    binaries.mkdir()
    virsh = binaries / "virsh"
    virsh.write_text("""#!/usr/bin/env python3
import os, signal, sys, time
from pathlib import Path
args = sys.argv[1:]
uri = args[1] if args[:1] == ['-c'] and len(args) > 1 else ''
if args[:1] == ['-c']: args = args[2:]
command = args[0] if args else ''
root = Path(os.environ['VT_ROOT'])
mode = os.environ['VT_MODE']
if uri == 'test:///vt-test-other': (root / 'new-connection').write_text('ready')
with (root / 'commands').open('a') as log: log.write(' '.join(args) + '\\n')
hold = ((mode == 'startup' and command == 'domstats') or
        (mode in ('detail', 'description-prep') and args[:2] == ['dumpxml', 'vt-test-a'] and '--inactive' in args) or
        (mode == 'snapshots' and command == 'snapshot-list') or
        (mode in ('picker', 'export-prep') and args == ['dumpxml', 'vt-test-a']) or
        (mode == 'hostdev-prep' and command == 'nodedev-dumpxml') or
        (mode in ('viewer', 'viewer-switch') and command == 'domdisplay') or
        (mode in ('palette', 'completion') and command == 'help') or
        (mode == 'network' and command == 'net-dhcp-leases' and 'vt-test-net-b' in args and (root / 'armed').exists()) or
        (mode == 'pool' and command == 'vol-list' and 'vt-test-pool-b' in args and (root / 'armed').exists()) or
        (mode == 'refresh' and command == 'pool-list' and (root / 'armed').exists()))
if hold:
    signal.alarm(15)
    (root / 'started').write_text(command)
    while not (root / 'release').exists(): time.sleep(0.02)
if command == 'domstats':
    print("Domain: 'vt-test-a'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
    print("Domain: 'vt-test-b'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
elif command == 'dumpxml':
    name = next((arg for arg in args[1:] if not arg.startswith('-')), 'vt-test-a')
    print('<domain><name>' + name + '</name><vcpu>1</vcpu><memory>1048576</memory><currentMemory>1048576</currentMemory><devices/></domain>')
elif command == 'net-list':
    print('Name State Autostart Persistent\\n----\\nvt-test-net-a active yes yes\\nvt-test-net-b active yes yes')
elif command == 'pool-list':
    print('Name State Autostart Persistent Capacity Allocation Available\\n----\\nvt-test-pool-a running yes yes 10 GiB 1 GiB 9 GiB\\nvt-test-pool-b running yes yes 10 GiB 1 GiB 9 GiB')
elif command == 'net-dumpxml':
    print('<network><name>' + args[1] + '</name></network>')
elif command == 'pool-dumpxml':
    print('<pool><name>' + args[1] + '</name><target><path>/tmp</path></target></pool>')
elif command == 'domdisplay':
    print('spice://127.0.0.1:5900')
""", encoding="utf-8")
    virsh.chmod(0o755)
    ip = binaries / "ip"
    ip.write_text("#!/bin/sh\nprintf '[]\\n'\n", encoding="utf-8")
    ip.chmod(0o755)
    for name in ("virt-viewer", "remote-viewer"):
        viewer = binaries / name
        viewer.write_text("#!/bin/sh\ntouch \"$VT_ROOT/viewer-opened\"\n", encoding="utf-8")
        viewer.chmod(0o755)
    config = root / "config.toml"
    config.write_text('[console]\nviewer = "remote-viewer"\n', encoding="utf-8")
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
    env = os.environ.copy()
    env.update(TERM="xterm-256color", PATH=str(binaries) + os.pathsep + env.get("PATH", ""),
               XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory,
               VT_ROOT=directory, VT_MODE=mode)
    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
    child = subprocess.Popen([binary, "--connect", "test:///default", "--config", str(config)], stdin=slave, stdout=slave,
                             stderr=slave, env=env, cwd=directory, preexec_fn=controlling_terminal)
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
    def wait_for(predicate, timeout, message):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            pump()
        log = (root / "commands").read_text()[-1500:] if (root / "commands").exists() else "missing"
        raise AssertionError(f"{mode}: {message}; commands={log!r}; terminal={bytes(output)[-500:]!r}")
    def type_and_redraw(data, expected, message):
        os.write(master, data)
        for _ in range(4):
            pump()
        output.clear()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 42, 173, 0, 0))
        wait_for(lambda: expected in output, 1, message)
    try:
        if mode != "startup":
            wait_for(lambda: b"vt-test-a" in output and b"vt-test-b" in output, 8, "dashboard did not load")
            if mode in ("network", "pool"):
                os.write(master, b"3" if mode == "network" else b"4")
                label = b"vt-test-net-b" if mode == "network" else b"vt-test-pool-b"
                wait_for(lambda: label in output, 8, "list did not load")
                (root / "armed").write_text("armed")
            triggers = {
                "detail": b"\r", "snapshots": b" sc", "picker": b" mi", "viewer": b"v",
                "palette": b"\x10", "completion": b":do\t", "refresh": b"\x12",
                # Pool volumes are cached per view load, so moving the cursor
                # reads nothing: an explicit refresh (C-r) reloads the view
                # and runs the held `vol-list` of the second pool.
                "network": b"j", "pool": b"\x12", "viewer-switch": b"v",
                "description-prep": b":desc vt-test-a new description\r",
                "hostdev-prep": b":attach-hostdev vt-test-a vt-test-device\r",
                "export-prep": b" x",
            }
            if mode == "refresh":
                (root / "armed").write_text("armed")
            os.write(master, triggers[mode])
        wait_for(lambda: (root / "started").exists(), 8, "read did not start")
        assert not (root / "release").exists()
        output.clear()
        if mode == "viewer-switch":
            os.write(master, b":connect test:///vt-test-other\r")
            wait_for(lambda: (root / "new-connection").exists(), 8, "connection did not change")
            (root / "release").write_text("release")
            deadline = time.monotonic() + 0.5
            while time.monotonic() < deadline:
                pump()
            assert not (root / "viewer-opened").exists(), "old connection opened a viewer after switching"
        elif mode in ("startup", "network", "pool"):
            os.write(master, b"?")
            wait_for(lambda: b"Keybindings" in output, 1, "help was blocked by startup")
            os.write(master, b"\x1b")
            time.sleep(0.2)
        elif mode in ("detail", "snapshots"):
            os.write(master, b"q\x0c")
            wait_for(lambda: b"vt-test-b" in output, 1, "back navigation was blocked")
        elif mode == "picker":
            type_and_redraw(b"abc", b"abc", "picker input was blocked")
            os.write(master, b"\x1b")
            time.sleep(0.2)
        elif mode == "palette":
            type_and_redraw(b"snap", b"snap", "palette input was blocked")
            os.write(master, b"\x1b")
            time.sleep(0.2)
        elif mode == "completion":
            type_and_redraw(b"x", b"dox", "command-line input was blocked")
            os.write(master, b"\x1b")
            time.sleep(0.2)
        else:
            os.write(master, b"j")
            wait_for(lambda: b"\x1b[1mvt-test-b" in output, 1, "navigation was blocked")
        if mode != "viewer-switch":
            assert not (root / "release").exists(), "read returned before the input check"
        if mode.endswith("-prep"):
            output.clear()
            os.write(master, b"\x03")
            wait_for(lambda: b"stopped locally" in output, 8, "preparation did not cancel")
            commands = (root / "commands").read_text().splitlines()
            assert not any(line.split()[:1] in (["define"], ["attach-device"]) for line in commands), commands
            assert not (root / "vt-test-a.xml").exists(), "cancelled preparation wrote an export"
        os.write(master, b"q")
        wait_for(lambda: child.poll() is not None, 8, "application did not quit")
        assert child.returncode == 0, f"unexpected exit status: {child.returncode}"
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
