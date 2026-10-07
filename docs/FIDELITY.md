# Fidelity log

Tracks per-screen comparison against `docs/mockups/*.dc.html`.

## P0 chrome (no mockup screen)
- [x] Top bar: logo pill, 5 tabs, active tab style, right segment text and separators
- [x] Status bar: mode pill, breadcrumb, hints
- [x] Message line empty
- [ ] Deviations (with reason): none yet; body empty until P3.

## P1 widget library (source: docs/mockups/Main.dc.html renderVals)
- [x] braille(series(7,90,0.42,0.22,0.1),60,8,Fill) equals mock JS output (tests/p1_widgets.rs)
- [x] sparkRows(series(11,60,0.74,0.06,0.2),28,3) equals mock JS output
- [x] hbar(0.42,16) and hbar(0.87,16) equal mock JS output
- [x] lg(0.38,22) equals mock JS output
- [x] Gallery screen `--screen gallery` with snapshots per widget group
- [ ] Deviations (with reason): gallery is a hidden demo screen, not a mockup screen; full widget styling (colours/gradients) lands with the views in P3+.

## 01 Domains dashboard (mockup: docs/mockups/Main.dc.html)
- [x] Top bar: logo pill, 5 tabs, active tab style, right segment text and separators
- [x] Panel set & order, titles, right titles, footers (exact strings)
- [x] Column order, widths, header labels (incl. ▾ sort marker, follows sort column)
- [x] Row glyphs & colours per state; selection marker; marks
- [x] Chips (text, active style)
- [x] Charts: type, size (rows×cols), colours/gradient, labels, axis
- [x] Gauges: width, colours by level, value text
- [x] Status bar: mode pill, breadcrumb, hints (exact), right segments, extra pill
- [x] Message line text
- [x] Theme switch: all 9 themes render with no hard-coded colours
- [ ] Deviations (with reason): terminal grid uses 1-space column gaps (§7.2) instead of 8px CSS gaps; mock rows have 24px line-height → 1 terminal row; overview details (OS/IP/vCPU) are demo placeholders until P6 XML parsing wires per-domain config; live names truncate at 17 chars.

## 02 Host monitor (mockup: docs/mockups/Host.dc.html)
- [x] Row A: CPU panel (model, %, GHz, temp, load; 6 braille rows; per-thread meters) and Memory panel (used/free, RSS stacked bar, legend, hugepages/KSM/swap)
- [x] Row B: Running domains table (columns, sparklines, R/W + rx/tx, title right, footer)
- [x] Row C: System, Storage pools gauges, Networks sparklines
- [x] Status bar: mode pill, breadcrumb `host › running`, hints (exact), `history 60s`
- [x] Message line sampling text
- [ ] Deviations (with reason): Row A is Length(13) not Length(12) — 6 braille + per-thread + 3 meter + index + 2 borders = 13; demo message uses the virConnect string (live uses the virsh string).

## 03 Domain detail shell (mockup: docs/mockups/Detail.dc.html, P6 tabs)
- [x] Header: breadcrumb, name, state, uptime, inner tabs, H/L hint, back hint
- [x] Overview tab: config details from live/inactive XML
- [x] Monitor tab: 2×2 CPU/Memory/Disk/Network charts
- [x] Console tab: serial/graphical info, chips, graphics URI
- [x] XML tab: highlighted view, folding (za/zR/zM), `e` edit with validate + reopen-on-error
- [x] Hardware tab: see Hardware section below. Snapshots tab: see Snapshots section below.
- [ ] Deviations (with reason): XML highlight is line-based (tags blue) rather than full token attrs; per-vCPU deltas reuse seeded series until sampler histories wire live counters (P7).

