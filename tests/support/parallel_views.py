#!/usr/bin/env python3
"""Prove that the per-network `net-dumpxml` reads overlap.

A fake `virsh` answers `net-list` with 12 networks and counts how many
`net-dumpxml` processes are in flight at once. The Networks/Host view load must
run them concurrently (peak > 1) while staying inside READ_CONCURRENCY
(peak <= 6); the serial implementation peaked at 1.
"""
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

NETWORKS = 12
SLEEP = 0.05
LIMIT = 6

FAKE = """#!/usr/bin/env python3
import fcntl, sys, time
from pathlib import Path

args = sys.argv[1:]
sub = args[2] if len(args) > 2 else ""
if sub == "net-dumpxml":
    counter = Path('{counter}')
    lock = Path('{lock}')
    peak = Path('{peak}')
    with lock.open("a") as lf:
        fcntl.flock(lf, fcntl.LOCK_EX)
        n = int(counter.read_text() or "0") + 1
        counter.write_text(str(n))
        if n > int(peak.read_text() or "0"):
            peak.write_text(str(n))
        fcntl.flock(lf, fcntl.LOCK_UN)
    time.sleep({sleep})
    with lock.open("a") as lf:
        fcntl.flock(lf, fcntl.LOCK_EX)
        counter.write_text(str(int(counter.read_text() or "0") - 1))
        fcntl.flock(lf, fcntl.LOCK_UN)
    name = args[-1]
    print("<network><name>" + name + "</name><forward mode='nat'/><bridge name='virbr0'/></network>")
    sys.exit(0)
time.sleep({sleep})
if sub == "net-list":
    rows = " Name      State    Autostart   Persistent\\n" + "-" * 44 + "\\n"
    for i in range({nets}):
        rows += " vt-net-%02d  active   yes         yes\\n" % i
    print(rows)
elif sub == "list":
    print(" Id Name State\\n" + "-" * 20 + "\\n")
elif sub == "pool-list":
    print(" Name State Autostart\\n" + "-" * 30 + "\\n")
else:
    print("")
"""


def main() -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        binary_dir = root / "bin"
        binary_dir.mkdir()
        counter = root / "counter"
        peak = root / "peak"
        lock = root / "lock"
        counter.write_text("0")
        peak.write_text("0")
        lock.touch()
        fake = binary_dir / "virsh"
        fake.write_text(
            FAKE.format(counter=counter, lock=lock, peak=peak, sleep=SLEEP, nets=NETWORKS),
            encoding="utf-8",
        )
        fake.chmod(0o755)
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 43, 174, 0, 0))
        env = os.environ.copy()
        env.update(
            TERM="xterm-256color",
            PATH=str(binary_dir) + os.pathsep + env.get("PATH", ""),
            XDG_CONFIG_HOME=directory,
            XDG_STATE_HOME=directory,
            XDG_CACHE_HOME=directory,
        )
        child = subprocess.Popen(
            [sys.argv[1], "--connect", "test:///default", "--screen", "networks"],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
        )
        output = b""
        deadline = time.monotonic() + 3
        try:
            while time.monotonic() < deadline:
                ready, _, _ = select.select([master], [], [], 0.05)
                if ready:
                    chunk = os.read(master, 65536)
                    if not chunk:
                        break
                    output += chunk
                    if b"\x1b[6n" in chunk:
                        os.write(master, b"\x1b[1;1R")
        finally:
            child.send_signal(signal.SIGTERM)
            try:
                child.wait(timeout=6)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
            os.close(master)
            os.close(slave)
        # The ratatui backend writes changed cells individually, so the network
        # names are not contiguous in the ANSI stream. The concurrency peak is
        # the direct measurement: it can only exceed 1 if `net-list` returned
        # rows and the per-network reads overlapped.
        highest = int(peak.read_text() or "0")
        assert highest > 1, f"net-dumpxml reads stayed serial (peak {highest})"
        assert highest <= LIMIT, f"net-dumpxml concurrency {highest} exceeded {LIMIT}"
        print(f"net-dumpxml peak concurrency: {highest} (serial was 1, limit {LIMIT})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
