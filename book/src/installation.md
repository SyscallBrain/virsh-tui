# Installation

virsh-tui runs on Linux (x86_64 and aarch64). It needs the libvirt client
tools at run time. Install a package for your distribution, or build it from
source with a Rust toolchain.

## Requirements

| Tool | Package (Debian/Ubuntu, Fedora, Arch) | Used for |
|------|---------------------------------------|----------|
| `virsh` | `libvirt-clients`, `libvirt-client`, `libvirt` | everything (required) |
| `virt-install` | `virtinst`, `virt-install`, `virt-install` | the new domain wizard |
| `virt-viewer` | `virt-viewer` | the graphical viewer (`v`) |
| `ip` | `iproute2` | host interfaces in the Networks view |

A terminal with truecolor support gives the best result. On terminals without
it, virsh-tui falls back to the xterm 256-colour palette. The minimum terminal
size is 40 columns by 12 rows, with a reduced single-column layout. At least
100×30 is recommended; at least 153 columns and 30 rows are needed for the
side-by-side layout shown in the screenshots.

A font with box-drawing and braille characters is needed for the borders and
graphs. Most modern monospace fonts have them (Fira Code, JetBrains Mono,
DejaVu Sans Mono, Iosevka).

## Packages

Each release has packages and archives on the
[releases page](https://github.com/SyscallBrain/virsh-tui/releases/latest),
with `SHA256SUMS` listing the checksum of every file. x86_64 files are named
`amd64` or `x86_64`, aarch64 files `arm64` or `aarch64`. The binary inside is
static (musl), so it does not depend on your distribution's glibc. Releases
after 0.1.0 have these packages; 0.1.0 has only an x86_64 archive.

Releases after 0.3.1 also carry signed build provenance, which proves a file
was built by this repository's release workflow. With the
[GitHub CLI](https://cli.github.com) you can check it:

```sh
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify virsh-tui_*_amd64.deb --repo SyscallBrain/virsh-tui
```

| System | File | Install |
|--------|------|---------|
| Debian, Ubuntu | `virsh-tui_VERSION-1_amd64.deb` | `sudo apt install ./virsh-tui_*_amd64.deb` |
| Fedora, RHEL | `virsh-tui-VERSION-1.x86_64.rpm` | `sudo dnf install ./virsh-tui-*.x86_64.rpm` |
| openSUSE | `virsh-tui-VERSION-1.x86_64.rpm` | `sudo zypper install --allow-unsigned-rpm ./virsh-tui-*.x86_64.rpm` |
| Arch Linux | `virsh-tui-VERSION-1-x86_64.pkg.tar.zst` | `sudo pacman -U ./virsh-tui-*-x86_64.pkg.tar.zst` |
| Any Linux | `virsh-tui-VERSION-x86_64-unknown-linux-musl.tar.gz` | unpack it and put `virsh-tui` on your `PATH` |

The packages install `virsh-tui` in `/usr/bin` and pull in the libvirt client
(`libvirt-clients`, `libvirt-client` or `libvirt`). The `.deb` and `.rpm`
recommend `virt-install` and `virt-viewer`; on Arch they are optional
dependencies.

### Arch User Repository

Two AUR packages follow the releases:

- `virsh-tui-bin` installs the static binary of the release.
- `virsh-tui` builds the release from source with your Rust toolchain.

```sh
paru -S virsh-tui-bin     # or: yay -S virsh-tui-bin
```

## Build from source

You need Rust 1.88 or newer. Install it with [rustup](https://rustup.rs) if your
distribution ships an older version.

```sh
cargo install --locked virsh-tui
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
