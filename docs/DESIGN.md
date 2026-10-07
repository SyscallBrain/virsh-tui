# virsh-tui — Design Spec

Mockup sources (exact strings, data, colours): `docs/mockups/*.dc.html`. Implementation plan: `docs/PLAN.md`.

This document is the source of truth for the UI. Every screen on the canvas maps to a section below.
Stack: Rust + ratatui (+ crossterm). Every effect in the mockups is achievable with per-cell fg/bg/modifiers —
nothing relies on pixel tricks.

---

## 1. Principles

1. **virsh-transparent.** Every action shows the exact `virsh` / `virt-install` command it is about to run
   (preview before, echo after in the message line). The app teaches virsh while removing the need to memorise it.
2. **Vim-native.** Modal (NORMAL / INSERT / VISUAL / COMMAND / PALETTE / PICKER / CONFIRM), counts, `.` repeat,
   `:` ex-commands, `␣` leader with which-key style sub-menus, `gg/G`, `C-d/C-u`, `/` filter, `n/N`.
3. **Dense but calm.** One accent (blue) for focus; colour carries meaning (state, metric type), never decoration.
4. **Nothing destructive without a preview.** Destroy / undefine / revert / delete always open a CONFIRM modal with
   the command list; undefine requires typing the domain name.
5. **Live.** 1 s default refresh via `virConnectGetAllDomainStats`; libvirt event stream for state changes (no polling for state).

---

## 2. Theme tokens

Themes are a flat set of 16 semantic tokens. All widgets use tokens only — never raw colours.

| token     | role                                              | tokyo-night |
|-----------|---------------------------------------------------|-------------|
| `bg`      | main background                                   | `#1a1b26` |
| `bg2`     | top bar, status bar, code blocks, inputs          | `#16161e` |
| `hl`      | chips, field background, unfocused selection, gauge track | `#292e42` |
| `sel`     | focused selection row (visual)                    | `#283457` |
| `gutter`  | unfocused borders, dashed separators              | `#3b4261` |
| `comment` | labels, hints, dim text, `shut off`               | `#565f89` |
| `fg`      | primary text                                      | `#c0caf5` |
| `fg2`     | secondary text, values                            | `#a9b1d6` |
| `blue`    | focus border, accent, NORMAL mode, active tab     | `#7aa2f7` |
| `cyan`    | network / IPs / flags in commands / net rx        | `#7dcfff` |
| `magenta` | memory, snapshots, marks, PALETTE/PICKER mode     | `#bb9af7` |
| `green`   | running, read I/O, INSERT mode, success, `[x]`    | `#9ece6a` |
| `yellow`  | paused, warnings, net tx, ISO/media               | `#e0af68` |
| `orange`  | keys in hints, write I/O, numeric args            | `#ff9e64` |
| `red`     | crashed, destructive, CONFIRM mode, CPU hot       | `#f7768e` |
| `teal`    | CPU chart base gradient                           | `#73daca` |

Built-in themes (exact hex values are in every artboard's script, `THEMES` map, key order:
`bg bg2 hl sel gutter comment fg fg2 blue cyan magenta green yellow orange red teal`):

- `tokyo-night` (**default**), `tokyo-night-storm`, `tokyo-night-day` (light)
- `catppuccin-mocha`, `gruvbox-dark`, `nord`, `dracula`, `kanagawa`, `everforest`

Custom themes: `~/.config/virsh-tui/themes/<name>.toml` with the same 16 keys. Option `transparent = true`
renders `bg` as `Color::Reset` (inherit terminal).

---

## 3. Global layout (every screen)

```
┌ row 0  (1 line)  TOP BAR   bg2: [◆ virsh-tui](bg=blue,fg=bg2,bold)  1 Domains  2 Host  3 Networks  4 Storage  5 Events ……  ⌁ uri │ host │ ⟳ 1s │ HH:MM:SS
│ row 1… (fill)    BODY      panels, 1 cell gap, 1 blank line above first row of panels (room for titles)
│ row n-1 (1 line) STATUS    bg2: [MODE pill] [breadcrumb (bg=hl)]  key hints (keys orange, text comment) …… [counters (bg=hl)] [extra pill]
└ row n  (1 line)  MESSAGE   last result: "✓ Started k8s-worker-02  ── virsh start k8s-worker-02" / "-- INSERT --" / errors in red
```

