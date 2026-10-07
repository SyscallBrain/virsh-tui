# virsh-tui — Implementation Plan

> **Audience:** the coding agent that will build virsh-tui end to end.
> **Goal:** a complete, production-quality Rust + ratatui TUI for libvirt/virsh whose UI is **visually identical**
> to the approved design. Work through the phases in order. Do not skip acceptance criteria.

---

## 0. Read this first

### 0.1 Source-of-truth documents (read them in this order before writing code)

1. `docs/DESIGN.md` — the UI spec: tokens, layout, widgets, screens, and keymap. **Binding.**
2. `docs/mockups/*.dc.html` — the source of the 10 approved screens. They are HTML mockups and do not run as-is
   (they depend on a design-canvas runtime). Use them as the **exact reference** for:
   - every visible string (labels, hints, titles, footers, column headers, units, glyphs);
   - the demo data (domain names, numbers, leases, pools, volumes, snapshots, events), which is in the
     `renderVals()` arrays at the bottom of each file;
   - the colour of every element (CSS classes `.d`=comment, `.f2`=fg2, `.k`=orange bold, `.bl`=blue, `.cy`=cyan,
     `.mg`=magenta, `.gr`=green, `.ye`=yellow, `.or`=orange, `.rd`=red, `.tl`=teal, `.b`=bold;
     `var(--x)` = theme token `x`);
   - chart generation (`braille()`, `sparkRows()`, `hbar()`, `lg()`, `lvl()`, `grad()`, `series()`). Port these to Rust 1:1.
3. The rendered canvas of the mockups is private; everything it shows is in 1 and 2.

| Mockup file | Screen | Phase |
|---|---|---|
| `Main.dc.html` | 01 Domains dashboard | P3 |
| `Host.dc.html` | 02 Host monitor | P5 |
| `Detail.dc.html` | 03 Domain › Hardware editor (INSERT) | P7 |
| `Wizard.dc.html` | 04 New domain wizard (step 3) | P11 |
| `Palette.dc.html` | 05 Command palette + virsh reference | P12 |
| `Help.dc.html` | 06 Keybindings overlay | P12 |
| `Networks.dc.html` | 07 Networks | P9 |
| `Storage.dc.html` | 08 Storage + insert-media picker | P10 |
| `Snapshots.dc.html` | 09 Snapshots + confirm modal | P8 |
| `Settings.dc.html` | 10 Settings › Themes | P13 |

### 0.2 Hard rules

- **Everything in English:** UI strings, code, identifiers, comments, docs, commit messages, and the README.
- **Fidelity over invention.** If the mockup shows it, reproduce it (text, order, colour, glyph, alignment).
  If something is not mocked (for example the Events view or wizard steps 1, 2, 4, 5 and 6), build it from the same
  components and conventions so that it looks like it belongs. Never introduce new colours, borders or styles.
- **Safety with the user's real VMs.** This machine has real domains on `qemu:///system`
  (`archlinux-install`, `fedora-install`, `metasploitable2`). During development:
  - Only run **read-only** virsh commands against existing domains (`list`, `dumpxml`, `domstats`, `dominfo`,
    `net-*list`, `pool-*list`, `vol-list`, `snapshot-list`, `help`, …).
  - Mutating commands may only target throwaway objects that **you** created, named with the prefix `vt-test-`
    (domains, networks, pools, volumes). Delete them when done.
  - Use `--dry-run` (see §6.4) when exercising flows on real objects.
  - Never use `sudo`. If something needs root, stop and tell the user.
- **Quality gates for every phase:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test` all pass. Commit at the end of each phase with a message `phase N: <summary>`.
- No `unwrap()`/`expect()` outside tests and `main` setup. Use `color_eyre::Result` at the edges and `thiserror`
  inside modules.
- Keep the terminal sane: always restore it on exit, on panic (panic hook), and on `SIGTERM`/`SIGINT`.

### 0.3 Environment facts (verified on the user's machine)

- Rust 1.99, cargo available. Debian (testing), zsh. User is in the `libvirt` and `kvm` groups.
- `virsh` 12.8.0, `virt-install`, `virt-clone`, `virt-xml`, `virt-viewer`, and `remote-viewer` are installed.
- **libvirt development headers are NOT installed** (`pkg-config libvirt` fails). This is one of the reasons for the
  backend decision in §3.2.
- `node` is installed (used for chart fixtures in P1).
- `osinfo-query` is not installed. Use `virt-install --osinfo list` for the OS list.
- `virsh help` lists 285 commands. `virsh help <cmd>` has a stable NAME/SYNOPSIS/DESCRIPTION/OPTIONS layout.

---

## 1. Product scope (definition of done)

virsh-tui is done when a virsh power user can do **all** of the following without leaving the TUI, and every
mutating action shows the exact command before running it:

**Domains:** list/filter/sort, live metrics, start, shutdown, destroy, reboot, reset, suspend/resume, managed-save
(and remove), autostart toggle, rename, clone, undefine (with storage/NVRAM/snapshot-metadata options), define from
XML, edit XML in `$EDITOR`, export XML, serial console, graphical viewer, send Ctrl-Alt-Del (`send-key`), migrate
(live, persistent, undefine-source, to a `qemu+ssh://` URI), and bulk actions on marked domains.

**Hardware:** CPUs (count, max, topology, model, features, pinning, emulatorpin, iothreads), memory (current, max,
balloon, hugepages, shared memory), boot (firmware BIOS/UEFI, secure boot, boot order, boot menu), disks (attach,
detach, resize, bus, cache, io, discard, readonly), CDROM (insert/eject ISO), NICs (attach, detach, source, model,
MAC, link up/down, bandwidth), display (SPICE/VNC, listen, port, password), video, sound, input, TPM, USB redirection,
and host-device passthrough (USB and PCI via `nodedev-list`).

**Snapshots:** tree view, create (with every `snapshot-create-as` option), revert (with auto-safety snapshot),
delete (with children/metadata options), and edit XML.

**Networks:** list, detail, create (NAT/isolated/bridge/routed wizard-lite form), start, destroy, autostart, undefine,
edit XML, DHCP leases, and static leases via `net-update`.

**Storage:** pools (define-as, build, start, destroy, refresh, autostart, undefine, delete), volumes (create, clone,
resize, upload, download, wipe, delete), backing-chain and orphan detection, and the ISO library picker.

**Host:** CPU per thread, memory, load, temperature, versions, KVM capabilities, and pools/networks summary.

**App:** vim modal input, counts, `.` repeat, leader with which-key, ex command line, command palette with the
virsh reference for all 285 commands, a generic form for any raw virsh command, events log, 9 themes plus custom
themes, settings persisted to TOML, remappable keymap, multiple connections (`:connect`), `--dry-run`, and `--demo`.

---

## 2. Tech stack

Add crates with `cargo add` so you get the latest compatible versions. Do not pin from memory.

| Purpose | Crate |
|---|---|
| TUI | `ratatui` (with `crossterm` backend), `crossterm` (feature `event-stream`) |
| Async runtime | `tokio` (features: `rt-multi-thread`, `macros`, `process`, `sync`, `time`, `signal`, `io-util`, `fs`) |
| Streams | `futures` (for `EventStream`) |
| CLI | `clap` (derive) |
| Errors | `color-eyre`, `thiserror` |
| Logging | `tracing`, `tracing-subscriber`, `tracing-appender` (log file only; never log to the terminal) |
| Config | `serde`, `toml`, `directories` |
| XML | `roxmltree` (fast read-only parsing) + `xmltree` (mutable DOM for edits) |
| Diff | `similar` (line diff for the XML diff panel) |
| Fuzzy matching | `nucleo-matcher` |
| Shell quoting for display | `shlex` |
| Clipboard | `arboard`, with OSC 52 fallback written manually |
| Regex | `regex` |
| Time | `jiff` |
| Unicode width | `unicode-width` |
| Tests | `insta`, `pretty_assertions`, `tempfile` |

---

## 3. Architecture

### 3.1 Runtime model (unidirectional, TEA-style)

```
             ┌─────────────── tokio runtime ───────────────┐
 crossterm   │                                              │
 EventStream ─┼─► Input ─► KeyEngine ─► Action ─┐            │
 tick (1s)   ─┼──────────────────────────► Action ─┤        │
 backend     ─┼─► BackendEvent ─────────► Action ─┤        │
 (stats,     │                                    ▼        │
  events,    │                       App::update(&mut State, Action) -> Vec<Effect>
  cmd result)│                                    │        │
             │       Effect executor ◄─────────────┘        │
             │       (spawn virsh, fetch, open editor…)     │
             │                │ results as Action           │
             │                ▼                             │
             │       render(&State, &Theme, &mut Frame)     │  ← pure, no I/O
             └──────────────────────────────────────────────┘
```

- `State` is plain data. `update` is synchronous and pure (except logging), so it is easy to unit-test.
- `Effect` is an enum of side effects (`RunCommand(CommandPlan)`, `Fetch(FetchKind)`, `OpenEditor{..}`,
  `SuspendFor(ExternalProgram)`, `CopyToClipboard(String)`, `SaveConfig`, …).
