# Getting started

## First run

```sh
virsh-tui                            # default URI from the config (qemu:///system)
virsh-tui -c qemu:///session         # the per-user daemon
virsh-tui -c qemu+ssh://me@nas/system
virsh-tui --demo                     # example data, nothing is executed
```

The first screen is the domains dashboard. The list is on the left; the panel
on the right shows the selected domain and its live graphs. The bar at the top
shows the views (`1` to `5`), the connection URI, the host name, the refresh
interval and the clock.

## A five-minute tour

1. Move with `j` and `k`. The right-hand panel follows the selection.
2. Press `s` to start a stopped domain. The message line at the bottom shows
   `✓ Started <name>  ── virsh start <name>`: the result and the command.
3. Press `S` to send an ACPI shutdown. virsh-tui keeps watching the domain and
   tells you if the guest ignores the request, which happens when the guest has
   no ACPI support or is still booting.
4. Press `Enter` to open the domain detail. Use `H` and `L` to move between
   Overview, Monitor, Hardware, Snapshots, Console and XML. `Backspace` or `q`
   goes back.
5. Press `/` and type part of a name to filter the list. `Esc` clears the
   filter.
6. Press `Space` and wait a moment. A popup lists everything you can do after
   the leader key, such as `Space n` for a new domain or `Space s c` for a
   snapshot.
7. Press `:` and then `Tab`. Every command and `virsh` subcommand is listed.
   Try `:theme ` followed by `Tab` to preview the themes.
8. Press `?` for the full key reference, and `,` for the settings.
9. `:q` quits (or `q` from the dashboard).

## When you are not sure what a key does

Run with `--dry-run`. virsh-tui then shows the command each action would run
and executes nothing:

```sh
virsh-tui --dry-run
```

Read-only queries (the domain list, stats, XML) still run, so the screens show
real data.
