# Safety model

virsh-tui manages virtual machines, so a wrong key can stop a server or
delete a disk. These are the rules it follows.

## Commands, not a private API

Every change is a plan of `virsh` or `virt-install` invocations. virsh-tui
runs exactly the argument list it shows you, with `execve` and no shell, so a
domain name such as `a; rm -rf ~` stays a single argument. The same plan is
what `--dry-run` prints.

## Confirmation for destructive actions

Destroy, reset, undefine, snapshot revert and delete, network and pool
destroy and undefine, and volume delete and wipe ask for confirmation. The
dialog shows the command. Undefine also asks you to type the domain name.
Bulk actions list every affected domain. The same applies to destructive
subcommands typed on the `:` line.

## Your files and guests stay as they are

- XML edits go through `virsh define --validate`, so libvirt checks them
  before anything changes. A rejected file reopens in your editor.
- Exporting XML never overwrites an existing file.
- Settings that change running guests, such as the balloon stats period, are
  off until you turn them on.

## Temporary files

XML handed to `virsh define` or `attach-device` is written to
`$XDG_RUNTIME_DIR` with a random name, created exclusively (`O_EXCL`) with
mode 0600, and removed afterwards.

## Untrusted text

Domain names, descriptions, event details and command output come from
libvirt and from guests. virsh-tui strips terminal control sequences from
them before display, and escapes values it writes into XML.

## Timeouts

Each `virsh` call has a time limit suited to the command (long for
migrations, block jobs and volume uploads, short for queries). A command that
runs over its limit is killed instead of left running in the background.

## What virsh-tui does not do

It never uses `sudo` and never asks for passwords. It runs with your user's
libvirt permissions, nothing more.