- Rendering happens after every processed batch of actions, capped at 60 fps. There is no redraw when nothing changed.
- External programs that take over the terminal (`virsh console`, `$EDITOR`, the `:!` pager) suspend the TUI
  (leave the alternate screen, disable raw mode), run, and then restore and force a redraw.

### 3.2 Backend decision: a virsh subprocess backend behind a trait

Implement `trait Backend` with two implementations:

1. **`VirshBackend`** (the real one). It runs `virsh -c <uri> …` as subprocesses with `LC_ALL=C`, parses
   machine-friendly output, and runs **mutations by executing exactly the argv shown to the user**. That makes the
   "shows the exact command" promise true by construction. It needs no native dependencies and works the same with
   remote URIs.
2. **`DemoBackend`**. It is fully in-memory and deterministic (seeded). It reproduces the mockup data exactly and
   simulates metrics and state transitions. Mutations update the in-memory model and "succeed" after a short delay.
   It is used by `--demo`, by snapshot tests, and for development.

(A future `virt`-crate FFI backend can be added behind the same trait. It is not part of this plan.)

```rust
#[async_trait] // or return-position impl Future in traits; pick one and stay consistent
pub trait Backend: Send + Sync {
    async fn list_domains(&self) -> Result<Vec<DomainSummary>>;
    async fn domain_xml(&self, name: &str, inactive: bool) -> Result<String>;
    async fn all_domain_stats(&self) -> Result<Vec<RawDomainStats>>;   // virsh domstats --raw
    async fn host_info(&self) -> Result<HostInfo>;
    async fn host_sample(&self) -> Result<HostSample>;                 // cpu per thread, mem, load, temp
    async fn networks(&self) -> Result<Vec<NetworkSummary>>;
    async fn network_xml(&self, name: &str) -> Result<String>;
    async fn dhcp_leases(&self, net: &str) -> Result<Vec<DhcpLease>>;
    async fn host_interfaces(&self) -> Result<Vec<HostIface>>;
    async fn pools(&self) -> Result<Vec<PoolSummary>>;
    async fn volumes(&self, pool: &str) -> Result<Vec<Volume>>;
    async fn snapshots(&self, domain: &str) -> Result<Vec<Snapshot>>;  // includes parent links
    async fn domain_addresses(&self, domain: &str) -> Result<Vec<IfAddr>>;
    async fn virsh_help_index(&self) -> Result<Vec<HelpEntry>>;
    async fn virsh_help(&self, cmd: &str) -> Result<HelpDoc>;
    async fn os_variants(&self) -> Result<Vec<OsVariant>>;
    async fn run(&self, plan: &CommandPlan) -> Result<CommandOutput>;  // the only mutation entry point
    fn subscribe_events(&self) -> tokio::sync::mpsc::Receiver<LibvirtEvent>;
}
```

### 3.3 Module layout (single binary crate)

```
src/
  main.rs                 // CLI parsing, terminal setup/teardown, panic hook, runtime start
  cli.rs                  // clap: --connect/-c URI, --demo, --dry-run, --theme, --config, --screen, --dump, --size
  app/
    mod.rs                // App, main loop, batching, fps cap
    state.rs              // State + sub-states per view
    action.rs             // Action enum
    effect.rs             // Effect enum + executor
    update.rs             // update() dispatch → per-view update fns
    mode.rs               // Mode enum + mode pill metadata
  backend/
    mod.rs                // Backend trait, shared types
    virsh/
      mod.rs              // VirshBackend
      exec.rs             // process spawning, timeouts, LC_ALL=C, stderr capture
      parse_list.rs       // list/net-list/pool-list/vol-list tables
      parse_domstats.rs   // domstats --raw
      parse_xml.rs        // domain/network/pool/vol/snapshot XML → models
      parse_help.rs       // virsh help / help <cmd>
      parse_leases.rs
      events.rs           // long-running `virsh event --all --loop --timestamp` (+ net-event, pool-event)
      host_local.rs       // /proc/stat, /proc/meminfo, /proc/loadavg, /proc/cpuinfo, hwmon (local URIs only)
    demo/
      mod.rs              // DemoBackend
      fixtures.rs         // data copied from docs/mockups (see §10)
      sim.rs              // deterministic random-walk metrics, state transitions
  model/                  // Domain, DomainConfig (devices), Network, Pool, Volume, Snapshot, Host, Event…
  metrics/
    ring.rs               // RingBuffer<f32> with multi-resolution (60s@1s, 5m@1s, 1h@10s)
    sampler.rs            // turns cumulative counters into rates; per-domain & host histories
  command/
    plan.rs               // CommandPlan { steps: Vec<CommandStep> }, CommandStep { program, argv, stdin, temp_files }
    builders/             // one file per area: lifecycle.rs, hardware.rs, snapshot.rs, network.rs, storage.rs, install.rs, migrate.rs
    display.rs            // shlex-quoted single-line display + syntax-coloured Spans (see §7.3)
    history.rs            // executed command log (for `yc` and :messages)
  xml/
    edit.rs               // typed edit operations on xmltree (set vcpu, topology, cpu model, boot order…)
    diff.rs               // pretty-print + similar line diff → Vec<DiffLine>
    highlight.rs          // XML syntax highlighting for the XML tab
  input/
    key.rs                // KeyChord parsing ("C-d", "␣sc", "gg") from/to strings
    engine.rs             // count prefix, pending sequences, leader timeout, which-key trigger
    keymap.rs             // default keymap + TOML overrides, context-sensitive (view, mode)
    textinput.rs          // single-line editable input with cursor (INSERT mode fields, palette, ex line)
    ex.rs                 // ex command parser (:start x, :setmem x 8G --live, :theme, :connect…)
  theme/
    mod.rs                // Theme { tokens }, style helpers (Theme::panel(focused) etc.)
    builtin.rs            // the 9 themes (§5.1)
    load.rs               // ~/.config/virsh-tui/themes/*.toml
  config.rs               // Config struct, load/save ~/.config/virsh-tui/config.toml
  ui/
    mod.rs                // root render: top bar, body (by view), status bar, message line, overlays
    layout.rs             // reference-size layouts & breakpoints
    chrome/ topbar.rs statusbar.rs messageline.rs
    widgets/              // reusable widgets (§7)
      panel.rs table.rs braille.rs sparkline.rs gauge.rs hbar.rs slider.rs stacked_bar.rs
      thread_meters.rs chips.rs form.rs modal.rs tree.rs keyhints.rs whichkey.rs fuzzy_list.rs
    views/
      dashboard.rs host.rs networks.rs storage.rs events.rs
      domain/ mod.rs overview.rs monitor.rs hardware.rs snapshots.rs console.rs xml.rs
    overlays/
      wizard.rs palette.rs help.rs confirm.rs media_picker.rs settings.rs ex_line.rs which_key.rs
tests/
  snapshots.rs            // insta snapshot tests of every screen in demo mode (§11)
  fixtures/virsh/         // captured real virsh outputs (read-only commands) for parser tests
```

---

## 4. Data model and metrics

### 4.1 Core types (summarised; add fields as needed)

```rust
pub enum DomainState { Running, Paused, ShutOff, Crashed, PmSuspended, Transition(TransitionKind) }
pub struct DomainSummary { name, uuid, id: Option<u32>, state: DomainState, autostart: bool, persistent: bool,
                           vcpus: u32, max_vcpus: u32, mem_kib: u64, max_mem_kib: u64, has_managed_save: bool }
pub struct DomainConfig { os: OsInfo, machine, firmware: Firmware, secure_boot: bool, cpu: CpuConfig,
                          memory: MemoryConfig, boot_order: Vec<BootDev>, devices: Vec<Device>,
                          graphics: Option<Graphics>, agent_connected: Option<bool> }
pub enum Device { Disk(Disk), Cdrom(Disk), Nic(Nic), Video(..), Sound(..), Input(..), Tpm(..),
                  UsbRedir(..), HostDev(..), Controller(..), Channel(..), Other(String) }
pub struct DomainMetrics { cpu_pct: Ring, mem_used: Ring, disk_rd: Ring, disk_wr: Ring, iops: Ring,
                           net_rx: Ring, net_tx: Ring, totals: NetTotals, ... }
pub struct LibvirtEvent { ts, kind: EventKind, object: ObjRef, event: String, detail: String, source: EventSource }
```

### 4.2 Sources and formulas (VirshBackend)

