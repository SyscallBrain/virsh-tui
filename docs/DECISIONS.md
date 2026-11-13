# Decisions and deviations

## P0
- Work in place on branch `dev/virsh-tui` instead of a git worktree: the user asked for a separate branch, not a separate directory.
- `tracing-subscriber` needs feature `env-filter` for `EnvFilter`; added explicitly.
- `directories::ProjectDirs::state_dir()` returns `Option<&Path>`; log file path built via `to_path_buf()`.

## P1
- Chart maths (`rng`, `series`, `sample`, `braille`, `sparkRows`, `hbar`, `lg`) ported 1:1 from `docs/mockups/Main.dc.html`; JS helpers extracted to `tests/fixtures/js/charts.js` and expected outputs committed under `tests/fixtures/charts/`.
- `grad` ported as pure colour-index assignment; row colouring happens in views (P3+).
- Gallery `--screen gallery` renders full-frame for now; it will become a proper chrome body panel when views land.
- `panel::render` allowed `clippy::too_many_arguments`; `charts::braille` allowed `clippy::needless_range_loop` to keep the 1:1 port readable.

## P2
- Real `virsh snapshot-list --parent` columns are Name | Creation Time (date, time, tz) | State | Parent | Children | Descendants; state is index 4; the parent at index 5 is absent for a root snapshot.
- `virsh nodeinfo` on this machine reports `CPU model: x86_64`, `CPU(s): 6`, `Memory size: N KiB`; hostname comes from the `hostname` crate (reports `rdebian` here, `forge` in demo).
- `virsh help` index lines are indented with 4 spaces; parser skips non-indented headers.
- Demo memory labels (`12.4/16G`, `—/4G`) parsed as GiB floats to KiB; em-dash means 0.
- Read-only probe verified against `qemu:///system` (3 shut-off domains); no mutations, no sudo.

## P3
- Dashboard renders demo data by default in `--demo`; live mode (no `--demo`) fetches `virsh list --all` + `domstats --raw` once at startup (read-only).
- Sort marker `▾` follows the active sort column; default `state › name`.
- Non-running selection shows the `Not running` panel with config summary + `s start` hint.
- Live domain names truncate at 17 chars to preserve the grid.

## P4
- `q`/`ZZ`/`C-c` quit at the app level (before the key engine): `Z` alone is managed-save, so `ZZ` can never reach the engine as a sequence.
- `y` alone is unbound globally so yank sequences (`yy`/`yu`/`yi`/`yc`) work; the confirm modal handles `y`/`n` itself.
- View tabs `1`-`5` stay unbound until later phases; binding digits now would break count prefixes (e.g. `15j`).
- `retarget()` is identity for now: every builder takes the domain, so bulk reuses the same plan per target. Per-target argv rebuilds land with parametrized builders (P7+).
- `yank-uuid` copies the name as a placeholder until real UUIDs flow through `DomainRow` (P6).
- Integration test needs one 32M qcow2 in tempdir: internal snapshots require all disks selected, so a diskless domain cannot snapshot. Still `vt-test-*` + tempdir, cleaned in `Drop`.
- `keymap.toml` overrides parsed as stub (`apply_overrides` identity); full TOML wiring in P13.

## P5
- View digits buffer as counts; a lone `1`-`5` resolves through the keymap on a 300 ms idle flush (`KeyEngine::flush`). Multi-digit timeouts cancel. `g`-prefixed bindings are unaffected.
- Which-key popup shows for pending prefixes (`Space`, `Space s`, `Space m`, `g`, `y`) after 300 ms.
- Remote hosts hide per-thread meters, temp, and freq (no /proc); live local only for now (`:connect` keeps the dashboard flow, host seasons in P14 remote work).
- Live temperature needs hwmon; containers without it show `—`.

## P6
- `q` goes back in detail (quits elsewhere); `H`/`L` switch inner tabs; `⏎` opens detail from dashboard/host, `e` jumps to the XML tab.
- `e` on the XML tab edits the **inactive** XML in `$EDITOR`, defines with `--validate`, and reopens with the error as an XML comment on failure (aborts when saved unchanged).
- `:rename <d> <new>` runs `domrename`; `:desc <d> <text>` uses the xmltree define path. `␣r` prefills the rename line.
- Host `⏎` opens the first running domain (host table has no cursor yet; cursor lands in P7+ polish if needed).