- **Tabs**: inactive `comment`, number in `orange` bold; active = bg `bg`, fg `blue`, bold.
- **Mode pill colours**: NORMAL=blue, INSERT=green, VISUAL=magenta, COMMAND=yellow, PALETTE/PICKER=magenta, CONFIRM=red. Text `bg2`, bold.
- **Message line**: success `green ✓`, error `red ✗`, info `comment`. Always append `── <command>` in `comment` when a virsh call ran.
- Minimum terminal size: 100×30. Below that, show a centered "terminal too small (WxH, need 100×30)" panel.
  Layout breakpoints: ≥160 cols → two-column dashboard (as mock); 100–159 → right column moves under the list as tabs (Overview | CPU | Mem | Disk | Net).

### Panel (the basic container)

- `Block::bordered().border_type(Rounded)` (configurable: plain / double / thick).
- Border `gutter`; **focused** panel border `blue` and title `blue`.
- Title top-left, bold, `fg2` (padded with one space each side). Optional right title (top-right, `comment`),
  bottom-left / bottom-right footers in `comment` (sort mode, counts, local key hints).
- Inner padding: 1 cell horizontal.

### Tables / lists

- Header row: `comment`, bold, underline separator (`hl`).
- Selected row: bg `sel` (focused panel) or `hl` (unfocused), name bold `fg`, plus a `▌` marker in `blue` in col 0.
- Marked rows (`m` / VISUAL): `◆` in `magenta` in col 1; status bar shows `N marked` pill (magenta).
- Row height 1 line (mock uses extra leading only for readability).

### State glyphs (domains)

| state       | glyph | colour   |
|-------------|-------|----------|
| running     | `●`   | green    |
| paused      | `‖`   | yellow   |
| shut off    | `○`   | comment  |
| crashed     | `✗`   | red      |
| pmsuspended | `◌`   | magenta  |
| in transition (starting/shutting down) | spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | blue |

Icon sets: `unicode` (default, as above), `nerd` (nerd-font glyphs), `ascii` (`* = o x ~`).

---

## 4. Charts & gauges (ratatui mapping)

| visual | used in | implementation |
|--------|---------|----------------|
| **Braille area chart with vertical gradient** | CPU (dashboard, host) | Custom widget: 2×4 dots per cell (U+2800 + bitmask), filled from the value down. Colour per *row* from a gradient top→bottom: `red, magenta, magenta, blue, blue, cyan, cyan, teal`. Y labels `100% / 50% / 0%` in a 5-col gutter; x labels `-60s -30s now`. |
| **Mirrored braille** | Network rx/tx | Same widget; rx fills upward (cyan), tx downward (yellow), dashed `gutter` line between. |
| **Multi-row sparkline** | Memory, Disk R/W | ratatui `Sparkline` (or custom multi-row with `▁▂▃▄▅▆▇█`). Memory = magenta, read = green, write = orange. |
| **Inline sparkline** | Host "Running domains" table cells, network list | single-row `▁…█` string, coloured by the current level. |
| **Line gauge** | host CPU/RAM/pools, memory, volume usage | `LineGauge` with `━` filled / `━` in `hl` for the track. Colour by level: `<50% green`, `<80% yellow`, else `red`. |
| **Eighth-block bar** | CPU% column in domain table | `█` + partial `▏▎▍▌▋▊▉`, track `░` in `hl`, followed by right-aligned `NN%`. |
| **Per-thread meters** | Host CPU | 32 columns × 3 rows of vertical blocks, row colours `red / blue / cyan`, index under each (comment). |
| **Stacked bar** | Host memory by domain | full-block segments, one palette colour per domain + legend with `■`. Palette order: orange, blue, magenta, cyan, green, yellow, teal, red, fg2. |
| **Slider** | wizard / forms | `━━━━●──────` — filled in metric colour, knob bold, track `gutter`. |

Graph style option: `braille` (default) | `block` | `tty` (ASCII). Option `gradient = true|false`.
History windows: `t` cycles 60 s → 5 min → 1 h (ring buffers per domain per metric; 1 h kept at 10 s resolution).

---

## 5. Screens

### 01 · Domains dashboard (`1`)
- Left column (≈58%): **Domains** table (focus) — columns: marker, mark, state glyph, NAME, STATE, CPU (vCPUs), MEMORY (used/max), CPU% bar, UPTIME, A (autostart ✓/·).
  Filter bar on top: `/ filter…` + state chips (`all`, `running`, `paused`, `crashed`, `off`) with counts. Footers: `sort: state › name`, `2/15 · 3 marked`.