| Data | Command / source | Notes |
|---|---|---|
| Domain list | `virsh list --all --name` + `domstats --raw --state --vcpu --balloon` | Use domstats for state, vcpus, and memory. Autostart and persistent come from `virsh list --all --autostart` / `--persistent` name lists (one call each). |
| Config | `virsh dumpxml <d>` (live) and `--inactive` (pending config) | Parse with roxmltree into `DomainConfig`. Cache by a hash of the XML. |
| Stats (all, 1 s) | `virsh domstats --raw` (all groups) | **One call per tick for all domains.** |
| CPU % | Δ`cpu.time`(ns) / (Δwall(ns) × `vcpu.current`) × 100 | Clamp to 0–100. The Host view's "CPU" column uses the same per-domain %. |
| Memory used | `balloon.available − balloon.unused` (KiB), fallback `balloon.rss` | The guest needs stats polling enabled: on first sight of a running domain, if `Config.monitoring.balloon_period` is on (default **on**), run `virsh dommemstat <d> --period 2 --live` (live only, never persistent). Document it in Settings › Monitoring. |
| Host RSS | `balloon.rss` | Host memory stacked bar. |
| Disk R/W | Δ`block.N.rd.bytes` / Δ`wr.bytes`, IOPS = Δ(`rd.reqs`+`wr.reqs`) | Sum over disks for the domain total. Per-disk values for the detail view. |
| Net rx/tx | Δ`net.N.rx.bytes`/`tx.bytes`, totals, `rx.drop`, `rx.errs` | Show as bits/s (Mb/s) in the UI, as in the mock. |
| IP | `virsh domifaddr <d> --source lease`, fallback `--source agent` | Only for the selected domain, every 10 s. |
| Guest agent | live XML channel `org.qemu.guest_agent.0` `state='connected'` | No guest commands needed. |
| Uptime | local URI: pid from `/run/libvirt/qemu/<name>.pid` → `/proc/<pid>/stat` starttime; else first-seen-running time prefixed `≥` | Show `—` when unknown. |
| Host CPU per thread, load, freq, temp | local: `/proc/stat`, `/proc/loadavg`, `/proc/cpuinfo`, `/sys/class/hwmon/*` (`k10temp` Tctl, `coretemp` Package) | Remote: `virsh nodecpustats --percent` (blocks 1 s; run in a background task) and hide per-thread, temp, and freq. |
| Host memory | local `/proc/meminfo` (incl. HugePages_*, KSM from `/sys/kernel/mm/ksm/pages_sharing`) | Remote: `virsh nodememstats`. |
| Host static | `virsh nodeinfo`, `virsh version --daemon`, `virsh capabilities`, `uname -r` (local) | Once at startup and on reconnect. |
| Events | long-running `virsh event --all --loop --timestamp`, plus `net-event --all --loop --timestamp` and `pool-event --all --loop --timestamp` | Parse with regex (format: `<ts>: event '<type>' for domain '<name>': <Event> <Detail>`). Restart with backoff if it dies. App actions also push synthetic events (for example "snapshot created"). |
| Networks | `net-list --all --name` + `net-dumpxml`, `net-info` | |
| Leases | `net-dhcp-leases <net>` | Parse the table. Static hosts come from the `<dhcp><host>` elements in the XML. |
| Host interfaces | local `/sys/class/net/*` (type, speed, operstate) + `ip -j addr` | Attached vnets come from domain XML `<target dev>`. |
| Pools / volumes | `pool-list --all --details`, `pool-dumpxml`, `vol-list --pool p --details`, `vol-dumpxml` | "Used by" is computed by matching volume paths against all domain disk sources. "Backing of N" comes from `<backingStore>`. "Orphan" means used by no domain and not a backing file. |
| Snapshots | `snapshot-list <d> --parent` + `snapshot-current --name` + `snapshot-dumpxml` (lazily for the selected one) | Build the tree from the parent links. |
| OS list | `virt-install --osinfo list` | Cache in `~/.cache/virsh-tui/osinfo.json`. |
| Help | `virsh help` (index) and `virsh help <cmd>` | Parse into `HelpDoc { name, summary, synopsis, description, options: Vec<HelpOpt{flag, arg: Option<String>, required, positional, repeatable, desc}> }`. Cache per virsh version in `~/.cache/virsh-tui/help-<version>.json`. |

**Capture parser fixtures:** in P2, run each read-only command above on this machine and save the outputs to
`tests/fixtures/virsh/<command>.txt`. Write parser unit tests against them. Also hand-write fixtures for states the
machine lacks (running domain domstats, leases, snapshots). Use the exact formats from libvirt documentation.

### 4.3 Ring buffers

- Every metric has three resolutions: 60 samples @1 s, 300 @1 s, and 360 @10 s (1 h). `t` switches the visible one.
- Rates are computed from cumulative counters. On a counter reset (domain restarted), drop the sample.
- Keep histories for all running domains (cheap), so switching selection shows full history immediately.

---

## 5. Theme system

### 5.1 Tokens and built-in themes

There are 16 tokens in this exact order: `bg bg2 hl sel gutter comment fg fg2 blue cyan magenta green yellow orange red teal`.
Meanings are in DESIGN.md §2. Built-in values (copied from the mockups; they must match exactly):

```
tokyo-night        #1a1b26 #16161e #292e42 #283457 #3b4261 #565f89 #c0caf5 #a9b1d6 #7aa2f7 #7dcfff #bb9af7 #9ece6a #e0af68 #ff9e64 #f7768e #73daca
tokyo-night-storm  #24283b #1f2335 #292e42 #2e3c64 #3b4261 #565f89 #c0caf5 #a9b1d6 #7aa2f7 #7dcfff #bb9af7 #9ece6a #e0af68 #ff9e64 #f7768e #73daca
tokyo-night-day    #e1e2e7 #d0d5e3 #c4c8da #b7c1e3 #a8aecb #6172b0 #3760bf #4c5a8f #2e7de9 #007197 #9854f1 #587539 #8c6c3e #b15c00 #f52a65 #118c74
catppuccin-mocha   #1e1e2e #181825 #313244 #45475a #45475a #7f849c #cdd6f4 #bac2de #89b4fa #89dceb #cba6f7 #a6e3a1 #f9e2af #fab387 #f38ba8 #94e2d5
gruvbox-dark       #282828 #1d2021 #3c3836 #504945 #504945 #928374 #ebdbb2 #d5c4a1 #83a598 #8ec07c #d3869b #b8bb26 #fabd2f #fe8019 #fb4934 #8ec07c
nord               #2e3440 #272c36 #3b4252 #434c5e #4c566a #7b88a1 #eceff4 #d8dee9 #81a1c1 #88c0d0 #b48ead #a3be8c #ebcb8b #d08770 #bf616a #8fbcbb
dracula            #282a36 #21222c #343746 #44475a #44475a #6272a4 #f8f8f2 #e2e2dc #bd93f9 #8be9fd #ff79c6 #50fa7b #f1fa8c #ffb86c #ff5555 #8be9fd
kanagawa           #1f1f28 #16161d #2a2a37 #2d4f67 #363646 #727169 #dcd7ba #c8c093 #7e9cd8 #7fb4ca #957fb8 #98bb6c #e6c384 #ffa066 #e46876 #7aa89f
everforest         #2d353b #232a2e #343f44 #475258 #475258 #859289 #d3c6aa #9da9a0 #7fbbb3 #83c092 #d699b6 #a7c080 #dbbc7f #e69875 #e67e80 #83c092
```

- Use `Color::Rgb`. Detect truecolor (`COLORTERM=truecolor|24bit`). If it is absent, map each colour to the nearest
  xterm-256 index (implement the standard 6×6×6 cube + greyscale nearest match) and log a one-time hint.
- `transparent = true` renders `bg` as `Color::Reset`. `bg2`, `hl`, and `sel` stay coloured.
- Custom theme file `~/.config/virsh-tui/themes/<name>.toml`: a `[tokens]` table with the 16 keys as `"#rrggbb"`.
  A missing key is an error shown in the message line, and the app falls back to tokyo-night.

### 5.2 Style helpers (use these everywhere; never build ad-hoc styles in views)

`theme.text()`, `.dim()` (comment), `.secondary()` (fg2), `.key()` (orange+bold), `.accent()` (blue),
`.panel_border(focused)`, `.panel_title(focused)`, `.row_selected(focused)` (bg sel or hl), `.chip()` (bg hl fg fg2),
`.chip_active(color)`, `.mode_pill(mode)`, `.state(DomainState)`, `.level(f32)` (green <0.5, yellow <0.8, red),
`.metric(Metric)` (CPU gradient / magenta / green / orange / cyan / yellow).

---

## 6. Commands: plan, display, execution

### 6.1 CommandPlan

Every mutating feature produces a `CommandPlan` (one or more `CommandStep`s) **before** anything runs. The plan is
shown in the relevant panel ("Will run", "Equivalent command", palette preview, confirm modal), and the same plan is
executed. Steps run sequentially and stop at the first failure. Partial failures are reported in the message line
as `✗ step 2/3 failed: <stderr first line>`, with full stderr in `:messages`.

### 6.2 Builders (examples; implement the complete matrix in Appendix A)

```
lifecycle::start(d)                 → virsh start d
lifecycle::shutdown(d)              → virsh shutdown d
lifecycle::destroy(d)               → virsh destroy d                          (confirm)
hardware::set_vcpus(d, cur, max, live, config)
   → [virsh setvcpus d max --maximum --config]  (only if max changed)
     [virsh setvcpus d cur --live --config]      (flags from Apply-to)
hardware::topology/model/features   → virsh define <tempfile>                  (XML edit path)
media::insert(d, target, iso, live, config) → virsh change-media d sda <iso> --update --live --config
snapshot::revert(d, s, opts)        → [snapshot-create-as d --name auto-before-revert-HHMM --atomic]  (if safety on)
                                      virsh snapshot-revert d s --running
```

