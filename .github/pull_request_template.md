## What and why

<!-- What does this change, and what problem does it solve?
     If it fixes an issue, write "Closes #123". -->

## Checklist

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass
- [ ] Tests cover the change (snapshots reviewed with `cargo insta review` if the UI changed)
- [ ] The book (`book/src/`) is updated if keys, options or behaviour changed
- [ ] Screenshots regenerated (`scripts/screenshots.py`) if a screen changed visibly
- [ ] Changes to libvirt go through a `CommandPlan`; integration tests only touch `vt-test-*` objects
- [ ] Commit messages follow Conventional Commits (`fix:`, `feat:`, `docs:` …)