- **Events** panel: last 4 libvirt events (`time  glyph  domain  event  detail`), coloured by event type.
- **Host** panel: 6 line gauges in 2 columns (CPU, RAM, Swap, pools, hugepages).
- Right column = selected domain: **Overview** (key/value grid + action chips `S r p c v ␣s e`), **CPU** braille chart, **Memory** + **Disk I/O** side by side, **Network** mirrored chart with totals.
- For shut-off domains, the right column shows config only (no charts) plus a big hint `s to start`.

### 02 · Host monitor (`2`)
- **CPU** panel: model, freq, temp, load average; 6-row braille history; per-thread meters.
- **Memory** panel: stacked "host RSS by domain" bar + legend + hugepages / KSM / swap line.
- **Running domains** table (focus): NAME, vCPU, CPU%, CPU 60 s sparkline, MEM, MEM sparkline, DISK R/W, NET ↓/↑. Title-right: total vCPU / threads and overcommit ratio.
- Bottom row: **System** (kernel, libvirt, QEMU, KVM/nested/IOMMU, NUMA, uptime), **Storage pools** gauges, **Networks** with sparklines.

### 03 · Domain detail (`⏎` on a domain)
- Header: breadcrumb `domains › name`, state, uptime, inner tabs **Overview | Monitor | Hardware | Snapshots | Console | XML** (`H/L`).
- **Hardware**: left **Devices** list (icon coloured by device class, name, summary; `a` add, `d` remove, `J/K` reorder boot), center editor form for the selected device, right **Pending · XML diff** (red `-` / green `+`) and **Will run** (command list, `:w` apply, `u` undo field, `:q!` discard, `gx` open XML).
- Forms: sections in `blue` bold; fields bg `hl`; focused field bg `sel` + green outline + block cursor in INSERT; radios `(●)/( )`, checkboxes `[x]/[ ]` (green when on); warnings in yellow with `⚠` (e.g. "needs reboot").
- Every form has an **Apply to** section: `config (--config)` / `live (--live)`.
- Device editors needed: CPUs (count/max, topology, model, features, pinning, emulatorpin, iothreads), Memory (current/max, balloon, hugepages, shared/virtiofs), Boot (firmware, secure boot, order, menu), Disk (source, bus, cache, io, discard, readonly, size → resize), CDROM (media insert/eject), NIC (source network/bridge, model, MAC, link state, bandwidth), Display (SPICE/VNC, listen, port, password), Video, Sound, Input, TPM, USB redirection / host-device passthrough (USB + PCI).
- **XML** tab: read-only highlighted XML with folding; `e` opens `$EDITOR` and validates with `virsh define --validate` on save.
- **Console** tab: instructions + `c` to suspend the TUI and run `virsh console` (Ctrl-] returns), `v` to spawn `virt-viewer`/`remote-viewer` detached.

### 04 · New domain wizard (`␣n`)
- Centered modal over the dimmed dashboard. Steps: **1 Name & OS → 2 Install media → 3 CPU & Memory → 4 Storage → 5 Network → 6 Review & create**.
- Left: step list (`✓` done green, `●` current blue on `sel`, `○` todo) with the chosen value under each step; osinfo min/recommended requirements and a check.
- Right: step form. Sliders + numeric fields; presets as chips (selected chip = magenta bg).
- Bottom: **Equivalent command** (`virt-install …`) updating live, lines belonging to the current step highlighted with `hl`. `y` copy, `C-e` edit as XML.
- Keys: `⇥` next field, `h/l` adjust, `H/L` ×step, `C-n/C-p` next/prev step, `⎋` cancel (draft autosaved).

### 05 · Command palette (`C-p`)
- Modal, top-centered. Input with prefix modes: `❯` actions, `:` ex, `/` domains, `#` docs.
- Left: fuzzy results (match chars orange bold), icon by category, bound key on the right; then **recent**.
- Right: **virsh reference** for the highlighted action — SYNOPSIS + OPTIONS (each flag with a `[ ]`/`[x]` toggle via `⇥`) + the resulting command. This is the "never search for the right flag again" feature: the option list is generated from `virsh help <cmd>` (parse at startup, cache).
- `⏎` opens the action's form pre-filled; `C-y` copies the command.

### 06 · Keybindings (`?`)
- Full-screen modal, 4 columns of sections (see §6). `/` searches. Destructive actions in red. Footer: counts, `.` repeat, keymap path.

