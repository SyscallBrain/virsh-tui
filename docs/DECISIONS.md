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
- Real `virsh snapshot-list --parent` columns are Name | Creation Time (date, time, tz) | State | Parent | Children | Descendants; parent is index 5, state index 4.
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
- Revert plan is multi-step: `[snapshot-create-as auto-before-revert-HHMM --atomic]` + `[snapshot-revert …]`; safety name uses live HHMM (frozen 1438 in demo/snapshot fixtures).
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
- **Known limitation.** Actions are awaited inside key handling, so a long job (migration, wipe) keeps the UI
  busy until it finishes. Moving executions to background tasks with progress in the message line is the next
  structural step.
