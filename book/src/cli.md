# Command-line options

```text
virsh-tui [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `-c`, `--connect <URI>` | libvirt connection URI, e.g. `qemu:///system`. Defaults to `general.default_uri` |
| `--demo` | use the in-memory demo backend; nothing is executed |
| `--dry-run` | show the commands that actions would run, without running them |
| `--theme <NAME>` | theme for this session (built-in or custom), overriding the config |
| `--config <PATH>` | use this config file instead of `~/.config/virsh-tui/config.toml` |
| `--screen <NAME>` | open on a given screen (see below) |
| `--probe` | print the parsed domains, host, networks and pools as JSON and exit (read-only) |
| `--dump <FILE>` | render one frame to an ANSI file and exit |
| `--size <WxH>` | frame size for `--dump`, default `174x43` |
| `--frozen` | freeze the demo clock and fill the graph history (for tests and screenshots) |
| `-h`, `--help` | print help |
| `-V`, `--version` | print the version |

`--screen` accepts `dashboard`, `host`, `networks`, `storage`, `events`,
`detail`, `monitor`, `hardware`, `snapshots`, `wizard`, `palette`, `help`
and `settings`. Without `--dump` it only changes the start view for the five
top-level views.

## Examples

```sh
# Manage the per-user daemon
virsh-tui -c qemu:///session

# A remote host over SSH
virsh-tui -c qemu+ssh://admin@hv01/system

# See what the keys would do, without changing anything
virsh-tui --dry-run

# Check what virsh-tui parses from your host
virsh-tui --probe | jq '.domains[].name'

# Render a screenshot-ready frame of the demo
virsh-tui --demo --frozen --screen host --dump host.ans --size 160x45
```