### 07 · Networks (`3`)
- **Virtual networks** table (name, mode, bridge, subnet, autostart) and **Host interfaces** (iface, type, speed, address, attached vnets).
- Right: network detail (forward, bridge, IPv4/6, DHCP range, DNS), **Traffic** mirrored chart, **DHCP leases** table (static `◆` magenta / dynamic `●` green) with actions: pin lease as static (`net-update add ip-dhcp-host --live --config`), remove static, copy IP, jump to domain.

### 08 · Storage (`4`)
- **Pools** (state, type, target path, usage gauge). **Volumes** of the selected pool: NAME, FORMAT, CAPACITY, ALLOC, usage gauge, USED BY (domain, `◆ backing of N` magenta, `⚠ orphan` yellow).
- Volume actions: `n` new, `R` resize, `C` clone, `u` upload, `W` wipe, `X` delete (confirm).
- **Insert media picker** (`␣mi`, from any view): fuzzy ISO list from the `isos` pool (or any pool via `⇥`, or filesystem browse), shows current media to be ejected, `live`/`config`/boot-from-cdrom toggles, `virsh change-media … --update --live --config` preview.

### 09 · Snapshots (domain tab)
- Tree with box-drawing (`├─ │ └─`), `◆` snapshot (magenta), `●` current (green, `← current`), date, state at snapshot, kind (disk / mem+disk).
- Detail panel: created, state, parent, children, disks, size, description.
- **CONFIRM modal** (red border, CONFIRM mode): question with names highlighted, consequences in plain words, options (after-revert state, `--force`, "snapshot current state first" ON by default), command list, `n` cancel / `y` revert (red bg).

### 10 · Settings (`,`)
- Categories: General, Appearance, Keybindings, Connections, Monitoring, Confirmations, Console & viewer.
- Theme list with 8-colour swatch strip; `j/k` previews **live** (whole app re-themes), `⏎` applies, `⎋` reverts.
- Appearance options: borders (rounded/plain/double/thick), icons (unicode/nerd/ascii), graphs (braille/block/tty + gradient), transparent background, dim behind modals.
- Preview panel: 16-token palette + sample rows + gauge + chart.
- Persisted to `~/.config/virsh-tui/config.toml`.

---

## 6. Keymap (default — all remappable in `keymap.toml`)

**Navigation**: `j/k` · `gg/G` · `C-d/C-u` · `h/l` focus pane · `H/L` prev/next inner tab · `1…5` views · `⏎` open · `⌫`/`q` back · `/` filter · `n/N` next/prev match.
**Modes**: `i`/`a` edit field (INSERT) · `⎋` NORMAL · `V` visual select · `m` mark · `:` ex · `C-p` palette.
**Domain**: `s` start · `S` shutdown · `D` destroy● · `r` reboot · `R` reset● · `p` pause/resume · `Z` managed-save · `a` autostart · `c` console · `v` viewer · `e` edit XML · `X` undefine●.
**Yank**: `yy` name · `yu` uuid · `yi` ip · `yc` last command.
**Leader `␣`**: `n` new · `c` clone · `r` rename · `M` migrate · `s c/r/d` snapshot new/revert/delete · `mi/me` insert/eject ISO · `da/dr` disk attach/resize · `ia/il` NIC attach/link · `x` export XML · `b` boot order. Pressing `␣` and waiting 300 ms shows a which-key popup bottom-right.
**Monitor**: `t` window 60s/5m/1h · `+/-` refresh interval · `o` cycle sort · `g c/m/d/n` focus graph.
**Ex**: `:start <dom>` · `:setmem <dom> 8G --live` · `:attach-iso <dom> <path>` · `:connect <uri>` · `:theme <name>` · `:sort <col>` · `:w` · `:q` · `:qa` · `:!virsh <args>` (raw passthrough, output in a pager).
**App**: `?` help · `,` settings · `C-r` refresh · `C-l` redraw · `ZZ`/`:qa` quit.
Counts (`3j`) and `.` (repeat last action on the current/marked selection) work everywhere. Actions on marked domains apply to all marks (bulk).
(● = confirmation required)

---

## 7. Typography & glyphs

- Assumes a monospace font with box-drawing and Braille (U+2800–28FF) coverage (JetBrains Mono / DejaVu Sans Mono / any Nerd Font).
- Bold only for: titles, selected names, mode pills, keys in hints, active tab.
- Glyph vocabulary: `◆ ● ○ ‖ ✗ ◌ ▌ ◎ ◫ ▣ ▤ ⏻ ⇄ ▭ ▢ ⌨ ⛨ ⇢ ⌁ ⟳ ❯ ✓ ⚠ ↓ ↑ ━ ─ ░ █ ▁…█ ⠀…⣿`.
