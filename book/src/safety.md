# Safety model

virsh-tui manages virtual machines, so a wrong key can stop a server or
delete a disk. These are the rules it follows.

## Commands, not a private API

Every change is a plan of `virsh` or `virt-install` invocations. virsh-tui
runs exactly the argument list it shows you, with `execve` and no shell, so a
domain name such as `a; rm -rf ~` stays a single argument. The same plan is
what `--dry-run` prints.

## Confirmation for destructive actions

Destroy, reset, undefine, managed save, snapshot revert and delete, network
and pool destroy and undefine, disk and NIC detach, `:block-resize`, clone,
and volume delete and wipe ask for confirmation. The dialog shows the
commands, quoted as a shell would need them. Undefine also asks you to type
the domain name. Any action on more than one marked domain asks first and
lists the command for every domain. The same applies to destructive
subcommands typed on the `:` line, which always confirm.

Settings › Confirmations can turn off the dialog for domain destroy, reset
and undefine, volume delete and wipe, and snapshot revert. Each toggle
affects only its own action: pool and network destroy, detach and raw
destructive commands always confirm.

Settings › Confirmations › Bulk (`confirmations.bulk_threshold`) brings the
dialog back for those actions when they would act on that many marked
domains or more.

## Read-only connections

`virsh-tui --read-only` (`-r`), `general.read_only = true` or a connection
profile with `read_only = true` (see
[Configuration](configuration.md#per-connection-settings)) make the session
look-only, and the top bar shows a yellow `read-only` pill. Three layers
keep it that way:

- Keys and commands that would change something are refused before any
  dialog, with the reason: `✗ read-only connection (--read-only): destroy
  not run`. Typed `virsh` subcommands run only if they are known to just
  read (`dominfo`, `domstats`, `dumpxml`, `net-dhcp-leases`, …); anything
  else, unknown commands included, is refused.
- `virsh` is started with `-r`, so libvirt itself refuses any change that
  would get through.
- `virt-install` and `virt-clone` are never started (except
  `virt-install --print-xml`).

The memory stats period (`monitoring.balloon_on`) is not set on a
read-only connection, because it changes the running guests. The serial
console needs a read-write connection; the graphical viewer works.

## Sizes and hardware edits

Sizes need a unit (`512M`, `8G`, `1.5T`): virsh reads a bare number as KiB,
so `8192` is refused instead of becoming an 8 MiB guest. virsh-tui passes
sizes to virsh with an explicit unit. `:w` refuses invalid or empty values,
a current value above the maximum, and a disk size smaller than the disk
(shrinking destroys data). Raising vCPUs or memory beyond what a running
guest booted with is saved to the configuration and reported as needing a
restart, instead of failing half-way. A domain without a CPU `<topology>`
keeps it that way when its maximum vCPUs change. NIC link changes use the
MAC address and also update the configuration.

Disks attached with `:attach-disk` get their format (`qcow2`, `raw`, …) from
the file name or the image header; when it cannot be told, the command is
refused rather than attaching a qcow2 image as raw. `--live` is only passed
to a running or paused domain.

## Your files and guests stay as they are

- Domain XML edits go through `virsh define --validate`; network edits use
  `virsh net-define --validate` when the installed virsh supports it. A
  rejected edit reopens in your editor when you are still viewing the same
  object; otherwise press `e` there to retry. Quitting the editor without
  changes abandons the draft. A kept draft is only reopened while the object
  still has the XML it was made from; if the object changed meanwhile, the
  draft is saved to a file (path in the message line) instead of being
  defined over the newer configuration.
- An invalid `config.toml` is never overwritten: virsh-tui says so and runs
  on defaults without saving. Settings are written atomically.
- Exporting XML never overwrites an existing file.
- Switching connections waits until pending hardware edits have been applied
  with `:w` or discarded with `:q!`.
- Settings that change running guests, such as the balloon stats period, are
  off until you turn them on.

## Temporary files

XML handed to `virsh define` or `attach-device` is written to
`$XDG_RUNTIME_DIR` (or the system temporary directory if unavailable) with a
random name, created exclusively (`O_EXCL`) with mode 0600, and removed
afterwards. If the editor stops after changing its file, or the edited file
cannot be read as UTF-8 within the size limit, that file is retained for
recovery; its path is shown in the message line or recorded in the log.
Remove the recovery file after retrieving its contents.

## Untrusted text

Domain names, descriptions, event details and command output come from
libvirt and from guests. virsh-tui strips terminal control sequences from
them before display, and escapes values it writes into XML.

## Timeouts

Each `virsh` call has a time limit suited to the command (long for
migrations, block jobs and volume uploads, short for queries). A command that
runs over its limit is killed instead of left running in the background.

## Terminal hand-off

`Ctrl-z` inside the editor or console is ignored (the program is resumed at
once) instead of freezing the terminal. Closing the terminal (`SIGHUP`) or
`SIGQUIT` stops virsh-tui and its `virsh` children like `SIGTERM` does. A
console or viewer that cannot start, or exits with an error, is reported in
the message line.

## What virsh-tui does not do

It never uses `sudo` and never asks for passwords. It runs with your user's
libvirt permissions, nothing more.

## Cancelling a background job

`Ctrl-c` stops the local client for an active background job. When the
running step is a migration, a save (`managedsave`, `save`), a dump or a
snapshot, virsh-tui also sends `virsh domjobabort <domain>`, which asks
libvirt to stop its own job (the job line says
`cancelling (domjobabort <domain>)`). Other operations may continue in
libvirt after the client stops, so verify the object state before retrying.
Previously completed steps are not rolled back.

While such a job runs, the job line shows libvirt's progress from
`virsh domjobinfo`, read once a second: `25% · 1.0/4.0 GiB · ~30s left`.

If a hardware job may have changed the domain but did not finish, its pending
edits remain visible and another apply is blocked. Record any values you need,
discard the draft and reopen domain details to reload the configuration before
retrying. A successful hardware job also waits for its read-back before another
apply can start.

## Doctor

The doctor's checks only read (`virsh` queries, `PATH` lookups, files). A fix
runs only after you press `f` and confirm it, through the same paths as the
rest of the app: `:set`, the confirmation dialog with the commands, the
editor, or `:connect`. Fixes that need root are shown and copied, never run:
virsh-tui does not use `sudo` or `pkexec`.
