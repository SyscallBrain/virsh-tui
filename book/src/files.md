# Files and paths

virsh-tui follows the XDG base directory layout.

| Path | Contents |
|------|----------|
| `~/.config/virsh-tui/config.toml` | settings ([Configuration](configuration.md)) |
| `~/.config/virsh-tui/keymap.toml` | key overrides ([Key bindings](keys.md#remapping-keys)) |
| `~/.config/virsh-tui/themes/*.toml` | custom themes ([Themes](themes.md)) |
| `~/.local/state/virsh-tui/virsh-tui.log` | log (the previous one is kept as `virsh-tui.log.old` past 4 MiB) |
| `~/.local/state/virsh-tui/history` | lines typed at `:` (`general.history_size`), mode 0600 |
| `~/.local/state/virsh-tui/wizard-draft.toml` | the new domain wizard's draft |
| `$XDG_RUNTIME_DIR/vt-*.xml` | temporary XML files, created with mode 0600 and removed after use |

`$XDG_CONFIG_HOME` and `$XDG_STATE_HOME` change the first two locations.
Temporary files fall back to the system temp directory when
`$XDG_RUNTIME_DIR` is not set.

Each session appends to the log. `general.log_level = "debug"` or
`RUST_LOG=virsh_tui=debug virsh-tui` makes it verbose; include it when you
report a bug.