**XML edit path:** `dumpxml --inactive` → apply typed `xml::edit` ops on xmltree → write to
`$XDG_RUNTIME_DIR/virsh-tui/vt-<domain>.xml` (fallback `/tmp`) → `virsh define <file> --validate`. The display shows
`virsh define /tmp/vt-<domain>.xml` exactly as in the mockup (use the real path).

### 6.3 Display formatting (exactly as in the mockups)

A command renders as styled spans:
`$ ` (comment) · program `virsh`/`virt-install` (green) · subcommand (blue) · domain/object positional (fg) ·
flags `--x` (cyan) · numeric arguments (orange) · paths/names/strings (yellow) · line continuation `\` (comment).
Multi-line display (virt-install) puts one flag group per line, indented 2, with a trailing ` \`.
Use `shlex` quoting for copy-to-clipboard text.

### 6.4 Execution

- `tokio::process::Command`, `LC_ALL=C`, `-c <uri>` always explicit, stdin null, 30 s default timeout
  (configurable). Long jobs (clone, vol-upload, migrate, wipe) have no timeout and show progress in the message line
  with a braille spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, blue). Cancel with `C-c` (send SIGINT, then kill after 3 s).
- `--dry-run`: `run()` does not execute. It prints `[dry-run] <command>` to the message line and log, and returns success.
- Every executed command is appended to the history (`yc` copies the last one; `:messages` shows the log).
- Success message format (message line): `✓ <Past-tense summary>  ── <first command>` (summary fg, `✓` green, rest comment).
  Example: `✓ Started k8s-worker-02  ── virsh start k8s-worker-02`.
- After a mutation, trigger an immediate refresh of the affected objects.

---

## 7. Widget library (`ui/widgets`) — exact rendering rules

Build these in **P1** together with a hidden `--screen gallery` demo screen and a snapshot test for each one.
Port the algorithms from the mockup scripts 1:1 (same maths, same characters).

1. **Panel**: `Block::bordered().border_type(Rounded)` (style from config), border `gutter`, or `blue` when focused.
   Title: `" Title "` bold, `fg2` (`blue` when focused), top-left. Optional `title_right` (top, right-aligned,
   comment, may contain coloured spans), `footer_left` and `footer_right` (bottom, comment). Inner horizontal padding 1.
2. **Table**: header row comment+bold. Under the header, a full-width `─` line in `hl`. Rows are 1 line.
   Column 0 = selection marker `▌` (blue) on the selected row. Selected row bg `sel` (focused) or `hl`; selected
   name bold `fg`. Column gaps = 1 space. Scrolling keeps the selection visible with a 2-row scrolloff (vim
   `scrolloff=2`). Supports marks (`◆` magenta in col 1).
3. **BrailleChart**: port `braille(d, w, h, mode)` with modes `Line`, `Fill`, `Down`. Each output row gets one
   colour from `grad(rows, colors)` (top→bottom). Optional left gutter (5 cols) with labels on rows 0 / mid / last
   (`100%`, ` 50%`, `  0%`), and an x-axis line `-60s … -30s … now` (justify space-between). Sampling uses the
   `sample()` linear interpolation from the mockup.
4. **MirroredBraille**: rx `Fill` rows (cyan) + one dashed separator row `╌` in `gutter` + tx `Down` rows (yellow).
5. **SparkRows**: port `sparkRows(d, w, h)` (multi-row `▁▂▃▄▅▆▇█`). `spark(d, w)` is the single-row case.
6. **LineGauge**: `━` × round(v·w) in level/metric colour, then `━` in `hl` for the rest, then ` value` in fg2.
   (Note: the mock uses `━` for the track too, coloured `hl`.)
7. **HBar** (CPU% column): port `hbar(v, w=16)`: `█` × full + partial `▏▎▍▌▋▊▉`, track `░` in `hl`, then `NN%`
   right-aligned in 5 cols. Bar + percent are coloured `level(v)` when running. For non-running rows, show
   `  —` in comment.
8. **ThreadMeters**: N columns 2 cells wide + 1 gap. 3 rows from `sparkRows([v,v],2,3)` coloured
   `red / blue / cyan` top→bottom, plus an index row (comment, right-aligned 2 digits).
9. **StackedBar**: segments of `█` coloured by palette order `orange, blue, magenta, cyan, green, yellow, teal, red, fg2`,
   each width round(GiB/total·W) with a minimum of 1, and the remainder `█` in `hl`. A legend grid of 2 columns:
   `■ name(padEnd 15) value(padStart 6)`.
10. **Slider**: `━` × n in metric colour + `●` bold + `─` × rest in `gutter` (width 36 in the wizard).
11. **Chips**: ` text ` on bg `hl` fg `fg2`. A key inside a chip uses the key style. An active chip uses
    bg `sel` fg `fg` (filters) or bg metric colour fg `bg2` (wizard presets, primary buttons). Chips are separated by 1 space.
12. **Form fields** (`form.rs`): label (fg2, fixed width) · value box `" value "` bg `hl`. In NORMAL mode the focused
    field is bg `sel`. In INSERT mode the focused field is bg `sel`, the cursor is a block (bg fg / fg bg), and a green
    "outline" is approximated by green `▕`/`▏` half-blocks on both sides of the box. Checkbox `[x]` green / `[ ]` comment.
    Radio `(●)` blue / `( )` comment. Select: `value ▾`, which opens a bordered dropdown list (selected line bg `sel` with
    `❯ ` blue). Section headers are blue bold. Warnings are `⚠ text` in yellow. Numeric fields: `h/l` or `C-a/C-x` ±1,
    `H/L` ×step. Size fields accept `512M`, `8G`, `1.5T`.
13. **Modal**: centered `Rect`. The background is re-rendered with every style replaced by `comment`/`gutter` fg and
    no bg colours (the "dim behind modals" effect; it can be disabled in settings, in which case the background is
    left unchanged). Use `Clear` before drawing the modal. Border blue (or red for CONFIRM). The modal background is `bg`.
14. **Tree**: box-drawing prefixes `├─ │  └─` in comment, as in the snapshot mockup.
15. **KeyHints**: sequences of `key` (orange bold) + ` label` (comment), separated by 2 spaces. They truncate from the
    right with `…` when they do not fit.
16. **WhichKey**: after the leader `␣` (or any pending prefix such as `g`, `y`, `␣s`) and 300 ms without input, show a
    small bordered popup bottom-right listing the next keys (`key  description`, keys orange). Not mocked; follow
    Panel + KeyHints styles.
17. **FuzzyList**: nucleo matching. Matched characters are orange bold, the rest fg. Category prefix in comment.
    Right-aligned key binding in orange.
18. **XmlView**: tag names blue, attribute names cyan, values yellow (green for strings in quotes is acceptable only
    if the mock does not show it; prefer yellow), text fg, comments comment. Fold with `za`/`zR`/`zM`. Diff lines:
    `- ` red, `+ ` green, context fg2, hunk headers `@@ … @@` comment.

---

## 8. Input system (vim)

### 8.1 Modes

`Normal`, `Insert` (editing a form field or text input), `Visual` (range selection in tables), `Command` (`:` line),
`Palette`, `Picker`, `Confirm`. Pill labels are uppercase (`NORMAL`, `INSERT`, `VISUAL`, `COMMAND`, `PALETTE`,
`PICKER`, `CONFIRM`). Colours are in DESIGN.md §3. The message line shows `-- INSERT --` / `-- VISUAL --` in comment
while in those modes.

### 8.2 Key engine

- Parses counts (`3j`, `5G`), multi-key sequences (`gg`, `yy`, `␣sc`, `ZZ`), and the leader (`space`).
- Keeps a pending buffer. A completed match dispatches `Action::Key(KeyAction, count)`. A partial match waits. After
  300 ms with a partial match, it shows which-key. `Esc` clears the pending buffer.
- `.` repeats the last *mutating* `KeyAction` with the same count on the current selection (or marks).
- Bindings are context-scoped: `(View, Mode) → HashMap<KeySeq, KeyAction>`, with `Global` as fallback.
- `~/.config/virsh-tui/keymap.toml` overrides by action name, for example
  `[normal.domains] "shutdown" = ["S"]`. Unknown action names give a startup warning in the message line.
- Default bindings: exactly DESIGN.md §6 and `docs/mockups/Help.dc.html`. The Help overlay is **generated from
  the live keymap** (so remaps show up), grouped into the same sections and order as the mockup.

### 8.3 Ex commands (`:`)

Prompt `:` on the message line (mode COMMAND, yellow pill), with Tab completion (commands, domain names, theme
names, file paths for ISOs) shown in a popup above the line (FuzzyList style). History with `↑/↓` (persisted to
`~/.local/state/virsh-tui/history`). Implement the commands in DESIGN.md §6 plus `:messages`, `:set <option>=<value>`,
`:help <topic>`, and `:<n>` (jump to row n). Any unknown `:word args` whose word is a virsh command runs as
`virsh word args` (with confirmation if the command is in the destructive list). `:!virsh …` runs raw and shows the
output in a pager modal.

---

## 9. Screens — layout at the reference size

**Reference terminal: 174 columns × 43 rows** (the mockups are 1360×860 px ≈ 7.8 px × 20 px cells).
All snapshot tests use 174×43. Mockup rows with extra leading (24–30 px) are **one** terminal row.

Global vertical split: `Length(1)` top bar · `Min(0)` body · `Length(1)` status bar · `Length(1)` message line.
Body margin: 1 column left and right. No top margin is needed because titles sit on the borders. Horizontal gap
between columns: 1. Vertical gap between stacked panels: 0 at heights below 48 rows, 1 at 48 rows or more.

### 9.1 Top bar (all views)
`[◆ virsh-tui]` (bg blue, fg bg2, bold, padded 1) + tabs `" N Label "` (N orange bold; active tab bg `bg` fg blue bold;
inactive comment) + flexible gap + `⌁ ` (comment) `qemu:///system` (fg2) `  │  ` (comment) `forge` (hostname, fg2) `  │  ⟳ ` `1s` `  │  ` `HH:MM:SS  `.
Tabs: `1 Domains  2 Host  3 Networks  4 Storage  5 Events`. Settings shows `⚙ settings  │  HH:MM:SS` on the right.

