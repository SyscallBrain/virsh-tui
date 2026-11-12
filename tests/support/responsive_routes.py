#!/usr/bin/env python3
"""Exercise command routes with a held fake backend and a real terminal loop."""
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

binary, route = sys.argv[1:]
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    binaries = root / "bin"
    binaries.mkdir()
    virsh = binaries / "virsh"
    virsh.write_text("""#!/usr/bin/env python3
import os, signal, sys, time
from pathlib import Path
args = sys.argv[1:]
if args[:1] == ['-c']: args = args[2:]
command = args[0] if args else ''
root = Path(os.environ['VT_ROOT'])
with (root / 'commands').open('a') as log: log.write(' '.join(args) + '\\n')
if command in ('start', 'reboot', 'destroy', 'net-start', 'pool-start', 'snapshot-create-as', 'change-media', 'net-define', 'net-autostart', 'vol-wipe'):
    if os.environ['VT_ROUTE'] == 'repeat' and not (root / 'first').exists():
        (root / 'first').write_text('done')
        sys.exit(0)
    signal.signal(signal.SIGINT, lambda *_: sys.exit(0))
    signal.alarm(15)
    if command == 'net-define':
        xml = Path(args[1])
        (root / 'xml-path').write_text(str(xml))
        (root / 'xml-copy').write_text(xml.read_text())
    (root / 'started').write_text('started')
    while True:
        if os.environ['VT_ROUTE'] == 'disconnect' and (root / 'disconnect').exists():
            print('connection closed during operation', file=sys.stderr)
            sys.exit(1)
        if os.environ['VT_ROUTE'] == 'completion' and (root / 'release').exists():
            sys.exit(0)
        time.sleep(0.02)
elif command == 'domstats':
    print("Domain: 'vt-test-a'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
    print("Domain: 'vt-test-b'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
elif command == 'dumpxml':
    print('<domain><name>' + args[1] + '</name><vcpu>1</vcpu><memory>1048576</memory><currentMemory>1048576</currentMemory><devices><disk type="file" device="cdrom"><target dev="hdb" bus="ide"/></disk></devices></domain>')
elif command == 'net-list':
    print('Name State Autostart Persistent\\n----\\nvt-test-net-a active yes yes\\nvt-test-net-b active yes yes')
elif command == 'pool-list':
    print('Name State Autostart Persistent Capacity Allocation Available\\n----\\nvt-test-pool-a running yes yes 10 GiB 1 GiB 9 GiB\\nvt-test-pool-b running yes yes 10 GiB 1 GiB 9 GiB')
elif command == 'net-dumpxml':
    print('<network><name>' + args[1] + '</name></network>')
elif command == 'pool-dumpxml':
    print('<pool><name>' + args[1] + '</name><target><path>/tmp</path></target></pool>')
elif command == 'vol-list':
    print('Name Path\\n----\\nvt-test-vol /tmp/vt-test-vol')
elif command == 'vol-dumpxml':
    print('<volume><name>vt-test-vol</name><capacity>1048576</capacity><allocation>0</allocation><target><format type="raw"/></target></volume>')
""", encoding="utf-8")
    virsh.chmod(0o755)
    ip = binaries / "ip"
    ip.write_text("#!/bin/sh\nprintf '[]\\n'\n", encoding="utf-8")
    ip.chmod(0o755)
    config = root / "config.toml"
    config.write_text("[confirmations]\ndestroy = false\n" if route == "no-confirm" else "", encoding="utf-8")
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
    env = os.environ.copy()
    env.update(TERM="xterm-256color", PATH=str(binaries) + os.pathsep + env.get("PATH", ""),
               XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory,
               VT_ROOT=directory, VT_ROUTE=route)
    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
    child = subprocess.Popen([binary, "--connect", "test:///default", "--config", str(config)],
                             stdin=slave, stdout=slave, stderr=slave, env=env,
                             preexec_fn=controlling_terminal)
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
        log = (root / "commands").read_text()[-1500:] if (root / "commands").exists() else "missing"
        raise AssertionError(f"{route}: {message}; commands={log!r}; terminal={bytes(output)[-500:]!r}")
    try:
        wait_for(lambda: b"vt-test-a" in output and b"vt-test-b" in output, "dashboard did not load")
        if route in ("keyboard", "wizard-draft", "disconnect", "completion"):
            os.write(master, b"s")
        elif route == "command":
            os.write(master, b":start vt-test-a\r")
        elif route == "palette":
            output.clear()
            os.write(master, b"\x10")
            wait_for(lambda: b"Command palette" in output, "palette did not open")
            os.write(master, b"start\r\r")
        elif route == "repeat":
            output.clear()
            os.write(master, b"r")
            wait_for(lambda: b"Rebooted vt-test-a" in output, "first action did not complete")
            os.write(master, b".")
        elif route in ("confirm", "no-confirm"):
            os.write(master, b"D")
            if route == "confirm":
                wait_for(lambda: b"Destroy domain" in output, "confirmation did not appear")
                assert not (root / "started").exists(), "command started before confirmation"
                os.write(master, b"y")
        elif route in ("network", "pool", "network-create", "volume"):
            output.clear()
            os.write(master, b"4" if route in ("pool", "volume") else b"3")
            label = b"vt-test-vol" if route == "volume" else b"vt-test-pool-b" if route == "pool" else b"vt-test-net-b"
            wait_for(lambda: label in output, "target view did not load")
            if route == "network-create":
                os.write(master, b"nvt-test-new\r")
            elif route == "volume":
                output.clear()
                os.write(master, b"W")
                wait_for(lambda: b"Wipe volume" in output, "volume confirmation did not appear")
                assert not (root / "started").exists(), "volume command started before confirmation"
                os.write(master, b"y")
            else:
                os.write(master, b"s")
        elif route == "snapshot":
            output.clear()
            os.write(master, b" sc")
            wait_for(lambda: b"snapshot-create-as" in output, "snapshot form did not open")
            os.write(master, b"\r")
        elif route == "media":
            os.write(master, b":attach-iso vt-test-a /tmp/vt-test.iso\r")
        else:
            raise AssertionError(f"unknown route {route}")
        wait_for(lambda: (root / "started").exists(), "command did not start")
        if route == "network-create":
            xml_path = Path((root / "xml-path").read_text())
            assert xml_path.exists(), "network XML was removed before define completed"
            assert "<name>vt-test-new</name>" in (root / "xml-copy").read_text()
        if route == "wizard-draft":
            output.clear()
            os.write(master, b" n")
            wait_for(lambda: b"New domain" in output, "new wizard did not open while busy")
            os.write(master, b"vt-test-new\r")
            draft = root / "virsh-tui" / "wizard-draft.toml"
            wait_for(lambda: draft.exists() and 'name = "vt-test-new"' in draft.read_text()
                     and 'step = 1' in draft.read_text(), "new draft did not accept input or advance")
            os.write(master, b"\x1b")
            time.sleep(0.2)
        if route == "snapshot":
            output.clear()
            os.write(master, b"q")
            wait_for(lambda: b"vt-test-b" in output, "could not leave detail during snapshot job")
        log_before = (root / "commands").stat().st_size
        os.write(master, b"j")
        selection_command = {"network": "net-dhcp-leases vt-test-net-b", "network-create": "net-dhcp-leases vt-test-net-b", "pool": "vol-list --pool vt-test-pool-b", "volume": "vol-list --pool vt-test-pool-b"}.get(
            route, "dumpxml --inactive vt-test-b")
        wait_for(lambda: selection_command in (root / "commands").read_text()[log_before:],
                 "navigation was blocked")
        output.clear()
        os.write(master, b":start vt-test-b\r")
        wait_for(lambda: b"is running" in output, "second command was not refused")
        time.sleep(0.1)
        mutations = [line for line in (root / "commands").read_text().splitlines()
                     if line.split()[:1] and line.split()[0] in ('start', 'reboot', 'destroy', 'net-start', 'pool-start', 'snapshot-create-as', 'change-media', 'net-define', 'net-autostart', 'vol-wipe')]
        verb = "reboot" if route == "repeat" else "destroy" if route in ("confirm", "no-confirm") else "start"
        expected = {"network": ["net-start vt-test-net-a"], "pool": ["pool-start vt-test-pool-a"]}.get(
            route, [f"{verb} vt-test-a"] * (2 if route == "repeat" else 1))
        if route in ("snapshot", "media", "network-create", "volume"):
            prefix = {"snapshot": "snapshot-create-as vt-test-a ", "media": "change-media vt-test-a hdb /tmp/vt-test.iso", "network-create": "net-define ", "volume": "vol-wipe --pool vt-test-pool-a vt-test-vol --algorithm zero"}[route]
            assert len(mutations) == 1 and mutations[0].startswith(prefix), f"unexpected command executions: {mutations!r}"
            expected = mutations.copy()
        assert mutations == expected, f"unexpected command executions: {mutations!r}"
        output.clear()
        if route == "disconnect":
            (root / "disconnect").write_text("disconnect")
            wait_for(lambda: b"connection closed during operation" in output, "disconnect was not reported")
        elif route == "completion":
            (root / "release").write_text("release")
            wait_for(lambda: b"Started vt-test-a" in output, "completion did not retain the original target")
        else:
            os.write(master, b"\x03")
            wait_for(lambda: b"stopped locally" in output, "cancellation did not complete")
        if route == "network-create":
            assert not xml_path.exists(), "network XML survived cancellation"
            output.clear()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 42, 173, 0, 0))
            wait_for(lambda: b"vt-test-new" in output, "cancelled network draft was not restored")
            os.write(master, b"\x1b")
            time.sleep(0.15)
        time.sleep(0.15)
        mutations = [line for line in (root / "commands").read_text().splitlines()
                     if line.split()[:1] and line.split()[0] in ('start', 'reboot', 'destroy', 'net-start', 'pool-start', 'snapshot-create-as', 'change-media', 'net-define', 'net-autostart', 'vol-wipe')]
        assert mutations == expected, f"refused command executed later: {mutations!r}"
        os.write(master, b"q")
        wait_for(lambda: child.poll() is not None, "application did not quit")
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
