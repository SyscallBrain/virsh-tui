# Compact UI and review visibility

The user approved the UI/UX audit recommendations and requested implementation.
Keep the existing Rust/ratatui visual language and English copy. Preserve wide-screen layouts where possible.

## Approach

Prioritize active work over secondary panels: compact hardware uses editor-first panes, the wizard uses a focus-following form, Settings presents its active pane, and the palette switches between results and reference. This is preferable to shrinking all columns or requiring a larger terminal, which hides essential values.

Confirmations remain visible above every supported layout. Below the supported minimum, affirmative input must not execute hidden actions. Generic confirmation previews retain every command and allow scrolling, with fixed actions and typed confirmation. Long lines must remain inspectable.

Essential labels and instructions use the existing secondary foreground token; keep the palette unchanged and reserve the dim token for decorative or incidental content. Compact hints prioritize navigation, cancellation and contextual actions.

## Acceptance

- Hardware values and focused fields remain visible at 120x36.
- Wizard fields follow focus at 80x24 and 40x12; command review is accessible.
- Settings options can be seen and edited at 80x24.
- Snapshot confirmation appears at 80x24 and 40x12; hidden confirmations cannot execute below 40x12.
- All bulk targets/commands remain inspectable, with confirm/cancel fixed.
- Palette reference and command remain accessible at 100x30 and 80x24.
- Update DESIGN, mockups, FIDELITY, DECISIONS and book to match.
- Demo-only validation; no libvirt mutations, no sudo, no new dependencies.