## P7
- Pending counts XML *sections* (`vcpu`, `cpu`, …), so max+auto-topology reads as 2 pending like the mockup.
- `xml::diff` normalizes both sides through a custom deterministic pretty printer (sorted attributes): xmltree attribute HashMaps emit in random order, which produced spurious/flaky hunks.
- The pretty printer preserves namespace declarations (skipping reserved `xmlns`/`xml` prefixes) and qualified open/close tags so output re-parses and `virsh define` accepts it.
- Digits buffer as counts globally; hardware INSERT intercepts keys before the engine, with `h`/`l` adjusting only number/size fields (text fields type them).
- `y` in the hardware tab copies will-run (mockup); yank `yy`/`yu`/`yi`/`yc` keep working elsewhere.
- Nodedev attach runs `nodedev-dumpxml` → temp `<hostdev>` → `attach-device --config`; the fuzzy picker UI lands with P12.
- Integration needs max vCPU/memory headroom in the test XML (hot-plug within maxima).

## P8
- Revert plan is multi-step: `[snapshot-create-as auto-before-revert-HHMM --atomic]` + `[snapshot-revert …]`; safety name uses the live date and time `YYYYMMDD-HHMMSS` (review RB-7: HHMM collided; frozen 1438 in demo/snapshot fixtures).
- Snap modal keys bypass the generic confirm: `1/2/3` after-state, `f` force, `s` safety, `c/o/m` delete flags, `y` run, `n/Esc` close.
- `e` on the snapshots tab suspends into `virsh snapshot-edit --snapshotname`.
- Live snapshots build the tree from parent links (dates empty until per-snapshot dumpxml).

## P9
- `n`/`e`/`l` are context overrides in the networks view (`n` is NextMatch elsewhere); `␣l`/`␣L` pin/unpin via `net-update … --live --config`.
- `⏎` on a lease jumps to the mapped domain (DESKTOP-W11G → win11-gaming in demo data).
- New-network DHCP range auto-computes (.2 – .254) from the CIDR field while typing.
- `qemu:///session` cannot start networks (bridge needs privilege): `it_net` covers define/dumpxml/autostart/undefine.

## P11
- `Space+n` opens the wizard (was Clone; `Space+c` still clones).
- Steps 1/2/4/5/6 are fully modeled in `WizardState` (summaries feed the side list + argv); only step 3 renders the full field matrix so far.
- `virt-install` runs with explicit `--connect <uri>`; display shows the same argv.
- `it_wizard` defines the wizard-model XML directly: unprivileged sessions cannot run real `virt-install` installs.

## P10
- `vol-clone` takes `--pool` once (`vol-clone --pool P vol new`); double `--pool` is rejected.
- `it_vol` removes the pool dir before define: virsh keeps volume files on undefine, so reruns would see stale `exists already`.
- Storage `n`/`e`/`h`/`l`/pool-vol keys are context overrides in the Storage view; `j/k` keep engine counts.
- Picker ISO path comes from the pool XML target + volume name (no extra vol-dumpxml round-trip).

## P12
- Palette `Enter`: snapshot-* opens the detail modal, lifecycle runs on the context domain, raw commands stage for review (history + message).
- Generic form = staged command in the preview box: `Tab` toggles the option under the reference cursor, `C-l` focuses the pane, `C-y` copies. `--name` stages a fixed placeholder.
- Help overlay is generated from keymap section data (same order/labels as the mockup); remaps flow through automatically.
- Events drain from the libvirt stream each tick; filters are pure predicates over the buffer.
- `every_help_entry_parses` shells out to live virsh (skips cleanly without it) and asserts each doc name round-trips.

## P13
- `Theme` gained `border`, `transparent`, `icons` (all `Copy`); every `panel_block` uses `theme.border`, glyphs use `theme.icons`. Custom names are `Box::leak`ed to keep `Theme: Copy`.
- `transparent` flows through the shared style helpers (`text`/`dim`/panels); ad-hoc `tokens.bg` spans keep their colour (follow-up).
- `:set key=value` writes config + disk and rebuilds the theme; unknown keys error.
- Style maps encode fg+bg token letters per cell; empty cells are `??` (terminal default), content cells must resolve.

