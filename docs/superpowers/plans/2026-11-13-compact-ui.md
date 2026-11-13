# Compact UI Implementation Plan

> Execute the approved audit changes with regression tests and a final independent review.

**Goal:** Keep forms, confirmations and command review usable across supported terminal sizes.
**Architecture:** Retain existing view state and wide layouts; adapt renderers and modal input at compact breakpoints. Keep mutation plans unchanged.
**Tech Stack:** Rust, ratatui, existing test backend and snapshot suite.
**Spec:** docs/superpowers/specs/2026-11-13-compact-ui-design.md

## Constraints

English UI and docs. Demo-only checks. Never sudo or mutate existing libvirt objects. Work in the user's current checkout; preserve unrelated changes. No new dependencies.

## Tasks

- [x] Confirmation safety: first add failing regression coverage for minimal snapshot modals and hidden affirmative input; render compact snapshot dialogs, retain complete bulk previews, support scrolling and fixed actions.
- [x] Hardware: failing rendering test at 120x36, editor-first compact panes and focus following; preserve wide layout.
- [x] Wizard: failing tests for final focused fields in small terminals, compact form and command review with concise hints.
- [x] Settings/palette: failing tests for active options/reference in small terminals, render the active pane and provide contextual navigation.
- [x] Legibility/docs: use secondary foreground for essential copy, sync approved UI sources and user book, regenerate affected demo screenshots.
- [x] Verification: cargo fmt --check, cargo clippy --all-targets -- -D warnings, cargo test; inspect demo frames at 174x43, 120x36, 100x30, 80x24 and 40x12. Independent review and resolve findings.

## Review focus

Resizing while confirmations are open; typed names versus scroll keys; command lines wider than the viewport; focus on the last form field; pane navigation at the minimum terminal size.


## Execution record

- Implemented all six task groups. Added compact rendering/input regressions, including invisible confirmations after resize, long Unicode typed names, long hardware paths, and immediate scroll reversal at both ends.
- Independent review findings (hidden long-path caret, accumulated overscroll, Unicode typed-name clipping) were fixed and re-reviewed with no outstanding findings.
- Verification: cargo fmt --check; cargo clippy --all-targets -- -D warnings; cargo test --locked; mdbook build. Demo screenshots and README GIF regenerated with the supplied scripts. Screenshots explicitly request RGB output so the HTML renderer is independent of the invoking shell's colour support.
- Ruling: work in the current checkout and leave changes uncommitted for user review; no libvirt connection or mutation was needed.