### 9.2 Status bar and message line
Status bar: mode pill · breadcrumb (bg hl, fg fg2, padded 1) · key hints (context-specific, exactly as in each mockup) ·
flexible gap · right segments (bg hl): counters (`● 9  ‖ 1  ✗ 1  ○ 4` in state colours), `[+] modified` (yellow),
`history 60s`, `draft autosaved`, and so on, as in each mockup. Then an optional extra pill (for example `3 marked`,
bg magenta).
Message line: see §6.4. It is empty otherwise.

### 9.3 01 Domains dashboard (`Main.dc.html`) — Phase 3
- Body columns: `Length(101)` left, `Min(0)` right.
- Left column: **Domains** `Min(20)`, **Events** `Length(6)` (4 lines), **Host · forge** `Length(5)` (3 gauge rows × 2 columns).
  - Domains panel: line 1 is the filter row: `/ filter…` (comment; becomes a text input with `/`), right-aligned chips
    `all 15` (active) `● running 9` `‖ paused 1` `✗ crashed 1` `○ off 4` (glyph coloured). Chips are clickable with
    `f` cycling them (and `F` back). Line 2 is the header. Then the rows.
    Column widths: `1,1,2,17,9,4,10,Min(22),9,2` with header `"", "", "", NAME, STATE ▾, CPU, MEMORY, CPU%, UPTIME, A`.
    The `▾` follows the sort column. Footers: left `sort: state › name`, right `2/15 · 3 marked`.
    Title right: `15 total`.
  - Events rows: `HH:MM:SS  ` (comment) `glyph ` (event colour) `name(padEnd 16)` `event(padEnd 11)` (event colour)
    `detail` (comment). Title right `libvirt event stream`.
  - Host rows: label padEnd 6 (comment) + LineGauge width 22 + value. Order (row-major across the 2 columns):
    CPU, Pool, RAM, NVMe, Swap, Huge (in real mode, use the two largest pools by name). Title right `<cpu model> · <threads>t · <mem> GiB`.
- Right column (selected domain): **Overview** `Length(9)`, **CPU** `Length(11)`, **Memory | Disk I/O**
  `Length(8)` split 50/50 with gap 1, **Network · <iface>** `Min(9)`.
  - Overview: title = domain name, title right `● running` in the state colour. A 2-column key/value grid (keys comment, padded
    `OS       `, `Machine  `, `Firmware `, `vCPU     `, `Disks    ` / `Uptime  `, `IP      `, `Display `, `Agent   `, `Auto    `),
    a blank line, then chips `S shutdown  r reboot  p pause  c console  v viewer  ␣s snapshot  e edit xml`
    (chips adapt to state: for shut off, `s start  e edit xml  ␣n clone …`).
  - CPU: title right `NN% · N vCPU · 60s` (NN blue bold). 8 braille rows (width = inner − 5), gradient
    `red, magenta, magenta, blue, blue, cyan, cyan, teal`, gutter labels, x-axis row.
  - Memory: title right `used/max GiB` (used magenta). LineGauge (magenta) + ` NN%`. 3 SparkRows magenta. Then
    `rss X · swap-in Y · balloon` (comment).
  - Disk I/O · <first disk>: title right `N IOPS`. `read  ` (green) + value, 2 SparkRows green, `write ` (orange) + value, 2 SparkRows orange.
  - Network: title right `↓ X Mb/s` (cyan) `  ↑ Y Mb/s` (yellow). MirroredBraille 4+3 rows, then
    `total ↓ … ↑ … · drops 0 · errs 0 · bridge virbr0` (comment).
  - For a non-running domain, replace the CPU/Mem/Disk/Net panels with one panel "Not running" that shows the configuration
    summary and a large hint `s  start` (key style).
- `⏎` opens the domain detail. Selection persists across refreshes by UUID.

### 9.4 02 Host monitor (`Host.dc.html`) — Phase 5
- Row A `Length(12)`: **CPU · <model>** (≈59%) and **Memory · <total> GiB** (≈41%).
  CPU panel: title right `NN% · F GHz · T°C · load a b c` (NN blue bold, temp yellow). 6 braille rows (gradient
  `red, magenta, blue, blue, cyan, teal`), labels on rows 0/3/5. Then `per-thread ───…` (comment) and ThreadMeters
  (fit as many threads as the width allows; if they do not all fit, show the busiest N plus `+K` in comment).
  Memory panel: title right `X used · Y free` (X magenta bold), `host RSS by domain` (comment), StackedBar (width
  = inner), legend (up to 8 entries + `other (n)`), and `hugepages 1G a/b · KSM xG · swap a/bG`.
- Row B `Min(0)`: **Running domains** (focused). Columns `1,16,5,6,32,10,18,22,Min` with headers
  `NAME vCPU CPU ▾ CPU · 60s MEM MEM · 60s DISK  R / W NET  ↓ / ↑`. CPU sparkline coloured by level. MEM
  sparkline magenta. R green, W orange, ↓ cyan, ↑ yellow. Title right `N running · V vCPU on T threads · overcommit X×`.
  Footer `⏎ open · o sort · f filter`.
- Row C `Length(7)`: **System**, **Storage pools** (`● name(padEnd 10) gauge(14) value`), **Networks**
  (`● name mode sparkline(16)`), in 3 equal columns.
- Message line: `-- sampling virConnectGetAllDomainStats every 1000 ms --` (comment). In virsh mode, write
  `-- sampling virsh domstats every 1000 ms --`.

### 9.5 03 Domain detail (`Detail.dc.html`) — Phases 6–7
- Under the top bar, a 1-row header: `domains ›` (comment) `name` (bold) `● running` `3d 04h` (comment), then the inner
  tabs `Overview Monitor Hardware Snapshots Console XML` (active: bg sel, fg fg, bold, padded 1; others comment), then
  right-aligned `H/L switch tab  ⌫ back`. Then 1 blank row.
- **Overview tab:** the dashboard's Overview panel enlarged (all config details: OS, machine, firmware, CPU, memory,
  all disks with sizes, NICs with MAC/IP/source, graphics, autostart, persistent, managed save, description/title
  metadata). It is editable via `␣r` rename and `:desc`.
- **Monitor tab:** a 2×2 grid of the dashboard's CPU, Memory, Disk, and Network charts at full height, plus a per-vCPU row
  (vcpu.N.time deltas from domstats) drawn as ThreadMeters, and per-disk/per-NIC breakdown tables.
- **Hardware tab:** columns `Length(38)` Devices, `Min(0)` editor, `Length(60)` right stack.
  - Devices list: `icon(2) name summary(right, comment)`. Icon colours: CPU blue, memory magenta, disk green,
    CDROM yellow, NIC cyan, others fg2. Selected row bg hl + bold (unfocused) or sel (focused). The last line has a separator
    plus `+ add hardware  a`. Footer `d remove · J/K reorder boot`.
  - Editor: one form per device type (§1 Hardware list). Title = device name, title right `● N pending` (yellow)
    when there are changes. Sections and fields exactly as in the mockup for CPUs.
  - Right stack: **Pending · XML diff** (`Min`) and **Will run** (`Length(12)`) with chips `:w apply` (bg green,
    fg bg2), `u undo field`, `:q! discard`, `gx open XML`. Title right `y copy`.
  - Changes accumulate per domain until `:w`. Leaving with pending changes asks for confirmation (`:q!` discards).
- **Snapshots tab:** §9.9. **Console tab:** a panel describing serial/graphical access with chips `c serial console`,
  `v open viewer`, `C-]` hint, and graphics connection info (`spice://127.0.0.1:5901`). **XML tab:** XmlView of the
  live XML; `e` edits the inactive XML in `$EDITOR`, then validates and defines, and on error reopens the editor with the error as
  an XML comment at the top (like `virsh edit`).