## Hotfix: signal hook reactor panic
- `install_signal_hook()` called `tokio::signal::unix::signal()` before any
  runtime existed (`cargo run` panicked: "there is no reactor running").
  Fixed by creating the runtime first and registering inside `block_on`,
  with the watcher as a `tokio::spawn` task instead of a raw thread.
- Verified via isolated tmux: clean start, dashboard renders, `q` quits,
  no panic. (`script`-PTY runs fail on cursor-position queries — harness
  artifact, needs a CPR-answering terminal.)

## Code review fixes (2026-10-06)

- **Live data pipeline.** All periodic virsh calls run in `app::poller` (background task) and reach the UI as
  `LiveMsg`s; the UI loop never waits on a subprocess. One `virsh domstats --raw` per tick feeds the table, the
  `metrics::store::MetricsStore` histories, shutdown tracking and the host panel. Mockup series are only used in
  `--demo`/tests.
- **Shutdown is tracked, not assumed.** `virsh shutdown` only sends an ACPI request and exits 0. The UI now shows
  `⏻ Shutdown requested … waiting for the guest…` and resolves it to `✓ <vm> shut off` or, after 90 s, a warning
  suggesting `:shutdown <vm> --mode agent` or `D`.
- **Hardware editor** is built from the real domain's inactive XML (`HardwareState::from_config`). With topology/model
  changes, the persistent config goes through one `virsh define` of the fully edited XML, followed only by `--live`
  native steps, so a stale define can never revert native `--config` changes. Unsupported edits are refused.
- **Temp files** go through `command::tempxml` (random name, O_EXCL, 0600, auto-removed) instead of predictable
  `/tmp/vt-<name>.xml` paths.
- **Demo safety net:** `command::exec::set_demo(true)` makes plan execution and `virsh::exec::run` refuse to spawn.
- **Untrusted text** (DHCP hostnames, event fields) is passed through `model::sanitize`; hand-built XML values go
  through `xml::escape`.
- **Balloon stats period is NOT set automatically** (it would change running guests); memory falls back to host RSS
  and says `no guest stats`. Revisit as an explicit opt-in in Settings › Monitoring.

## Review follow-up (2026-10-06, later)

- **App structure.** `app/mod.rs` is split into area modules (`live`, `domains`, `detail`, `networks`, `storage`,
  `wizard`, `palette`, `settings`, `ex`, `render`) and the loop state lives in `App` (`draw`, `on_live`, `on_tick`,
  `on_key`). Quit keys are checked after the text-input overlays.
- **Live views.** Networks/Storage/Host reload on every entry and after every operation; Networks parses
  `net-dumpxml` with roxmltree, maps leases to domains through `domiflist`, and plots bridge traffic from
  `/sys/class/net/*/statistics`. Storage computes backing chains after reading all volumes.
- **Events** carry a type and scope; `event`, `net-event` and `pool-event` streams share one channel; virsh-tui's
  own actions are logged with scope `app`.
- **Settings.** Every category is functional and persisted to the file the config came from (`--config` or the
  default path). Guest memory stats (`dommemstat --period N --live`) are opt-in. Confirmations can be disabled per
  action through `config::runtime()`.
- **Keymap.** `keymap.toml` overrides by snake_case action name; the Help overlay is generated from the live map.
- **Hardware shortcuts** (`␣da ␣dr ␣ia ␣il ␣M`) pre-fill the `:` line with the full virsh command instead of
  opening bespoke forms: the user sees and edits exactly what runs. Full virsh syntax on `:` always passes through.
- **Timeouts.** Long virsh jobs get a 24 h limit (others 30 s) and processes are killed on timeout.
- **Original limitation.** Actions and reads were awaited inside key handling
  and could keep the UI busy until they finished. Responsive jobs now schedule
  those operations separately; route evidence and platform limits are below.
- Remote host CPU and memory sampling is unavailable; the new-domain wizard is disabled for remote connections until reliable remote capacity facts are available.
- Snapshot creation uses Tab to select name, description, atomic, or quiesce, and Space toggles the selected option.


## Responsive execution

