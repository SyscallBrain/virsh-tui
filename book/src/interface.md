# How the interface works

## Screen layout

```text
 ◆ virsh-tui  1 Domains  2 Host  3 Networks  4 Storage  5 Events     ⌁ qemu:///system │ host │ ⟳ 1s │ 14:32
 ╭ panels … ╮
 │          │
 ╰──────────╯
 NORMAL  domains › arch-dev   j/k move  ⏎ open  s start  …                       ● 9  ‖ 1  ✗ 1  ○ 4
 ✓ Started k8s-worker-02  ── virsh start k8s-worker-02
```

- The top bar lists the five views with their number keys, then the
  connection URI, the host name, the refresh interval and the time. When the
  connection is lost, the URI is replaced by a red `disconnected` and
  virsh-tui keeps retrying with backoff.
- The status bar shows the current mode, where you are (a breadcrumb), the
  most useful keys for that screen, and domain counts by state.
- The message line shows the result of the last action. When the action ran a
  command, the command follows the `──` separator. `:messages` shows the full
  history of messages.

At 153 columns or more the dashboard shows the domain list and the metrics
side by side. Between 100 and 152 it stacks the panels and the metrics move
to tabs (Overview, CPU, Mem, Disk, Net) under the list. Below 100 × 30 it
shows a "terminal too small" notice instead of a broken layout.

## Modes

| Mode | How you get there | What keys do |
|------|-------------------|--------------|
| `NORMAL` | default, `Esc` | keys are commands |
| `INSERT` | editing a field in the hardware editor or the wizard | keys type text |
| `FILTER` | `/` | keys type a live filter; `Enter` keeps it, `Esc` clears it |
| `COMMAND` | `:` | keys type a command; `Tab` completes, `Enter` runs |
| `VISUAL` | `V` | `j`/`k` extend a selection of domains |
| `CONFIRM` | a destructive action | `y` confirms, `n` or `Esc` cancels |

## Counts and repeat

A number before a motion repeats it: `5j` moves five rows down. The single
digits `1` to `5` on their own switch views, after a short pause to see if
another digit follows. `.` repeats the last action that changed something.

## The leader key

`Space` is the leader key. Less common actions live behind it so that single
letters stay free for the frequent ones. After you press `Space`, a popup in
the bottom-right corner lists what can follow:

- `Space n` new domain, `Space c` clone, `Space r` rename, `Space M` migrate
- `Space s c` / `Space s r` / `Space s d` snapshot create, revert, delete
- `Space m i` / `Space m e` insert or eject an ISO
- `Space d a` / `Space d r` attach or resize a disk
- `Space i a` / `Space i l` attach a NIC or toggle its link
- `Space x` export the XML, `Space b` edit the boot order

The same popup appears for other prefixes such as `g` and `y`.

## Every action shows its command

virsh-tui builds each mutation as a plan of one or more `virsh` (or
`virt-install`) invocations and runs exactly that argument list, without a
shell. Before a destructive action, the confirmation dialog shows the
command. After any action, the message line shows it, and `yc` copies the last
one to the clipboard.

Some actions prefill the `:` line with the full `virsh` command instead of
running it, so you can adjust flags first. Disk and NIC attachment
(`Space d a`, `Space i a`) and migration (`Space M`) work this way.

## Confirmations

`D` (destroy), `R` (reset), `X` (undefine), snapshot revert, volume delete and
volume wipe ask for confirmation. Undefine also asks you to type the domain
name. Each confirmation can be turned off in the settings (`,` then
Confirmations) or in the `[confirmations]` table of the
[config file](configuration.md).

## Marks and bulk actions

`m` marks the selected domain and `V` starts a visual selection. With marks in
place, lifecycle keys such as `s`, `S` or `p` apply to every marked domain,
and the confirmation lists them all. `Esc` leaves visual mode.

## Dry run

Start with `--dry-run` to see what each action would run without executing
it. Messages are prefixed with `[dry-run]`. Queries still run, so the screens
show real data. `--demo` implies dry run and uses example data.
