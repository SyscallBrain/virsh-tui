# Contributing

Bug reports, ideas and pull requests are welcome on
[GitHub](https://github.com/SyscallBrain/virsh-tui). Questions go to
[Discussions](https://github.com/SyscallBrain/virsh-tui/discussions), and
security problems to a
[private advisory](https://github.com/SyscallBrain/virsh-tui/security/advisories/new)
(see `SECURITY.md`). For anything larger than a small fix, open an issue
first so we can agree on the approach.

## Building and testing

```sh
git clone https://github.com/SyscallBrain/virsh-tui
cd virsh-tui
cargo run -- --demo
```

Before sending a pull request, run the same checks as CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The default tests need neither libvirt nor root. They parse recorded `virsh`
output from `tests/fixtures/` and compare rendered screens with
[insta](https://insta.rs) snapshots in `tests/snapshots/`. When you change
the UI on purpose, review and accept the new snapshots with
`cargo insta review`.

The required Linux CI check also runs a real backend/lifecycle target in a
disposable Docker container. To reproduce that check:

```sh
docker build -f scripts/ci/libvirt.Dockerfile -t virsh-tui-libvirt-test .
docker run --rm --network none virsh-tui-libvirt-test
```

The image builds with Rust 1.99.1 on Debian 12, then runs an unprivileged
`qemu:///session` daemon and a QEMU software-emulated (TCG) guest. It uses no
KVM, host sockets, host disks or network. The tests exercise the app's real
backend, builders and executor through lifecycle and snapshot operations,
dry-run, failed-plan short-circuiting and cleanup after an assertion fails.
Each domain has a unique `vt-test-*` name and UUID; cleanup verifies ownership.
Missing tools, a failed daemon or leaked domains fail the check. The container
prints the libvirt and QEMU versions used, and release validation runs it too.

Other real-daemon targets are behind the `it` feature. Some still use fixed
names or require KVM, so run them only in a disposable libvirt environment.
The automated image runs only `it_lifecycle`, not the entire `it` suite.
Never touch objects you did not create, even if their name starts with
`vt-test-`, and never use `sudo` in tests.

## Project layout

| Path | Contents |
|------|----------|
| `src/backend/` | `virsh` execution, output parsers, the event stream, the demo backend |
| `src/command/` | command plans, builders for each action, execution, temporary XML files |
| `src/app/` | the event loop, key dispatch per view, background loading |
| `src/ui/` | views, overlays (help, palette, wizard, confirmations) and widgets |
| `src/input/` | key engine, key map, the `:` parser, Tab completion |
| `src/theme/` | built-in themes, custom theme loading, the 256-colour fallback |
| `book/` | this documentation (mdBook) |
| `docs/` | the UI design spec (`DESIGN.md`) and the release process (`RELEASE.md`) |

## Screenshots

The images in the README and this book are rendered from demo mode
(`virsh-tui --demo`), so they never show real hosts.

## Documentation

The book lives in `book/src/`. Preview it with:

```sh
mdbook serve book --open
```

## Code style

- Rust 2024 edition; the minimum supported Rust version is 1.88.
- `rustfmt.toml` sets a line width of 110.
- No `unwrap()` on data that comes from `virsh` or from the user: parse
  errors become messages, not panics.
- Every mutation goes through a `CommandPlan`, so it can be shown, confirmed,
  dry-run and copied.

## Performance benchmarks

Run `cargo bench --bench performance` for release-mode CPU measurements of charts,
fuzzy search, palette navigation, metrics ingestion, full frames and large XML
pages. The workloads use in-memory data and ratatui's test backend, without a
libvirt connection. Compare runs on the same machine with other builds stopped;
the output includes median, minimum and maximum batch times. Terminal transport
and libvirt subprocess latency are outside these measurements.