Live dashboard lifecycle shortcuts, snapshot confirmations, media changes, selected network and storage actions,
confirmed plans, raw virsh commands and several command-line actions now use
background jobs. Wizard creation and define-only submissions also use jobs;
a requested graphical viewer is opened after creation. Wizard XML preparation
and validation run in the background around the foreground editor.
Normal key dispatch is now synchronous and schedules backend work separately.
Route evidence is recorded in `superpowers/plans/2026-11-11-responsive-jobs-evidence.md`.
While a job runs, input is conservatively limited to navigation, filtering,
help, cached palette browsing, settings display, opening and editing a wizard draft,
`:messages`, `:help` and redraw, and `Ctrl-c` requests cancellation. A local cancellation
does not imply that libvirt rolled back or stopped server-side work.

A lone digit is resolved after 180 ms of idle time so view switches meet the
responsiveness target. Digits typed before that deadline still form a count.

The first frame, ordinary domain detail opening, network/pool selection,
completion, palette help, detail shortcuts, the media picker, manual refresh
and graphical viewer lookup now use background reads. Domain and network XML
validation and definition run as jobs, and network editor preparation runs
in the background. Edited XML that cannot safely be recovered in memory is
kept in its private file with its path reported, to avoid discarding user changes.
On Linux, macOS, FreeBSD and NetBSD, a foreground editor or console owns its process
group and the terminal during handoff; signal shutdown stops that group before
the terminal is restored. Linux has been exercised with PTY tests; macOS,
FreeBSD and NetBSD have been cross-checked for compilation but not tested at runtime.
Other Unix platforms still have limited cleanup after a foreground leader exits.

The responsive message-line states have text snapshots, and existing idle
snapshots remain unchanged. Screenshot regeneration was not run: neither
Google Chrome nor Chromium is available on the validation host. The optional
`aha` converter is also unavailable; mdBook and ImageMagick are available.

## Review fixes (2026-11-12 review, `docs/REVIEW-2026-11-12.md`)

- `Z` (P0-1): the app holds the first `Z` back. A second `Z` within 0.6 s
  quits (`ZZ`); a lone `Z` resolves through the keymap after the timeout and
  managed-save always confirms; `Z` + another key does nothing (vim). This
  replaces the P4 note above ("`Z` alone is managed-save").
- Confirmations are keyed on an explicit `ConfirmKind`, never on title or
  label text (P0-7). Pool/network destroy, detach, clone, managed-save,
  `:block-resize` and raw destructive commands always confirm.
- Sizes (P0-4) require a binary unit (`K`/`M`/`G`/`T`, optional `iB`); bare
  numbers and decimal units are rejected. Sizes reach virsh with an explicit
  unit (`kib_arg`). Disk shrink is refused (capacity from `domblkinfo`).
- The hardware form keeps the mockup's live `setvcpus N --live` step when
  only the maximum changes (harmless, fidelity); it is dropped when the value
  exceeds the running maximum (config-only + restart note, P1-12).
- `:attach-disk` (P0-5) infers the format from the extension (`qcow2`,
  `raw`/`iso`, `vmdk`, `vdi`, `vhdx`, `qed`) or the local image header
  (`QFI\xfb`, `KDMV`); otherwise the short form is refused with the full
  virsh command to type. No `qemu-img`/`vol-dumpxml` round trip.
- Orphan labels (P0-6) read inactive and live XML with `roxmltree`, resolve
  `<source pool= volume=>`, count backing references across all pools, and
  show `⚠ no domain` / `? unknown` (when a read failed) instead of `⚠ orphan`.
  The demo fixture keeps the mockup's `⚠ orphan`.
- Bulk jobs (P1-1) run every target; the single-target failure message keeps
  its `✗ step i/n failed: …` form (PTY tests match it), bulk adds
  `✗ n ok / m failed: names` and one `:messages` line per failure.
- Undefine (P1-4) always passes `--keep-nvram --snapshots-metadata
  --managed-save`: disks and UEFI vars stay, the metadata that blocks
  undefine goes. `has_managed_save` stays unused.
- Leaving Detail with pending hardware edits (`q`, `⌫`, view keys, opening
  another domain) is refused like `:q` (P1-9). `self.detail` is only a target
  while the Detail view is shown (`current_detail`, P0-3).
- Background reads have one slot and serial per kind (`ReadKind`, P1-49).
  Storage reads every active pool's volumes in the view load; pool switches
  are local (PF-1, P1-50). Networks read `domiflist` once per load.
