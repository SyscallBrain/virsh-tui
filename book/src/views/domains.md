# Domains

![Domains dashboard](../images/dashboard.png)

The domains view is the start screen. The list on the left shows every domain
on the connection with its state, vCPUs, memory, a CPU bar, uptime and
autostart flag. The right-hand side follows the selection: an overview with
the OS, machine type, firmware, IP address, display and guest agent status,
then CPU, memory, disk and network graphs. On narrower terminals the graphs
move under the list, with tabs (Overview, CPU, Mem, Disk, Net) that `[` and
`]` switch.

Below the list are the most recent libvirt events and a summary of the host:
CPU, memory, swap, hugepages and the two largest storage pools.

## Finding domains

| Key | Action |
|-----|--------|
| `j` / `k` | move down / up |
| `gg` / `G` | first / last domain |
| `Ctrl-d` / `Ctrl-u` | half a page down / up |
| `/` | filter by name as you type; `Enter` keeps the filter, `Esc` clears it |
| `n` / `N` | next / previous match |
| `f` / `F` | cycle the state chips: all, running, paused, crashed, off |
| `o` | cycle the sort order: state, name, CPU, memory, uptime |
| `:42` | jump to row 42 |

## Lifecycle

| Key | Action | virsh command |
|-----|--------|---------------|
| `s` | start | `virsh start` |
| `S` | ACPI shutdown | `virsh shutdown` |
| `D` | destroy (force off), confirms | `virsh destroy` |
| `r` | reboot | `virsh reboot` |
| `R` | reset (hard), confirms | `virsh reset` |
| `p` | pause or resume | `virsh suspend` / `virsh resume` |
| `Z` | save state to disk and stop | `virsh managedsave` |
| `a` | toggle autostart | `virsh autostart [--disable]` |
| `X` | undefine, asks you to type the name | `virsh undefine` |

After `S`, virsh-tui watches the domain. If it is still running after the
grace period, the message line says the guest ignored the request, and you
can retry or use `D`.

## Console, viewer and XML

| Key | Action |
|-----|--------|
| `c` | serial console (`virsh console`); `Ctrl-]` returns to virsh-tui |
| `v` | graphical viewer (`virt-viewer`, or `remote-viewer` if configured) |
| `e` | open the XML in your editor, then `virsh define --validate` it |
| `Space x` | save the XML as `./<name>.xml` in the current directory (never overwrites) |

## Copying

| Key | Copies |
|-----|--------|
| `yy` | domain name |
| `yu` | UUID |
| `yi` | first IP address |
| `yc` | the last command virsh-tui ran |

## Several domains at once

Mark domains with `m`, or press `V` and extend the selection with `j`/`k`.
Lifecycle keys then act on every marked domain. The confirmation dialog lists
them before anything runs.
