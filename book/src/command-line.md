# The command line

`:` opens the command line at the bottom of the screen. It accepts
virsh-tui's own commands and any `virsh` subcommand.

## Completion

`Tab` completes the word under the cursor and opens a list of candidates;
`Tab` and `Shift-Tab` move through it, `Enter` runs the line. Completion
knows about:

- virsh-tui commands and every `virsh` subcommand, with a one-line summary
- domain names, with their state, after commands that take a domain
- network names after `net-…` commands, pool names after `pool-…` commands
- theme names after `:theme`, with a colour preview
- connection URIs after `:connect`, setting names after `:set`
- `--flags` of any `virsh` subcommand, read from `virsh help <command>`
- file paths for words that start with `/`, `~/` or `./`

## Editing the line

`Up` / `Down` recall earlier lines of the session. `Left` / `Right`,
`Home` / `End` (or `Ctrl-a` / `Ctrl-e`) move the cursor; `Delete` deletes
under it, `Ctrl-w` deletes the previous word and `Ctrl-u` everything before
the cursor. Pasted text (bracketed paste) is inserted as text; outside the
command line or a text field a paste is ignored, so it can never run keys.
Arguments are split like a shell splits them, so quote paths with spaces:
`:attach-iso vm "/isos/My Distro.iso"`. Commands that take one argument
reject extra words instead of ignoring them.

## virsh-tui commands

| Command | Action |
|---------|--------|
| `:start <domain>` | start a domain |
| `:shutdown <domain>` | ACPI shutdown |
| `:destroy <domain>` | force off (confirms) |
| `:rename <domain> <new-name>` | rename (the domain must be shut off) |
| `:clone <domain> <new-name>` | clone with `virt-clone` (shut off or paused source; `Space c` prefills it) |
| `:desc <domain> <text>` | set the description |
| `:attach-disk <domain> <source> <target>` | attach a disk (format from the extension or the image header; `--live` only if the domain runs) |
| `:detach-disk <domain> <target>` | detach a disk |
| `:attach-nic <domain> <source> <model>` | attach a NIC to a network |
| `:block-resize <domain> <target> <size>` | grow a disk of a running domain (size needs a unit: `40G`; always confirms) |
| `:attach-iso <domain> [target] <iso>` | insert an ISO (alias `:media`) |
| `:attach-hostdev <domain> <nodedev>` | pass a host device through |
| `:connect <uri>` | switch connection |
| `:theme <name>` | switch theme (saved to the config) |
| `:sort name\|state\|cpu\|mem\|uptime` | sort the domain list |
| `:set <key>=<value>` | change a setting, validated, applied at once and saved |
| `:42` | jump to row 42 |
| `:messages` | open the message history (job output included; `j`/`k` scroll) |
| `:help [topic]` | open the help |
| `:w` | apply hardware changes |
| `:q` | close the current screen, or quit from the dashboard |
| `:q!` | discard hardware changes |
| `:qa` | quit |

## Running virsh directly

Anything that is not a virsh-tui command is passed to `virsh` as is, against
the current connection:

```text
:dominfo arch-dev
:setmem arch-dev 8G --live
:snapshot-create-as arch-dev pre-upgrade --atomic
:!virsh net-dhcp-leases default
```

The line is split like a shell would split it (quotes work), but no shell
runs, so pipes, redirections and variables have no effect. The connection is
set with `:connect`, not with `-c`. Destructive subcommands such as
`destroy`, `undefine` or `vol-delete` ask for confirmation first. The first
line of the output appears in the message line and the rest in
`:messages`. The list also covers `vol-upload`, `managedsave-remove`,
`detach-*`, `blockresize`, `vol-resize --shrink`, `migrate`,
`checkpoint-delete`, `nwfilter-undefine`, `secret-undefine` and
`qemu-monitor-command`.

Raw commands use the same single active job as other actions, including commands
that only read data. While a job is running, another command or connection change
is refused. `Ctrl-c` requests cancellation of the local client; check the object
state before retrying because libvirt may continue the operation. See
[Cancelling a background job](safety.md#cancelling-a-background-job).

When you type a command with its own arguments, such as
`:attach-disk vm /path/disk.qcow2 vdb --driver qemu --config`, virsh-tui
passes it to `virsh` unchanged instead of using its shorter built-in form.