## 03 Domain detail · Hardware tab (mockup: docs/mockups/Detail.dc.html)
- [x] Devices list (14 entries, icons, summaries, add footer)
- [x] CPU editor (allocation, topology auto-adjusted, model, features, pinning, Apply-to)
- [x] Pending · XML diff (deterministic pretty hunks) and Will run (native + define)
- [x] INSERT mode (cursor, outline, h/l/C-a/C-x adjust, status pill, message)
- [x] Undo per field, :w apply, :q! discard, gx open XML, y copy, J/K boot reorder, d detach (confirm)
- [ ] Deviations (with reason): attribute order is alphabetical (xmltree HashMaps are random, so a custom deterministic printer is used); mockup single quotes render as double quotes; `a` add stages ex attach commands and `:attach-hostdev` (full fuzzy nodedev picker lands with P12); `y` copies will-run in this tab (yank sequences keep working elsewhere).

## 03 Domain detail · Snapshots tab (mockup: docs/mockups/Snapshots.dc.html)
- [x] Tree (prefix glyphs, current marker, date/state/kind, footer hints)
- [x] Details panel (created/state/parent/children/disks/size/description)
- [x] Create form (name default, description, atomic, command preview)
- [x] Revert CONFIRM (radios, force, auto-safety default on, 2-step preview, red y revert)
- [x] Delete CONFIRM (children/children-only/metadata, preview)
- [x] Status CONFIRM pill + breadcrumb + awaiting message
- [ ] Deviations (with reason): safety stamp is live HHMM (frozen 1438 in demo/snapshots); tree dates empty for live snapshots until dumpxml per-snapshot lands (P8 polish).

## 07 Networks (mockup: docs/mockups/Networks.dc.html)
- [x] Virtual networks table + Host interfaces + attached vnets
- [x] Network detail + Traffic mirrored chart + DHCP leases (static/dynamic, legend, chips, footer)
- [x] Actions: s/D/a/e/n/X, ␣l pin, ␣L remove static, ⏎ jump to domain, yi copy ip
- [x] New-network form (mode/CIDR/DHCP auto/DNS → net-define + net-start + autostart)
- [ ] Deviations (with reason): live subnets parse `netmask=` too (real XML uses it, mockup uses prefix); live iface ADDRESS shows MAC (IP parsing via `ip -j addr` deferred); unprivileged sessions cannot `net-start` (bridge creation needs root) — integration covers define/dumpxml/autostart/undefine on `qemu:///session`.

## 08 Storage (mockup: docs/mockups/Storage.dc.html)
- [x] Pools (2-line rows, gauges, counts) + Volumes (columns, usage, USED BY rules)
- [x] Insert media picker (fuzzy query, pool cycle, live/config toggles, preview, footer)
- [x] PICKER pill + breadcrumb + insert-media message
- [x] Actions wired: pool n/b/s/D/r/a/X, vol n/R/C/u/W/X, ␣mi anywhere, ⏎ on ISO, C-e eject
- [ ] Deviations (with reason): pool/vol `n` stages ex prefill lines (full define-as wizards deferred to P11 patterns); live USED BY matches domain disk paths, backing shown as `◆ backing`; picker pool path comes from pool XML target.

## 04 New domain wizard (mockup: docs/mockups/Wizard.dc.html)
- [x] Modal (136×36, step N/6), step list (✓/●/○ + summaries), osinfo min/rec check
- [x] Step 3 CPU & Memory (sliders, maximum, topology, model dropdown, presets, options)
- [x] Equivalent command (per-step highlight, ◂ this step, styled flags)
- [x] Keys (Tab/h/l/H/L/C-n/C-p/Esc/C-u/C-e/y/Enter), INSERT status + autosaved + draft restore
- [x] All 6 steps modeled in state (summaries, validation, virt-install argv); step 3 has the full form UI
- [x] virt-install --noautoconsole + --print-xml edit-as-XML path; draft autosave/restore + C-u reset
- [ ] Deviations (with reason): steps 1/2/4/5/6 render a compact placeholder (full field rows in follow-up); `Space+n` was rebound from Clone to New domain (`Space+c` clones); virt-install display uses `--opt=value` form.