### 9.6 04 New domain wizard (`Wizard.dc.html`) — Phase 11
- Modal `Rect` 136×36 at the reference size (min 100×30, centered), border blue, title `＋ New domain`, title right
  `step N / 6`.
- Inside: columns `Length(32)` step list | `Min` form, then **Equivalent command** panel `Length(10)` (bg bg2), then a
  1-line key hint row with a right-aligned primary chip `C-n <Next step> ›` (bg blue).
- Step list: `✓`/`●`/`○` + `N  Title`, with the summary below (comment). The current step has bg sel. Below that,
  `osinfo · <id>`, `min …`, `rec …`, and `✓ meets recommended` (green) or `⚠ below minimum` (yellow).
- Steps and fields (build all six):
  1. **Name & OS:** name (validated: unique, `[A-Za-z0-9._-]`), OS variant (fuzzy select from `--osinfo list`,
     auto-detected from the ISO name when possible), title/description (optional).
  2. **Install media:** source kind radio — local ISO (picker from pools/filesystem), URL (`--location`),
     PXE (`--pxe`), import existing disk (`--import`), none (`--boot uefi` and an empty disk). Optional kernel args for `--location`.
  3. **CPU & Memory:** exactly the mockup (vCPU slider + field, maximum, topology auto/manual, CPU model dropdown, memory
     slider + presets `2G 4G 8G 16G 32G`, host free, maximum, options balloon / hugepages / shared).
  4. **Storage:** create new (pool select, size, format qcow2/raw, bus virtio/sata/scsi, cache) or use an existing
     volume. Add extra disks. Show free space in the pool.
  5. **Network:** source (virtual network list / bridge / macvtap direct / none), model (virtio/e1000e), MAC (auto
     or custom). Add more NICs.
  6. **Review & create:** a read-only summary of everything, firmware (BIOS/UEFI, secure boot, TPM 2.0 checkbox),
     graphics (SPICE/VNC/none), `[x] start after creation` `[x] open console` `[ ] autostart`, and the full command.
     `⏎` runs it, with the message line spinner `Creating fedora-42-ws…`.
- The command is `virt-install` with `--noautoconsole` (plus `--print-xml` for `C-e edit as XML`, which opens the generated
  XML in `$EDITOR` and then `virsh define`s it). Lines that belong to the current step have bg hl and the first gets
  `◂ this step` (blue) on the right.
- The draft is autosaved to `~/.local/state/virsh-tui/wizard-draft.toml` and restored on the next `␣n` (with `C-u` to reset).

### 9.7 05 Command palette (`Palette.dc.html`) — Phase 12
- Modal 148×34 at the top of the screen (row 3), centered horizontally, border blue, title `Command palette`, title right
  `context: <selected object>`.
- Input row on bg bg2: `❯` (magenta bold) + query (bold) + block cursor, right-aligned `shown / total` (comment).
  Prefixes switch source: `❯` actions (default), `:` ex commands, `/` domains (jump), `#` docs (virsh help search).
- Columns `Length(66)` results | `Min` reference, separated by a vertical `│` in hl, with a top separator in hl.
- Results: `icon category:match…rest` + key, as in the mock. Selected row bg sel bold. Then `recent` (comment)
  and the 3 most recent actions (`↺ ` comment).
- The action set is the curated app actions (each with a form) **plus every virsh command** from `virsh help`
  (285). Raw commands open a **generic form** built from `HelpDoc.options` (bool → checkbox, `<string>/<number>` →
  field, the required `[--domain] <string>` pre-filled from context). This is the "never search for the right flag"
  feature.
- Reference pane: `virsh <cmd>` (green bold) + `  — <summary>` (comment), `SYNOPSIS` (blue bold), the synopsis
  wrapped with flags in cyan, `OPTIONS` (blue bold) rows `[x]/[ ] --flag  description` (`⇥` toggles the option under
  the cursor in the pane; `C-l` focuses the pane), and a command preview box (bg bg2) updated live.
- Footer on bg bg2: `⏎ open form  C-j/k move  ⇥ toggle flag  C-y copy cmd  ⎋ close` and right-aligned prefix legend.

### 9.8 06 Help overlay (`Help.dc.html`) — Phase 12
- Modal 156×37, border blue, title `? Keybindings`, title right `/ search · ⎋ close`. 4 equal columns of sections.
  The section title is bold in its section colour with an hl underline. Rows: key (orange bold, width 11) + description (fg2;
  red for confirm-required). Footer legend as in the mock. `/` filters rows live.

### 9.9 09 Snapshots (`Snapshots.dc.html`) — Phase 8
- Domain detail, Snapshots tab: columns `Length(92)` tree | `Min` details.
- Tree rows: `prefix glyph name [← current]` | date `Length(12)` | state `Length(9)` | kind `Length(10)`. Footer
  `␣sc new · ␣sr revert · ␣sd delete · e edit`. Title right `N snapshots · internal|external`.
- Details panel: title = snapshot name; key/value lines as in the mock, then `description` (comment) and the text.
- **Create** form (`␣sc`): name (default `snap-YYYY-MM-DD-HHMM`), description, and the flags from §9.7, with the command preview.
- **Revert**: the CONFIRM modal exactly as in the mock (red border, title `⚠ Revert snapshot`, title right
  `snapshot-revert`, question line with domain blue and snapshot magenta, consequence text, radios
  `running / paused / as saved` → `--running` / `--paused` / none, `[ ] --force`, `[x] snapshot current state first`,
  command box, chips `n cancel` and `y revert` (bg red)).
- **Delete** (`␣sd`): CONFIRM with `[ ] --children`, `[ ] --children-only`, `[ ] --metadata`.

### 9.10 07 Networks (`Networks.dc.html`) — Phase 9
- Columns `Length(72)` | `Min`. Left: **Virtual networks** `Length(10)` (cols `1,2,13,9,8,Min,2`), **Host interfaces** `Min`
  (cols `2,10,8,8,Min`, then `─ attached to <net> ──` and two columns of `vnetN domain`).
- Right: top row `Length(10)`: **<network>** detail (title right `● active · autostart · persistent`) and **Traffic · <bridge>**
  `Length(42)` (MirroredBraille 3+2). Bottom **DHCP leases** (cols `2,18,16,16,19,Min`, legend, action chips, footer
  `net-update · live + config`).
- Actions: `s` start, `D` destroy (confirm), `a` autostart, `e` edit XML, `n` new network form (name, mode
  nat/isolated/route/bridge/open, bridge name auto, IPv4 CIDR, DHCP range auto-computed, DNS domain, IPv6 optional →
  generated XML → `net-define` + `net-start` + optional `net-autostart`), `X` undefine (confirm), `␣l` pin lease
  (`net-update <net> add ip-dhcp-host "<host mac='…' name='…' ip='…'/>" --live --config`), `␣L` remove static,
  `⏎` on a lease jumps to the domain.

### 9.11 08 Storage (`Storage.dc.html`) — Phase 10
- Columns `Length(60)` pools | `Min` volumes. Pool rows are 2 lines each (`● name  type · path` / gauge(34) value), and the
  selected pool has bg hl. Volume cols `1,24,7,9,9,13,Min` with the USED BY rules from §4.2.
- Actions: pool `n` (define-as form: name, type dir/logical/netfs/zfs/iscsi/disk, target/source fields per type),
  `b` build, `s` start, `D` destroy, `r` refresh, `a` autostart, `X` undefine / delete (confirm). Volume `n`
  (vol-create-as: name, capacity, format, allocation, backing volume), `R` resize (vol-resize; if attached to a
  running domain, use `blockresize` instead), `C` clone (vol-clone), `u` upload (`vol-upload` from a local path),
  `W` wipe (confirm; algorithm select), `X` delete (confirm, warns if used).
- **Insert media picker** (`␣mi` from any view; also `⏎` on an ISO volume): modal 110×27 as in the mock. Title
  `◎ Insert media`, title right `<domain> › <target> (cdrom, <bus>)` with the target in yellow. If the domain has no
  CDROM, the picker offers to attach one (`attach-disk <d> <iso> sdX --type cdrom --mode readonly --config [--live]`).
  `C-e` ejects only (`change-media <d> <target> --eject --live --config`). `⇥` cycles the pool, and "browse filesystem…" opens a
  path picker. The background view is dimmed (mode PICKER, magenta pill).

### 9.12 05 Events view (tab `5`; not mocked)
Full-height panel **Events** with a table `TIME(19) TYPE(10) OBJECT(18) EVENT(12) DETAIL(Min)`, using the same
colours as the dashboard event rows. Filter chips at the top (`all`, `lifecycle`, `devices`, `jobs`, `network`,
`storage`, `app`). `/` searches. Keeps the last 5,000 events in memory. `⏎` jumps to the object.

