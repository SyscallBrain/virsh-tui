# Contributing

Bug reports, ideas and pull requests are welcome on
[GitHub](https://github.com/SyscallBrain/virsh-tui). For anything larger
than a small fix, open an issue first so we can agree on the approach.

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

Integration tests that talk to a real libvirt daemon are behind the `it`
feature. They only create, change and delete objects whose name starts with
`vt-test-`, on `qemu:///session`:

```sh
cargo test --features it
```

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
| `scripts/screenshots.py` | regenerates the screenshots from demo mode |
| `scripts/presentation.py` | records the presentation GIF from demo mode |
| `docs/` | the original design notes and mockups |

## Screenshots

The images in the README and this book are rendered from demo mode, so they
never show real hosts:

```sh
cargo build --release
python3 scripts/screenshots.py
```

The script needs headless Chrome or Chromium and ImageMagick.

The presentation GIF at the top of the README (`images/demo.gif`) is a real
recording: the script runs `virsh-tui --demo` in a pseudo-terminal, types
the keys of each scene and draws the frames with headless Chrome. It uses a
temporary `HOME`, so your config and wizard draft are not touched. Edit the
`SCENES` list in the script to change what it shows.

```sh
cargo build --release
uv run scripts/presentation.py
```

It needs [uv](https://docs.astral.sh/uv/) (which installs the `pyte`
terminal emulator), headless Chrome or Chromium, ffmpeg and, optionally,
gifsicle.

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
