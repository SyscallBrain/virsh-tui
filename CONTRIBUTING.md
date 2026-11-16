# Contributing to virsh-tui

Thanks for helping. Bug reports, ideas and pull requests are all welcome.
Questions go to [Discussions](https://github.com/SyscallBrain/virsh-tui/discussions).
For security problems, please follow [SECURITY.md](SECURITY.md) instead of
opening an issue. For anything bigger than a small fix, please open an issue
first so we can agree on the approach before you spend time on it.
Everyone taking part is expected to follow the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Getting started

```sh
git clone https://github.com/SyscallBrain/virsh-tui
cd virsh-tui
cargo run -- --demo        # example data, nothing is executed
```

You need Rust 1.88 or newer. A libvirt daemon is only needed to try the
real backend and for the integration tests.

## Before you open a pull request

Run the same checks as CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- UI changes update [insta](https://insta.rs) snapshots in `tests/snapshots/`.
  Review them with `cargo insta review` and commit the accepted ones.
- If you change behaviour or key bindings, update the user guide in
  `book/src/` (preview with `mdbook serve book --open`).

## Integration tests

The required `fmt, clippy, test` CI check also runs the real backend/lifecycle
target in a disposable Docker container. Reproduce it with:

```sh
docker build -f scripts/ci/libvirt.Dockerfile -t virsh-tui-libvirt-test .
docker run --rm --network none virsh-tui-libvirt-test
```

The image builds with Rust 1.99.1 on Debian 12 and runs its own unprivileged
`qemu:///session` daemon. It needs no KVM, privileged container, host mounts or
host libvirt service: the guest uses QEMU software emulation (TCG), a temporary
disk and no network. The tests exercise `VirshBackend`, command builders and
the real executor through define/start/pause/resume/snapshot/delete/undefine,
plus dry-run, failed-plan short-circuiting and cleanup after a failed assertion.
Missing tools or a failed daemon are errors, and leftover domains fail the gate.
The container prints the libvirt and QEMU versions used. Release validation
runs the same target before any release binaries are built.

Other real-daemon targets are behind the `it` feature. Run them only in a
disposable libvirt environment: some still use fixed `vt-test-*` names, and
some require KVM. The automated container deliberately runs only
`cargo test --locked --features it --test it_lifecycle -- --test-threads=1`.
New tests must use unique names and verify ownership before cleanup; never
touch domains, networks or pools you did not create, and never use `sudo`.

## Code guidelines

- Every change to libvirt goes through a `CommandPlan`, so it can be shown,
  confirmed, dry-run and copied. Run commands as argument lists, never
  through a shell.
- Text from libvirt or guests is untrusted: sanitize it before display and
  escape it before writing XML.
- Parse errors become messages, not panics.
- Code, UI text, docs and commit messages are in English.

The [contributing chapter](https://syscallbrain.github.io/virsh-tui/contributing.html)
of the user guide describes the project layout.