### 9.13 10 Settings (`Settings.dc.html`) — Phase 13
- Opened with `,`. It replaces the body (not a modal). Columns `Length(29)` categories | `Length(50)` list | `Min` options/preview.
- Theme list rows: `❯` (blue, selected) + name + 8 swatches (`██` each in the theme's blue, cyan, magenta, green,
  yellow, orange, red, teal). Moving with `j/k` **live-previews** the whole app. `⏎` applies and saves, and `⎋` reverts to
  the theme from before opening.
- Appearance options exactly as in the mock. Preview panel: 16 swatches (2 rows × 8 colour blocks with token names
  in comment below; render each swatch as `█` × 8 in the token colour), sample rows, gauge, and a 5-row braille chart.
- Other categories (forms built with the same widgets):
  - **General:** default URI, refresh interval, confirm style, editor command, start view.
  - **Keybindings:** a read-only list of the current bindings, with the path of `keymap.toml`.
  - **Connections:** saved URIs list (`qemu:///system`, `qemu:///session`, `qemu+ssh://user@host/system`), set default.
  - **Monitoring:** history windows, balloon stats period (on/off + seconds), sampling of the per-thread host CPU.
  - **Confirmations:** per-action toggles (destroy, reset, undefine, revert, delete volume, wipe), type-name
    requirement for undefine.
  - **Console & viewer:** viewer program (`virt-viewer`/`remote-viewer`), console escape hint.

### 9.14 Responsive behaviour (every screen)
- At ≥ 160 columns: the layouts above.
- From 100 to 159 columns: the dashboard right column becomes a tabbed panel below the domains table
  (`Overview | CPU | Mem | Disk | Net`, switch with `[`/`]`). The Host view stacks row A vertically. Modals shrink to
  min(width − 4, preferred).
- Below 100×30: a centered panel "Terminal too small — W×H, need 100×30".
- Write snapshot tests at 174×43 (reference) and 120×36 (compact) for every screen.

---

## 10. Demo mode (`--demo`)

- Uses `DemoBackend` with fixtures transcribed **exactly** from the mockup scripts: 15 domains (names, states, vCPU,
  memory strings, CPU fractions, uptimes, autostart, marks for the 3 k8s nodes), arch-dev config (OS Arch Linux,
  pc-q35-10.1, UEFI/OVMF, SB off, 8 vCPU host-passthrough, 16 GiB, vda 120G qcow2 + vdb 500G + sda ISO, vnet3,
  192.168.122.48, 52:54:00:a3:1f:7c, SPICE :5901, qemu-ga connected, autostart), 4 events, host (forge,
  Ryzen 9 7950X, 32t, 64 GiB, …), networks, interfaces, leases, pools, volumes, ISOs, snapshot tree, and so on.
- Metrics are generated with a port of the mock's `rng`/`series` (xorshift32 seeded random walk) so the charts look the
  same. Each tick advances the walk.
- `--demo --frozen` (used by tests): freezes the clock at `14:32:07` and pre-fills histories from the seeds used in the
  mockups (CPU seed 7, base 0.42, amp 0.22, pull 0.1; memory seed 11; read 21; write 23; rx 31; tx 37; host CPU 5; …).
  Charts must then render the same glyphs as the mockups.
- Initial selection: `arch-dev` (row 2), marks on the k8s nodes, message line `✓ Started k8s-worker-02  ── virsh start k8s-worker-02`.
- `--screen <name>` opens straight into a screen state that matches a mockup:
  `dashboard`, `host`, `hardware` (INSERT on max vCPUs with the pending diff), `wizard` (step 3), `palette` (query
  "snap"), `help`, `networks`, `storage` (media picker open), `snapshots` (revert confirm open), `settings`, `gallery`.
- `--dump <file>` together with `--size 174x43`: render one frame to an ANSI file and exit (truecolor escape codes).
  This lets you (the agent) and the user check visuals with `cat file`, without an interactive terminal.

---

## 11. Testing strategy

1. **Unit tests:** parsers (against `tests/fixtures/virsh/*`), metric maths, ring buffers, key engine (counts,
   sequences, leader timeout, `.` repeat), ex parser, command builders (assert exact argv **and** display string), XML
   edit ops (round-trip and expected diff), theme loading, and layout functions.
2. **Snapshot tests (insta):** for each `--screen` state, render with `TestBackend` at 174×43 and 120×36 in
   `--demo --frozen` mode and snapshot the buffer **text**. Also snapshot a compact **style map**: for each cell, a
   letter for its fg token and bg token, so colours are regression-tested. Implement a reverse lookup from `Color` to
   token name.
3. **Fidelity review per screen (manual, by you):** after the snapshot exists, compare it line by line with the
   corresponding mockup file. Check every string, column order, glyph, and colour class. Record the result in
   `docs/FIDELITY.md` as a checklist per screen (`[x] title`, `[x] footers`, `[x] chips`, …). Any intentional
   deviation must be listed with its reason (for example "mock row height 26 px → 1 row").
4. **Integration tests (opt-in):** `cargo test --features it -- --test-threads=1`. They run against
   `qemu:///session`. They create `vt-test-*` objects (a tiny domain with no disk, `--boot hd` and 64 MiB memory), exercise
   define/start/pause/resume/destroy/snapshot/undefine, and clean up in `Drop`. They never touch other objects.
5. **Manual smoke test** on the real system (read-only): `cargo run -- -c qemu:///system` must list the 3 real
   domains and show host metrics without errors.

---

## 12. Phases and acceptance criteria

Every phase ends with the quality gates (§0.2), updated snapshots, `docs/FIDELITY.md` entries for the screens it
touched, and a commit.

**P0 — Scaffolding**
- `git init`, `.gitignore`, `cargo init --name virsh-tui`, add the dependencies, `rustfmt.toml` (max_width 110),
  clippy config, `README.md` stub, and `LICENSE` (ask the user which license; default to MIT if there is no answer).
- `main.rs`: clap CLI (§3.3), color-eyre, a tracing file logger (`~/.local/state/virsh-tui/virsh-tui.log`), terminal
  init/restore, panic hook, signal handling, and the main loop with tick and input.
- Theme module with the 9 themes and style helpers. Config load/save with defaults.
- Chrome: top bar, status bar, and message line with an empty body.
- ✅ `cargo run -- --demo` shows the chrome in tokyo-night; `q`/`ZZ` quits cleanly; snapshot `chrome_174x43`.

**P1 — Widget library**
- All widgets in §7, plus the `gallery` screen that shows each one with demo data.
- ✅ Snapshot per widget. The braille output for `braille(series(7,90,0.42,0.22,0.1),60,8,Fill)` equals the
  mock's JavaScript output. `node` is installed (`/usr/bin/node`): extract the helper functions from
  `docs/mockups/Main.dc.html` into `tests/fixtures/js/charts.js`, generate the expected strings with node, and
  commit them as fixtures. Do the same for `sparkRows`, `hbar`, and `lg`.

**P2 — Data layer**
- Models, Backend trait, the VirshBackend read paths in §4.2, the events stream, the metrics sampler, DemoBackend +
  fixtures + sim, and parser fixtures captured from this machine.
- ✅ The parser tests pass. A headless `virsh-tui --demo --dump` works. Against the real system, a debug command
  `virsh-tui --probe` prints the parsed domains, host info, networks, and pools as JSON (read-only), with no errors.

**P3 — Dashboard**
- Screen 01, pixel-for-pixel per §9.3: selection, filter (`/`), state chips (`f`), sort (`o` cycles name/state/cpu/mem/uptime),
  marks (`m`, `V`), detail panels following the selection, and the not-running variant.
- ✅ The `dashboard` snapshot matches the mockup content. FIDELITY.md section 01 is complete. It works live against the real system.

**P4 — Input system and lifecycle actions**
- Key engine, modes, counts, `.` repeat, leader + which-key, ex line with completion, keymap.toml, CONFIRM modal
  (y/n and type-the-name variants), CommandPlan/display/execution, `--dry-run`, message line results,
  history/`yc`/`:messages`.
- Lifecycle: start, shutdown, destroy, reboot, reset, pause/resume, managed-save (+ remove), autostart, rename, undefine
  (options form), clone (`virt-clone --original d --name n --auto-clone`), console (suspend TUI), viewer (detached
  spawn), send-key Ctrl-Alt-Del, edit XML via `$EDITOR`, export XML (`␣x` → save to `./<name>.xml`), and bulk on marks.
- ✅ Unit tests for every builder. Integration test lifecycle on `vt-test-*`. Dry-run shows the correct commands.

**P5 — Host monitor**
- Screen 02 per §9.4, with local and remote data paths.
- ✅ Snapshot matches. It shows real host data on this machine.

**P6 — Domain detail shell**
- Header + inner tabs, Overview, Monitor, Console, and XML (view, folding, `e` edit with validation and the reopen-on-error loop).
- ✅ Snapshots for each tab. Editing XML on a `vt-test-*` domain round-trips.

**P7 — Hardware editor**
- Form engine (fields, validation, INSERT mode, pending changes, undo per field), XML edit ops, diff panel, and the "Will
  run" panel. Editors for every device type in §1, including attach/detach of disks and NICs, hostdev passthrough
  (`nodedev-list --cap pci|usb_device` picker), and boot order (`J/K`).
