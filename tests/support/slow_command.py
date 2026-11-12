#!/usr/bin/env python3
"""Controllable child process for responsive execution tests."""
import argparse
import os
import signal
import sys
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--started', required=True)
parser.add_argument('--release', required=True)
parser.add_argument('--exit-code', type=int, default=0)
parser.add_argument('--flood', type=int, default=0)
parser.add_argument('--ignore-sigint', action='store_true')
args = parser.parse_args()
if args.ignore_sigint:
    signal.signal(signal.SIGINT, signal.SIG_IGN)
started = Path(args.started)
temporary_marker = started.with_suffix('.tmp')
temporary_marker.write_text(str(os.getpid()), encoding='utf-8')
os.replace(temporary_marker, started)
for stream in (sys.stdout.buffer, sys.stderr.buffer):
    stream.write(b'x' * args.flood)
    stream.flush()
while not Path(args.release).exists():
    time.sleep(0.01)
sys.exit(args.exit_code)
