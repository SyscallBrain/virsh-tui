#!/usr/bin/env bash
# Runs only inside the disposable image; no host sockets or disks are mounted.
set -euo pipefail

export XDG_RUNTIME_DIR
XDG_RUNTIME_DIR=$(mktemp -d)
trap 'rm -rf "$XDG_RUNTIME_DIR"' EXIT
libvirtd --daemon --pid-file "$XDG_RUNTIME_DIR/libvirtd.pid"

ready=false
for _ in {1..30}; do
    if virsh -c qemu:///session list --all >/dev/null 2>&1; then
        ready=true
        break
    fi
    sleep 1
done
if ! "$ready"; then
    echo 'The disposable libvirt session did not start' >&2
    exit 1
fi

virsh -c qemu:///session version
qemu-system-x86_64 --version
cargo test --locked --features it --test it_lifecycle -- --test-threads=1

# A leaked domain makes this gate red, even if both Rust tests passed.
remaining=$(virsh -c qemu:///session list --all --name)
if [[ -n "${remaining//[[:space:]]/}" ]]; then
    echo "Integration tests leaked domains: $remaining" >&2
    exit 1
fi
