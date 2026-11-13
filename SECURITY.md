# Security policy

virsh-tui runs `virsh` and `virt-install` commands that change your virtual
machines, networks and storage, so security reports are taken seriously.

## Reporting a vulnerability

Please report vulnerabilities privately through
[GitHub's security advisories](https://github.com/SyscallBrain/virsh-tui/security/advisories/new),
not in a public issue. Include the version (`virsh-tui -V`), the output of
`virsh --version`, the connection type (system, session or remote) and the
steps to reproduce.

You can expect a first answer within a week. Once a fix is released, the
advisory is published with credit to you, unless you prefer otherwise.

## Scope

virsh-tui is a front end for the libvirt command-line tools. Problems in
libvirt, `virsh`, `virt-install` or QEMU themselves belong to
[libvirt's security process](https://libvirt.org/securityprocess.html) or to
the project concerned.

How virsh-tui decides what it runs, and what it asks you to confirm, is
described in the
[safety model](https://syscallbrain.github.io/virsh-tui/safety.html) chapter of
the documentation.

## Supported versions

Only the latest release receives security fixes.
