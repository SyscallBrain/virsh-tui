# Contributing to virsh-tui

Thanks for helping. Bug reports, ideas and pull requests are all welcome.
For anything bigger than a small fix, please open an issue first so we can
agree on the approach before you spend time on it.

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
- If a screen changes visibly, regenerate the screenshots with
  `python3 scripts/screenshots.py` (needs a release build, headless Chrome
  and ImageMagick).

## Integration tests

Tests that talk to a real libvirt daemon are behind the `it` feature and use
`qemu:///session`:

```sh
cargo test --features it
```

They only create, change and delete objects named `vt-test-*`. Keep it that
way in new tests: never touch domains, networks or pools you did not create,
and never use `sudo`.

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
