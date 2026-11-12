#!/usr/bin/env python3
"""Exercise the production TUI loop against a held, entirely fake backend."""
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
    binaries = root / "bin"
    binaries.mkdir()
    virsh = binaries / "virsh"
    virsh.write_text("""#!/usr/bin/env python3
import os,signal,sys,time
from pathlib import Path
args = sys.argv[1:]
if args[:1] == ['-c']: args = args[2:]
cmd = args[0] if args else ''
with open(os.environ['VT_COMMAND_LOG'], 'a', encoding='utf-8') as f:
    f.write(' '.join(args) + '\\n')
if cmd in ('start', 'migrate'):
    signal.signal(signal.SIGINT, lambda *_: sys.exit(0))
    Path(os.environ['VT_STARTED']).write_text('started')
    while not Path(os.environ['VT_RELEASE']).exists(): time.sleep(0.02)
elif cmd == 'domstats':
    state = 1 if Path(os.environ['VT_LIVE_READY']).exists() else 5
    print("Domain: 'vt-test-a'\\n  state.state=" + str(state) + "\\n  vcpu.current=1\\n  vcpu.maximum=1")
    print("Domain: 'vt-test-b'\\n  state.state=5\\n  vcpu.current=1\\n  vcpu.maximum=1")
elif cmd == 'dumpxml':
    name = args[1] if len(args) > 1 else 'vt-test-a'
    print('<domain><name>' + name + '</name><vcpu>1</vcpu><memory>1048576</memory><currentMemory>1048576</currentMemory><devices/></domain>')
""", encoding="utf-8")
    virsh.chmod(0o755)
    ip = binaries / "ip"
    ip.write_text("#!/bin/sh\nprintf '[]\\n'\n", encoding="utf-8")
    ip.chmod(0o755)
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
    env = os.environ.copy()
    env.update(TERM="xterm-256color", PATH=str(binaries) + os.pathsep + env.get("PATH", ""),
               XDG_CONFIG_HOME=directory, XDG_STATE_HOME=directory, XDG_CACHE_HOME=directory,
               VT_STARTED=str(root / "started"), VT_RELEASE=str(root / "release"),
               VT_LIVE_READY=str(root / "live-ready"),
               VT_COMMAND_LOG=str(root / "commands"))
    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)
    child = subprocess.Popen([sys.argv[1], "--connect", "test:///default"], stdin=slave, stdout=slave,
                             stderr=slave, env=env, preexec_fn=controlling_terminal)
    output = bytearray()
    hold = float(os.environ.get("VT_HOLD_SECONDS", "0"))
    strict = os.environ.get("VT_STRICT_LATENCY") == "1"
    frame_deadline = 0.25 if strict else 1
    held_command = "migrate vt-test-a fake:///destination" if hold > 0 else "start vt-test-a"
    def pump(seconds):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            ready, _, _ = select.select([master], [], [], min(0.05, max(0, end - time.monotonic())))
            if ready:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    return
                output.extend(chunk)
                if b"\x1b[6n" in chunk: os.write(master, b"\x1b[1;1R")
    def wait_for(predicate, seconds, message):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if predicate(): return
            pump(min(0.01, max(0, end - time.monotonic())))
        log = (root / "commands").read_text()[-1000:] if (root / "commands").exists() else "missing"
        raise AssertionError(f"{message}; commands={log!r}; terminal={bytes(output)[-600:]!r}")
    try:
        wait_for(lambda: b"vt-test-a" in output and b"vt-test-b" in output, 8, "fake dashboard did not load")
        os.write(master, f":{held_command}\r".encode())
        wait_for(lambda: (root / "started").exists(), 4,
                 "fake job did not start; commands=" + ((root / "commands").read_text() if (root / "commands").exists() else "missing"))
        started_at = time.monotonic()
        output.clear()
        os.write(master, b"?")
        wait_for(lambda: b"Keybindings" in output, frame_deadline, "help did not render while the job was held")
        output.clear()
        os.write(master, b"\x1b")
        wait_for(lambda: b"vt-test-a" in output, frame_deadline, "help did not close while the job was held")
        log_before = (root / "commands").stat().st_size
        os.write(master, b"j")
        wait_for(lambda: "dumpxml --inactive vt-test-b" in (root / "commands").read_text()[log_before:],
                 frame_deadline, "selection did not change while the job was held")
        output.clear()
        os.write(master, b":start vt-test-b\r")
        wait_for(lambda: b"Job #1 is running" in output, frame_deadline, "second command was not refused")
        starts = [line for line in (root / "commands").read_text().splitlines() if line.startswith(("start ", "migrate "))]
        assert starts == [held_command], f"unexpected command executions: {starts!r}"
        output.clear()
        os.write(master, b"2")
        wait_for(lambda: b"Storage pools" in output, frame_deadline, "host view did not open during the job")
        output.clear()
        os.write(master, b"1")
        wait_for(lambda: b"vt-test-b" in output, frame_deadline, "dashboard did not reopen during the job")
        output.clear()
        os.write(master, b"/vt-test-b\r\x0c")
        wait_for(lambda: b"/ vt-test-b" in output, frame_deadline, "filter did not update during the job")
        output.clear()
        os.write(master, b"\x0c")
        wait_for(lambda: b"/ vt-test-b" in output, frame_deadline, "filter was not retained during the job")
        output.clear()
        os.write(master, b"/\x1b")
        wait_for(lambda: b"Job #1" in output, frame_deadline, "filter editor did not close while the job was held")
        output.clear()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 42, 160, 0, 0))
        wait_for(lambda: b"\x1b[2J" in output, frame_deadline, "resize did not redraw while the job was held")
        output.clear()
        (root / "live-ready").write_text("ready")
        wait_for(lambda: b"running" in output, 5, "live sample was not rendered while the job was held")
        output.clear()
        spinner = [glyph.encode("utf-8") for glyph in "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"]
        # Refusal messages intentionally replace progress for two seconds.
        wait_for(lambda: any(glyph in output for glyph in spinner), 3,
                 "job spinner did not return after the refusal message")
        previous = {glyph for glyph in spinner if glyph in output}
        output.clear()
        wait_for(lambda: any(glyph in output and glyph not in previous for glyph in spinner), 0.7 if strict else 4,
                 "job spinner did not advance while the command was held")
        if hold > 0:
            pump(max(0, hold - (time.monotonic() - started_at)))
        assert not (root / "release").exists(), "job released before responsiveness checks"
        output.clear()
        os.write(master, b"q")
        wait_for(lambda: b"before quitting" in output, frame_deadline, "quit was not refused during the job")
        output.clear()
        os.write(master, b"\x03")
        wait_for(lambda: b"cancelling" in output or b"stopped locally" in output,
                 frame_deadline, "cancellation was not acknowledged promptly")
        wait_for(lambda: b"stopped locally" in output, 5, "cancelled job did not report completion")
        assert not (root / "release").exists(), "job was released rather than cancelled"
        os.write(master, b"q")
        wait_for(lambda: child.poll() is not None, frame_deadline, "application did not quit after cancellation")
        assert child.returncode == 0, f"unexpected exit status {child.returncode}"
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