- Prefer native virsh commands when they exist (`setvcpus`, `setmem`, `setmaxmem`, `vcpupin`, `emulatorpin`,
  `iothreadadd`, `attach-disk`, `detach-disk`, `attach-interface`, `detach-interface`, `domif-setlink`,
  `domiftune`, `update-device`, `change-media`, `blockresize`). Use the define path only for what has no command.
- ✅ The `hardware` snapshot matches the mockup (INSERT on max vCPUs, topology auto-adjusted, diff, commands).
  Integration test on `vt-test-*` for vCPU and memory changes.

**P8 — Snapshots**
- §9.9 complete (tree, details, create form, revert confirm with safety snapshot, delete, edit XML).
- ✅ The `snapshots` snapshot matches the mockup with the confirm modal open. Integration test for create/revert/delete.

**P9 — Networks**
- §9.10 complete, including the new-network form and lease pinning.
- ✅ Snapshot matches. Integration test creating a `vt-test-net` isolated network (on `qemu:///session` if possible;
  otherwise only with `--dry-run`, and report this in FIDELITY.md).

**P10 — Storage + media**
- §9.11 complete, including the picker from any view and the attach-cdrom fallback.
- ✅ The `storage` snapshot (picker open) matches. Integration test for vol-create/resize/clone/delete in a `vt-test-pool` (dir pool in a tempdir).

**P11 — Wizard**
- §9.6, all 6 steps, draft autosave, `--print-xml` edit path.
- ✅ The `wizard` snapshot at step 3 matches. Integration test: create a `vt-test-vm` via the wizard model (no ISO,
  `--boot hd --disk none`, 64 MiB), then undefine it.

**P12 — Palette, help, events**
- §9.7 (including the generic raw-command form for all 285 commands), §9.8 (generated from the keymap), and §9.12.
- ✅ `palette` and `help` snapshots match. Every `virsh help` entry parses without error (test iterates the cached index).

**P13 — Settings and themes**
- §9.13, live theme preview, custom theme loading, all option categories wired to behaviour, and persistence.
- ✅ `settings` snapshot matches. Switching each of the 9 themes renders without missing tokens (snapshot of the dashboard
  style map per theme).

**P14 — Polish and release**
- Responsive layouts and the too-small screen (§9.14), 256-colour fallback, performance (a frame renders in < 4 ms at 174×43
  in release; the 1 s stats cycle uses < 2% CPU with 15 demo domains; measure and record in README), and remote URI
  (`qemu+ssh://`) sanity.
- Error UX: virsh missing, libvirtd down, permission denied (`not in libvirt group` hint), and lost connection (auto-reconnect
  with backoff; the top bar shows `⌁ disconnected` in red).
- README: features, screenshots (generated from `--dump` and converted with `aha` or a similar tool if available,
  otherwise ANSI files), install (`cargo install --path .`), config and keymap reference, and the theme format.
- ✅ Every FIDELITY.md section is checked. All quality gates pass. `cargo build --release` produces a working binary.

---

## 13. Appendix A — feature → command matrix

| Feature | Command(s) |
|---|---|
| Start / Shutdown / Destroy | `virsh start d` · `virsh shutdown d [--mode acpi\|agent]` · `virsh destroy d [--graceful]` |
| Reboot / Reset | `virsh reboot d` · `virsh reset d` |
| Pause / Resume | `virsh suspend d` · `virsh resume d` |
| Managed save | `virsh managedsave d [--running\|--paused]` · `virsh managedsave-remove d` |
| PM suspend | `virsh dompmsuspend d mem\|disk\|hybrid` · `virsh dompmwakeup d` |
| Autostart | `virsh autostart d [--disable]` |
| Rename | `virsh domrename d new` (shut off only; the UI enforces this) |
| Clone | `virt-clone --original d --name new --auto-clone [--file …]` |
| Undefine | `virsh undefine d [--remove-all-storage] [--nvram] [--snapshots-metadata] [--managed-save] [--checkpoints-metadata] [--tpm]` |
| Define / Edit | `virsh define file [--validate]` · `$EDITOR` + define |
| Export XML | `virsh dumpxml d [--inactive] [--security-info]` |
| Console / Viewer | `virsh console d` · `virt-viewer -c uri d` / `remote-viewer spice://…` |
| Send keys | `virsh send-key d KEY_LEFTCTRL KEY_LEFTALT KEY_DELETE` |
| Migrate | `virsh migrate d qemu+ssh://host/system --live --persistent [--undefinesource] [--copy-storage-all] [--verbose]` |
| vCPUs | `virsh setvcpus d N [--maximum] --live --config` · `virsh vcpupin d v cpulist --config [--live]` · `virsh emulatorpin d cpulist` · `virsh iothreadadd d id` |
| Memory | `virsh setmaxmem d SIZE --config` · `virsh setmem d SIZE --live --config` · `virsh dommemstat d --period N --live` |
| Disk | `virsh attach-disk d src target [--driver qemu --subdriver qcow2] [--targetbus virtio] [--cache none] --config [--live]` · `virsh detach-disk d target --config [--live]` · `virsh blockresize d target SIZE` |
| CDROM | `virsh change-media d target iso --update --live --config` · `--eject` · `--insert` |
| NIC | `virsh attach-interface d network default --model virtio [--mac …] --config [--live]` · `virsh detach-interface d network --mac … --config [--live]` · `virsh domif-setlink d iface up\|down` · `virsh domiftune d iface --inbound … --outbound …` |
| Generic device | `virsh attach-device d file.xml --config [--live]` · `detach-device` · `update-device` |
| Host devices | `virsh nodedev-list --cap pci\|usb_device` · `virsh nodedev-dumpxml dev` · attach-device with `<hostdev>` |
| CPU model / topology / features / firmware / boot order / TPM / graphics | XML edit path → `virsh define` |
| Snapshots | `snapshot-create-as` · `snapshot-revert d s [--running\|--paused] [--force]` · `snapshot-delete d s [--children\|--children-only\|--metadata]` · `snapshot-edit` · `snapshot-list d --parent` · `snapshot-current d --name` |
| Networks | `net-define file` · `net-start` · `net-destroy` · `net-undefine` · `net-autostart [--disable]` · `net-edit` (editor path) · `net-dhcp-leases` · `net-update net add\|delete ip-dhcp-host "<host …/>" --live --config` |
| Pools | `pool-define-as name type [--target …] [--source-host … --source-path …]` · `pool-build` · `pool-start` · `pool-destroy` · `pool-refresh` · `pool-autostart` · `pool-undefine` · `pool-delete` |
| Volumes | `vol-create-as pool name SIZE --format qcow2 [--backing-vol … --backing-vol-format …]` · `vol-clone` · `vol-resize [--shrink]` · `vol-upload` · `vol-download` · `vol-wipe [--algorithm …]` · `vol-delete` |
| Install | `virt-install --name … --osinfo … (--cdrom\|--location\|--pxe\|--import) --vcpus N,maxvcpus=M[,sockets=,cores=,threads=] --cpu MODEL --memory N,maxmemory=M --disk … --network … --boot uefi --graphics spice --noautoconsole [--tpm default] [--autostart]` |
| Connection | `virsh -c URI …` on every call. `:connect` swaps the URI, re-runs all fetches, and restarts the event streams. |

**Destructive list (always confirm):** destroy, reset, undefine, snapshot-revert, snapshot-delete, net-destroy,
net-undefine, pool-destroy, pool-delete, pool-undefine, vol-delete, vol-wipe, detach-*, and migrate with `--undefinesource`.

---

## 14. Appendix B — fidelity checklist template (`docs/FIDELITY.md`)

For each screen:
```
## 01 Domains dashboard  (mockup: docs/mockups/Main.dc.html)
- [ ] Top bar: logo pill, 5 tabs, active tab style, right segment text and separators
- [ ] Panel set & order, titles, right titles, footers (exact strings)
- [ ] Column order, widths, header labels (incl. ▾ sort marker)
- [ ] Row glyphs & colours per state; selection marker; marks
- [ ] Chips (text, active style)
- [ ] Charts: type, size (rows×cols), colours/gradient, labels, axis
- [ ] Gauges: width, colours by level, value text
- [ ] Status bar: mode pill, breadcrumb, hints (exact), right segments, extra pill
- [ ] Message line text
- [ ] Theme switch: all 9 themes render with no hard-coded colours
- [ ] Deviations (with reason):
```

---

## 15. Working agreements for the agent

- Before each phase, re-read the relevant DESIGN.md section and mockup file.
- Prefer small, composable widgets. Views only arrange widgets and map state to them.
- Do not add features or settings beyond this plan without noting them in `docs/DECISIONS.md` with a rationale.
- If a libvirt/virsh behaviour differs from what this plan assumes (output formats, flags), trust the real
  output, adapt, and record it in `docs/DECISIONS.md`.
- If you are blocked by something that needs the user (root, a licence choice, a missing package such as `node` for
  cross-checking), ask a concise question and continue with other phases meanwhile.
