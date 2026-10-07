# Troubleshooting

Most problems come from `virsh` itself. Run the same thing in a shell first:
`virsh -c <uri> list --all`. If that fails, virsh-tui fails the same way.

## "virsh list failed" or an empty list

- `virsh not found in PATH`: install the libvirt client package
  (see [Installation](installation.md#requirements)).
- `… (not in the libvirt group?)`: your user may not manage
  `qemu:///system`. Add it to the `libvirt` group and log in again, or use
  `-c qemu:///session`.
- `… (is libvirtd running?)`: start the daemon, for example
  `sudo systemctl enable --now libvirtd` (or the modular `virtqemud`).
- The list is empty but `virsh list --all` shows domains: check the URI in the
  top bar. Without `-c`, virsh-tui uses `general.default_uri`, which may
  differ from your `LIBVIRT_DEFAULT_URI`.

## The top bar says "disconnected"

The connection dropped (daemon restart, SSH timeout). virsh-tui retries with
increasing delays and the bar recovers on its own. `Ctrl-r` forces an
immediate reload of the domain list.

## Shutdown does nothing

`S` sends an ACPI request; the guest decides what to do with it. Guests
without ACPI support, guests still booting, and some installers ignore it.
virsh-tui reports when the domain is still running after the grace period.
Use `D` to force it off.

## No IP address

Addresses come from the network's DHCP leases first and from the guest agent
second. Domains on a bridge outside libvirt's DHCP, or with a static address,
only show an IP when `qemu-guest-agent` runs in the guest.

## Memory graphs show the balloon size only

The guest's own view of used memory needs a balloon stats period. See
`monitoring.balloon_on` in [Configuration](configuration.md).

## The editor or console leaves the screen garbled

virsh-tui redraws the whole screen when an external program returns. If
something still looks wrong, `Ctrl-l` redraws.

## The wizard fails to create a domain

The error from `virt-install` is shown under the form. Common causes:

- `--osinfo`: the OS name is not in the osinfo database. Leave the field
  empty to let `virt-install` detect it, or pick a name from
  `virt-install --osinfo list`.
- Permission denied on the pool directory: the user running libvirt cannot
  write to the pool. Check the pool's path and permissions in the Storage
  view.
- UEFI firmware not found: install OVMF (`ovmf`, `edk2-ovmf`) or choose BIOS
  in the last step.

## Strange characters instead of lines and graphs

Your font lacks box-drawing or braille characters. Use a font that has them,
or set `graphs = "block"` (or `"tty"`) and `icons = "ascii"` in
`[appearance]`.

## Colours look wrong

Set `COLORTERM=truecolor` if your terminal supports 24-bit colour but does not
advertise it. Inside tmux, also enable `set -ga terminal-overrides ",*:Tc"`.

## Reporting a bug

Include the version (`virsh-tui -V`), `virsh --version`, the URI type
(system, session, remote), what you pressed, and the log from
`~/.local/state/virsh-tui/virsh-tui.log`. Start with
`RUST_LOG=virsh_tui=debug` for a detailed log.
