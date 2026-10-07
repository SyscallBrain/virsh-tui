# virsh-tui

Rust + ratatui TUI for libvirt/virsh. Everything (UI, code, docs, commits) is in English.

- Implementation plan: `docs/PLAN.md` (follow its phases in order, with the acceptance criteria and safety rules in §0.2).
- UI source of truth: `docs/DESIGN.md` + `docs/mockups/*.dc.html` (exact strings, demo data, colours).
- Fidelity log: `docs/FIDELITY.md`. Deviations and assumptions: `docs/DECISIONS.md`.
- Never mutate libvirt objects that you did not create (`vt-test-*` prefix only). Never use sudo.
- Gates: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- User docs: `book/` (mdBook, published to GitHub Pages); keep it in sync with behaviour. Screenshots: `scripts/screenshots.py` (demo mode only).
