# Installation

virsh-tui runs on Linux. It needs the libvirt client tools at run time and a
Rust toolchain to build.

## Requirements

| Tool | Package (Debian/Ubuntu, Fedora, Arch) | Used for |
|------|---------------------------------------|----------|
| `virsh` | `libvirt-clients`, `libvirt-client`, `libvirt` | everything (required) |
| `virt-install` | `virtinst`, `virt-install`, `virt-install` | the new domain wizard |
| `virt-viewer` | `virt-viewer` | the graphical viewer (`v`) |
| `ip` | `iproute2` | host interfaces in the Networks view |

A terminal with truecolor support gives the best result. On terminals without
it, virsh-tui falls back to the xterm 256-colour palette. The layout needs at
least 100 columns by 30 rows; 160 columns or more gives the two-column layout
shown in the screenshots.

A font with box-drawing and braille characters is needed for the borders and
graphs. Most modern monospace fonts have them (Fira Code, JetBrains Mono,
DejaVu Sans Mono, Iosevka).

## Build from source

You need Rust 1.88 or newer. Install it with [rustup](https://rustup.rs) if your
distribution ships an older version.

```sh
cargo install --locked --git https://github.com/SyscallBrain/virsh-tui
```

This puts the `virsh-tui` binary in `~/.cargo/bin`. To build from a clone
instead:

```sh
git clone https://github.com/SyscallBrain/virsh-tui
cd virsh-tui
cargo build --release
./target/release/virsh-tui --demo
```

## Permissions

virsh-tui runs `virsh` as your user, so it sees exactly what `virsh` sees.

- For `qemu:///system`, your user must be allowed to manage the system
  daemon. On most distributions that means being in the `libvirt` group:
  `sudo usermod -aG libvirt "$USER"`, then log out and back in.
- `qemu:///session` needs no extra rights; it manages the per-user daemon.
- For remote hosts (`qemu+ssh://user@host/system`), set up key-based SSH
  login first. virsh-tui cannot answer password prompts.

Check that `virsh -c qemu:///system list --all` works in a shell before
starting virsh-tui. If it does not, see [Troubleshooting](troubleshooting.md).