- `virsh` table output is split at the header's column offsets
  (`table_rows`), with a whitespace fallback for unpadded output (P1-18/19).
- Small terminals (P1-40): from 40×12 to 100×30 every view renders a
  single list (`ui/views/minimal.rs`); below 40×12 the "too small" panel,
  and no overlay is drawn.
- `--osinfo` (P1-34) is `detect=on,require=off` unless the user typed an id;
  the ISO-name guess is only a hint. Wizard XML (define without starting,
  `C-e`) is one persistent phase, not `--print-xml=1` (P1-35, issue #5):
  phase 1 of a `--cdrom`/`--pxe` install ends in `<on_reboot>destroy</on_reboot>`
  and boots the media first forever. ISO/PXE add `--install no_install=yes`
  and append `hd,cdrom` / `hd,network` to `--boot`; URL installs must start
  after creation (no persistent XML to define). virsh failures join every
  `error:` line, since the first one is generic.
- `:messages` (P1-27) opens a scrollable overlay; the log keeps 2000 lines.
- Structured listings (`domstats`, `list`, `vol-list`, `pool-list`,
  `net-list`, `net-dhcp-leases`) may read 64 MiB of output (P1-51); raw
  commands keep the 1 MiB cap.
- Help (UX-7): `Tab` shows a second page of fixed per-view keys; its title
  gained ` ⇥ view keys`, the only change to the help snapshots. The
  which-key popup is generated from the live keymap.
- Not done: UX-5 (several jobs at once) would need a redesign of the single
  `active_job` slot that every view relies on; the one-job rule stays and is
  documented. UX-8: view-specific raw keys stay fixed (not remappable); they
  are listed on the second help page, and invalid key names in
  `keymap.toml` are now reported. PF-3: the autostart list is re-read every
  10 ticks (and after jobs) instead of every tick; a persistent virsh session
  was not attempted.

## Wizard ISO picker (2026-11-13)

- The ISO path field only listed known ISOs as a hint (`known: …`). `⏎` on
  it now opens a picker instead of going to the next step (`C-n` still
  does); the user asked for `⏎` only, not opening on focus.
- Sources: pool `.iso` volumes and a one-level scan of `[wizard] iso_dirs`.
  The default is the XDG download dir (`directories::UserDirs`, which reads
  `user-dirs.dirs`), not a hard-coded `~/Downloads`, because the project is
  public and folder names are localized. No recursion, so a large folder
  cannot stall the UI on open.
- "Used before" ISOs are not a separate source: virt-install creates a pool
  for the folder of a `--cdrom` path, so they already appear as pool ISOs.
- Remote URIs: local folders and the browser are off (the ISO must be on the
  remote host). Demo mode: the browser is off so screenshots never show real
  files.
- `qemu:///system` permission check: warn when an ancestor folder lacks
  `o+x`. Nothing is changed automatically (no chmod/ACL, no copy), per the
  safety rules; the warning suggests copying into a pool.
- `last_browse_dir` is saved with the draft but left out of
  `content_snapshot`, so browsing never makes a running create look stale.

## Logo and top-bar brand (2026-11-13)

- Logo: a rack of three VM blades whose LEDs use the domain-state colours
  (green running, yellow paused, grey shut off); the top blade shows `>_`.
  Same family as lazybw: mark + 5x7 pixel wordmark, tokyo-night colours,
  "tui" in blue. Generated by `scripts/make-logo.py` into
  `book/src/images/logo*.svg` and `book/theme/favicon.{svg,png}`.
- Top bar: the solid blue `◆ virsh-tui` chip was the loudest thing on the
  screen, used a glyph that already means "marked"/"snapshot", and drew the
  eye away from the active tab. The brand is now `▤ virsh-tui` in bold blue
  text on `bg2` with a `│` (gutter) divider, and the active tab is the solid
  blue pill (`bg2` text, bold). DESIGN.md §3, PLAN.md §9.1 and the mockups
  are updated to match.

## CPU performance pass (2026-11-13)

- Keep the subprocess backend and mutation previews unchanged. CPU benchmarks use
  fixtures and `TestBackend`, with no libvirt connection.
- Represent each braille dot column as an interval and encode cell masks directly;
  preserve the original chart maths and Unicode output. Interpolate multi-row
  sparks once per column and generate single-row sparks directly.
- Rank palette item indices, preserving identity for duplicate titles, and reuse
  one ranked result list when drawing the reference and command preview.
- Highlight only the visible XML page. Counting/scanning lines remains linear in
  document size; this deliberately avoids cache invalidation for public mutable
  XML state. Scroll positions are no longer narrowed to `u16`.
- Use borrowed name membership for metrics cleanup and borrow previous counters;
  avoid quadratic domain scans and copying per-device counters every tick. Derive
  aggregate totals from those same device counters rather than parsing them twice.
- Methodology, raw measurements and remaining limits: `performance/README.md`.
- View loads run their per-object `virsh` reads (`net-dumpxml`, `vol-list`) with
  bounded concurrency and overlap the independent top-level reads with
  `tokio::join!`; each `virsh` invocation is a fresh process whose startup cost
  dominated opening a view with many objects.

## Palette query matching (2026-11-13)

- Prepare the palette query as a nucleo `Atom` with `CaseMatching::Ignore` and
  `Normalization::Smart`. Passing the raw query to `Matcher::fuzzy_match` meant
  accented queries never matched and uppercase letters were case-sensitive.
  Case is ignored always (not fzf's "smart case"), so `Snap` and `snap` match the
  same entries. Whitespace stays a literal character, so no fzf-style
  `^`/`$`/`!` syntax is introduced.

## Packaging and release automation (2026-11-13)
- Release binaries are static musl builds for x86_64 and aarch64 (as in lazybw), so one binary runs on every distribution regardless of glibc (the previous build was linked against glibc 2.35 on `ubuntu-22.04`, so it could need up to that version). The archive name changes from `virsh-tui-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` to `virsh-tui-X.Y.Z-TARGET.tar.gz`.
- musl's own allocator made the release binary 1.4–2× slower on `cargo bench --bench performance`, so musl builds use mimalloc as the global allocator (`src/lib.rs`, `cfg(target_env = "musl")`, so the benchmarks use it too). One run each, the musl one in a Debian container: charts, fuzzy matching, palette and XML are on par with glibc; views are 9–23% slower (dashboard 174x43: 0.34 ms glibc, 0.48 ms plain musl, 0.42 ms with mimalloc); metrics ingest stays 1.3–1.65× slower (1000 domains: 2.2 / 4.2 / 2.8 ms), probably not allocation. All far below a frame budget. glibc builds (CI, `cargo install`, the AUR `virsh-tui` package) keep the system allocator. mimalloc is C, so building the musl target needs `musl-gcc` (`musl-tools` in CI).
- `.deb`/`.rpm` depend only on the libvirt client (`libvirt-clients` / `libvirt-client`) because the static binary has no shared-library dependencies (`auto-req = "no"` for the RPM); `virt-install` and `virt-viewer` are recommends. Arch depends on `libvirt` (the package that ships `virsh`); `namcap` warns it "may not be needed" because `virsh` is run, not linked.
- `[profile.release] strip = true` so the packages need no strip step (cross-architecture packaging cannot strip with the host `strip`); `[profile.bench]` keeps symbols for profiling.
- AUR publishing stays manual (`scripts/aur.sh`, `docs/RELEASE.md`), like lazybw: no AUR SSH key in GitHub secrets.
- GitHub Actions are pinned by commit SHA and updated by Dependabot; `dtolnay/rust-toolchain` was replaced by plain `rustup` commands.


## Compact UI audit follow-up (2026-11-13)

- The user approved the audit recommendations. Keep wide artboards and theme tokens; allocate compact space to editable values instead of shrinking all columns.
- Hardware review uses F6 so it works in NORMAL and INSERT without stealing typed letters. Wizard command review uses C-v and is excluded from draft persistence/content fingerprints. Palette reuses C-l rather than adding another pane shortcut.
- Confirmation preview scrolling is separate from typed names. All bulk commands are retained; the old six-command cap is removed. Below the minimum viewport, affirmative input is blocked because its preview cannot be displayed.
- Compact modes use focus-following lists, cell-aware command wrapping, fixed actions and concise instructions. Full app rendering is tested as well as individual widgets, because the global wizard status previously overwrote the compact footer.
