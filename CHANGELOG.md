# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- The dashboard shows the metrics next to the domain list from 153 columns
  (it needed 162). The list gives up its spare columns first, so common
  159-column terminals get the same layout as the screenshots.
- The actions in the domain overview are plain key hints (orange key,
  action text) in an aligned grid instead of boxed chips; the CPU graph
  adapts its height to the space left.

## [0.1.0] - 2026-10-07

First public release.

- Domains dashboard with live CPU, memory, disk and network graphs, filters,
  sorting, marks and bulk lifecycle actions
- Domain detail: overview, monitor with 60 s / 5 min / 1 h history, hardware
  editor with XML diff, snapshots, console and XML
- Host, Networks, Storage and Events views
- New domain wizard that builds and runs `virt-install`
- `:` command line with Tab completion and `virsh` passthrough, command
  palette, which-key popup, remappable keys
- Nine built-in themes, custom themes, settings screen, 256-colour fallback
- `--demo`, `--dry-run`, `--probe` and `--dump`

Download `virsh-tui-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` from the release
page, or build from source with
`cargo install --locked --git https://github.com/SyscallBrain/virsh-tui --tag v0.1.0`.
The [user guide](https://syscallbrain.github.io/virsh-tui/) covers
requirements and setup.

[Unreleased]: https://github.com/SyscallBrain/virsh-tui/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/SyscallBrain/virsh-tui/releases/tag/v0.1.0
