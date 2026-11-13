# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-11-13

### Added

- Linux packages: each release has static (musl) binaries for x86_64 and
  aarch64, as `.tar.gz` archives and as `.deb` (Debian, Ubuntu), `.rpm`
  (Fedora, RHEL, openSUSE) and Arch Linux packages, with `SHA256SUMS`. The
  AUR has `virsh-tui` (built from source) and `virsh-tui-bin`.

- New domain wizard: `Enter` on the ISO path opens an ISO picker with the
  ISOs from the storage pools and your download folder (or the folders in
  `[wizard] iso_dirs`), a filter, a per-source view and a filesystem
  browser. On `qemu:///system` it warns when the emulator probably cannot
  read the chosen ISO.

- Compact layouts for the hardware editor, wizard, Settings and command
  palette below their full widths: `F6` reviews the hardware diff, `Ctrl-v`
  the full wizard command, `Ctrl-l` switches the palette's results and
  reference. Confirmation previews keep every command and scroll.

### Changed

- The Host, Networks and Storage views read their networks and pools in
  parallel, so they load faster on hosts with many of them.
- Long-running commands use background jobs so the interface can continue to
  respond. Jobs report progress and support local cancellation; a remote
  operation may continue after its client stops.
- The dashboard shows the metrics next to the domain list from 153 columns
  (it needed 162). The list gives up its spare columns first, so common
  159-column terminals get the same layout as the screenshots.
- The actions in the domain overview are plain key hints (orange key,
  action text) in an aligned grid instead of boxed chips; the CPU graph
  adapts its height to the space left.

### Fixed

- Wizard domains that are only defined (or edited with `C-e`) are a single
  persistent phase that boots the disk first, instead of booting the install
  media forever (#5). URL installs must start after creation.
- `Space m i` from the hardware editor reaches the ISO picker instead of
  entering INSERT mode (#6).
- Every cell is painted with the theme background, so terminals with a
  different default background no longer show it in margins and gutters.
- Palette search matches accented and upper-case queries.
- The which-key popup appears after a lone `Space` (or `g`), not only after a
  second key such as `Space s`, and it no longer lets the view behind it
  show through.
- The new-domain wizard no longer fails on hosts without KVM: it uses the
  `host-model` CPU when `/dev/kvm` is missing, because libvirt rejects
  `host-passthrough` there.
- A failed command reports its `ERROR` line, not the first `WARNING` that
  virt-install printed before it.
- A single `Z` no longer runs managed save immediately: `ZZ` quits, a lone
  `Z` asks first.
- The cursor stays on the same domain when CPU or uptime re-sort the list;
  uptime sorts by time.
- Actions no longer target a domain left in the detail view; leaving it with
  unsaved hardware edits is refused.
- Sizes need a unit and reach virsh with one (a bare `8192` used to mean
  8 MiB); invalid or empty fields, shrinking a disk and values past the
  running maximum are caught before anything runs.
- `:attach-disk` sets the image format, so qcow2 images are not attached as
  raw; `--live` is only used on running domains.
- Volumes are no longer labelled orphans from incomplete data; inactive pools
  and names with spaces are listed; pool destroy always confirms.
- Bulk actions run every marked domain and report failures by name; hidden
  marks are not acted on; pause/resume and autostart are decided per domain.
- Undefine works on UEFI domains and domains with snapshots or a managed save.
- Upper-case letters can be typed in text fields; overlays no longer pass
  keys to the view behind them.
- `Ctrl-z` in the editor or console no longer freezes the terminal; a closed
  terminal stops the `virsh` children; the wizard no longer reopens its
  editor in a loop; editor and console failures are reported.
- Snapshots show dates, kinds, details and descriptions; storage prefills use
  virsh's argument order; network gateways stay inside their subnet; host
  CPU % no longer counts guest time twice; `:set` applies at once.

### Changed

- Terminals from 40×12 to 100×30 get a single-column layout instead of a
  "terminal too small" notice.
- `:messages` opens the full history; the `:` line has history and editing
  keys; pasted text is never executed as keys.
- `?` has a second page with the keys of each view; the which-key popup
  follows `keymap.toml`, and invalid key names there are reported.
- Remote connections show host CPU and memory facts and can open the wizard.
- Fewer `virsh` processes: the autostart list is cached, volume and interface
  lists are read once per view load, and bursts of selection changes are
  merged.

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

[Unreleased]: https://github.com/SyscallBrain/virsh-tui/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/SyscallBrain/virsh-tui/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/SyscallBrain/virsh-tui/releases/tag/v0.1.0
