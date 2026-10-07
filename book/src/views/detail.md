# Domain detail

`Enter` on a domain opens its detail view. `H` and `L` (or `[` and `]`) move
between the tabs; `Backspace` or `q` returns to the list.

## Overview

The domain's configuration at a glance: machine type, firmware, vCPUs,
memory, disks, network interfaces, graphics, autostart, persistence and UUID.

## Monitor

![Monitor tab](../images/monitor.png)

Live graphs for the running domain: total CPU and one meter per vCPU, memory
with balloon and RSS, read and write rates per disk, and receive and transmit
rates per interface. `t` cycles the history window between 60 seconds,
5 minutes and 1 hour. Samples are kept for an hour while virsh-tui runs, so
switching windows does not lose data.

The numbers come from `virsh domstats`, one call per refresh for all domains.
Balloon statistics need a guest driver and a stats period. virsh-tui leaves
the period alone unless you enable `monitoring.balloon_on`, because setting it
changes the running guest.

## Hardware

![Hardware editor](../images/hardware.png)

The hardware editor lists the devices on the left (CPUs, memory, boot
options, disks, CD-ROMs, NICs, display, video, sound, input, TPM, USB
redirection) and the selected device's form in the middle. The right column
shows the XML diff of your pending changes and, below it, the commands that
`:w` will run.

In NORMAL mode:

| Key | Action |
|-----|--------|
| `j` / `k` | move between devices |
| `Tab` / `Shift-Tab` | next / previous field of the form |
| `i` | edit the focused field (INSERT mode) |
| `a` | add a device of the selected kind: prefills `:attach-disk`, `:attach-nic` or `:attach-hostdev` |
| `d` | detach the selected device, confirms |
| `J` / `K` | move the selected device down / up in the boot order |
| `u` | undo the changes to the focused field |
| `:w` | apply the pending changes |
| `:q!` | discard them |
| `gx` | jump to the XML tab |

In INSERT mode, typing edits the field and `Backspace` deletes. On number and
size fields, `Ctrl-a` / `Ctrl-x` or `l` / `h` add or subtract 1, and `L` /
`H` add or subtract 10. `Space` toggles check boxes and radio options.
`Enter` or `Tab` moves to the next field, `Esc` goes back to NORMAL mode.

Changes are applied to the persistent configuration (`--config`), to the
running domain (`--live`), or both, depending on the "Apply to" boxes at the
bottom of each form. Fields that only take effect after a reboot are marked
with `⚠ needs reboot`. The editor writes a new XML and runs
`virsh define --validate`, so libvirt rejects invalid combinations before
anything changes.

## Snapshots

![Snapshots tab](../images/snapshots.png)

The snapshot tree, with the current snapshot marked.

| Key | Action |
|-----|--------|
| `j` / `k` | select a snapshot |
| `Space s c` | create a snapshot (name, description, flags) |
| `Space s r` | revert to the selected snapshot, confirms |
| `Space s d` | delete the selected snapshot, confirms |
| `e` | edit the snapshot XML (`virsh snapshot-edit`) |

## Console

Shortcuts for the serial console (`c`, which runs `virsh console`) and the
graphical viewer (`v`). Inside the console, `Ctrl-]` returns to virsh-tui.
The escape key can be changed with `console.escape`.

## XML

The live XML with syntax highlighting. `j`/`k`, `Ctrl-d`/`Ctrl-u` and
`gg`/`G` scroll. `e` opens it in your editor; when you save and quit,
virsh-tui runs `virsh define --validate` and reopens the editor with the
error if libvirt rejects the file, as `virsh edit` does. Closing the editor
without changes defines nothing.
