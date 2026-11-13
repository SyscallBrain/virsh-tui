#!/usr/bin/env python3
"""Prove that the per-network `net-dumpxml` reads overlap.

A fake `virsh` answers `net-list` with 12 networks and counts how many
`net-dumpxml` processes are in flight at once. Startup loads the Host and
Networks views together and both read every network's XML, so the serial
implementation peaks at 2 (one read per loader). Each load has its own
READ_CONCURRENCY semaphore, so the parallel implementation must exceed 2 and
stay within 2 * READ_CONCURRENCY.
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
LIMIT = 6  # READ_CONCURRENCY, per view load
LOADERS = 2  # Host and Networks both read every network's XML at startup
SERIAL_PEAK = LOADERS  # one read in flight per serial loader

FAKE = """#!/usr/bin/env python3
import fcntl, os, sys, time
from pathlib import Path

args = sys.argv[1:]
sub = args[2] if len(args) > 2 else ""
if sub == "net-dumpxml":
    # Count live processes, not increments: an aborted load kills its children
    # with SIGKILL, so a decrement-on-exit counter would leak and inflate the peak.
    inflight = Path('{inflight}')
    lock = Path('{lock}')
    peak = Path('{peak}')
    me = inflight / str(os.getpid())
    with lock.open("a") as lf:
        fcntl.flock(lf, fcntl.LOCK_EX)
        me.touch()
        n = 0
        for entry in inflight.iterdir():
            try:
                os.kill(int(entry.name), 0)
                n += 1
            except ProcessLookupError:
                entry.unlink()
        if n > int(peak.read_text() or "0"):
            peak.write_text(str(n))
        fcntl.flock(lf, fcntl.LOCK_UN)
    time.sleep({sleep})
    me.unlink()
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
        inflight = root / "inflight"
        peak = root / "peak"
        lock = root / "lock"
        inflight.mkdir()
        peak.write_text("0")
        lock.touch()
        fake = binary_dir / "virsh"
        fake.write_text(
            FAKE.format(inflight=inflight, lock=lock, peak=peak, sleep=SLEEP, nets=NETWORKS),
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
        # the direct measurement: it can only exceed SERIAL_PEAK if `net-list`
        # returned rows and the per-network reads within one load overlapped.
        highest = int(peak.read_text() or "0")
        assert highest > SERIAL_PEAK, f"net-dumpxml reads stayed serial (peak {highest})"
        assert highest <= LOADERS * LIMIT, f"net-dumpxml concurrency {highest} exceeded {LOADERS} x {LIMIT}"
        print(f"net-dumpxml peak concurrency: {highest} (serial was {SERIAL_PEAK}, bound {LOADERS * LIMIT})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