## 05 Command palette (mockup: docs/mockups/Palette.dc.html)
- [x] Modal (148×34 top), title + context, input row (❯ + shown/total)
- [x] Results (icon/category/match/key) + recent, reference pane (SYNOPSIS/OPTIONS + preview)
- [x] Footer legend + prefix legend; generic staged command (Tab toggles, C-l pane, C-y copy)
- [x] All 285 virsh help entries parse (test iterates live index)
- [ ] Deviations (with reason): staged valued options use fixed placeholders (`--name pre-upgrade-…`); raw commands stage for review instead of auto-running.

## 06 Keybindings overlay (mockup: docs/mockups/Help.dc.html)
- [x] Modal 156×37, 4 columns, section colours + underlines, destructive rows red, footer
- [x] Generated from keymap sections; live `/` filter
- [ ] Deviations (with reason): none.

## 05 Events view (tab 5, not mocked)
- [x] Full-height table TIME/TYPE/OBJECT/EVENT/DETAIL, filter chips, search, 5,000 cap
- [x] Live event stream subscription (virsh event --all --loop --timestamp) with backoff
- [ ] Deviations (with reason): built from dashboard components per plan §9.12.

## 10 Settings (mockup: docs/mockups/Settings.dc.html)
- [x] Categories (7) + Theme list (❯ + 8 swatches + custom hint) + Appearance options + Preview (16 swatches, samples, gauge, chart)
- [x] Live theme preview on j/k, Enter applies + saves, Esc reverts
- [x] Status breadcrumb + saved message; topbar settings segment
- [x] All 9 themes render (dashboard style-map snapshot, no missing tokens)
- [ ] Deviations (with reason): non-Appearance categories show their persisted values read-only in the list pane (text edits via `:set`); custom themes preview with default border/icons.

## Review 2026-10-06 (re-verification after the code review)

Method: every screen rendered with `virsh-tui --demo --frozen --screen <name> --dump <file> --size 174x43`
(ANSI output, `cat` to view) and compared line by line with its mockup; live mode checked read-only in tmux
against `qemu:///system`. Colours were checked by reading the code paths (style tokens), not pixel by pixel.
Several items ticked above were **not** true before this review and were fixed here:

- [x] Panel right titles are right-aligned with padding (were glued to the left title) — all views via `widgets::panel::block`.
- [x] Footers sit on the bottom border (were content lines or hand-drawn overlays).
- [x] Selected rows fill their whole width with `sel` (separators were unfilled → "striped" rows).
- [x] 1-column gap between side-by-side panels (panels were touching: `╮╭`).
- [x] Status bar right segments (counters, `[+] modified`, pills) are right-aligned; counters use state colours.
- [x] Top bar: right segment right-aligned, real hostname/clock in live mode (was always `forge` / `14:32:07`).
- [x] Message line: one coloured prefix (`✓ ✗ ⚠ ⏻ [dry-run]`), command tail dimmed (was `✓ ✓ …`, and `-- INSERT --` got a green ✓).
- [x] Dashboard: filter chips right-aligned as chips; table scrolls with scrolloff 2; action chips wrap like the mock.
- [x] Dashboard compact layout: real tabbed panel (was a one-line stub).
- [x] Hardware editor: sections (Allocation, Topology, Model, Features, Pinning, Apply to), value boxes, radios, checkboxes.
- [x] Snapshots: tree columns aligned (date/state/kind); revert modal 88×20 with padding, wrapping, real current snapshot, exact commands.
- [x] Modals dim the background ("dim behind modals" setting); transparent background setting works.
- [x] Palette: right title right-aligned, input row with count, result highlighting without duplicated category, vertical rule, reference of the *selected* action.
- [x] Help: key column wide enough (`:attach-iso` overflowed); keys come from the live keymap.
- [x] Networks / Storage / Host / Events: live data instead of mockup values (see DECISIONS.md).
- [ ] Deviations (with reason): Monitor tab adds a per-vCPU/disk/NIC row under the 2×2 charts (PLAN §9.5, not in a mockup);
      Settings categories other than Appearance use one option panel (not mocked); host network sparklines need a
      few samples before they show traffic.
