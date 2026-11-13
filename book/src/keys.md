# Key bindings

![Help overlay](images/help.png)

`?` shows the bindings inside virsh-tui. The overlay is built from the active
key map, so it reflects your `keymap.toml`. Type to filter it; `Tab` switches
to the keys of individual views and dialogs (Storage, Networks, the hardware
editor, snapshot dialogs, the media picker, Events), `↑` / `↓` scroll. While
an overlay is open it takes every key: nothing reaches the view behind it.

## Default bindings

### Navigation

| Key | Action | Name |
|-----|--------|------|
| `j` / `k` | down / up | `move_down` / `move_up` |
| `gg` / `G` | top / bottom | `go_top` / `go_bottom` |
| `Ctrl-d` / `Ctrl-u` | half page down / up | `half_down` / `half_up` |
| `Enter` | open | `open` |
| `Backspace`, `q` | back (`q` quits from the top level) | `back` |
| `/` | filter | `filter` |
| `n` / `N` | next / previous match | `next_match` / `prev_match` |
| `H` / `L`, `[` / `]` | previous / next tab | `prev_tab` / `next_tab` |
| `1` … `5` | Domains, Host, Networks, Storage, Events | `view_domains` … `view_events` |

### Domain actions

| Key | Action | Name |
|-----|--------|------|
| `s` | start | `start` |
| `S` | ACPI shutdown | `shutdown` |
| `D` | destroy (force off) | `destroy` |
| `r` | reboot | `reboot` |
| `R` | reset | `reset` |
| `p` | pause / resume (decided per domain) | `pause` |
| `Z` | managed save (suspend to disk), confirms | `managed_save` |
| `a` | toggle autostart (per domain) | `autostart` |
| `c` | serial console | `console` |
| `v` | graphical viewer | `viewer` |
| `e` | edit XML | `edit_xml` |
| `X` | undefine | `undefine` |

### Copy

| Key | Copies | Name |
|-----|--------|------|
| `yy` | name | `yank_name` |
| `yu` | UUID | `yank_uuid` |
| `yi` | IP address | `yank_ip` |
| `yc` | last command | `yank_cmd` |

### Leader (`Space`)

| Key | Action | Name |
|-----|--------|------|
| `Space n` | new domain | `new_domain` |
| `Space c` | clone | `clone` |
| `Space r` | rename | `rename` |
| `Space M` | migrate | `migrate` |
| `Space s c` / `s r` / `s d` | snapshot create / revert / delete | `snapshot_new` / `snapshot_revert` / `snapshot_delete` |
| `Space m i` / `m e` | insert / eject ISO | `insert_media` / `eject_media` |
| `Space d a` / `d r` | attach / resize disk | `disk_attach` / `disk_resize` |
| `Space i a` / `i l` | attach NIC / toggle link | `nic_attach` / `nic_link` |
| `Space x` | export XML | `export_xml` |
| `Space b` | boot order | `boot_order` |
| `Space l` / `Space L` | pin / unpin a DHCP lease | `lease_pin` / `lease_unpin` |

### Lists and monitor

| Key | Action | Name |
|-----|--------|------|
| `m` | mark / unmark | `mark` |
| `V` | visual selection | `visual` |
| `o` | cycle sort | `sort_cycle` |
| `f` / `F` | cycle filter chips | `chip_cycle` / `chip_cycle_back` |
| `t` | history window 60 s / 5 min / 1 h | `window_cycle` |
| `l` | focus the leases table (Networks) | `lease_focus` |
| `u` | undo field (hardware editor) | `undo_field` |
| `gx` | go to the XML tab | `goto_xml` |

### App

| Key | Action | Name |
|-----|--------|------|
| `:` | command line | `ex` |
| `Ctrl-p` | command palette | `palette` |
| `?` | help | `help` |
| `,` | settings | `settings` |
| `Ctrl-r` | refresh now | `refresh` |
| `Ctrl-l` | redraw the screen | `redraw` |
| `.` | repeat the last change | |
| `q`, `ZZ`, `Ctrl-c` | quit (see active jobs below) | `quit` |

`Z` waits 0.6 s for a second `Z` (`ZZ` quits); only a lone `Z` runs managed
save, after a confirmation. `Z` followed by another key does nothing.
A count is capped at 9999 (`9999j`). `q` and the view keys refuse to leave a
domain with unsaved hardware edits (`:w` applies, `:q!` discards). A `Ctrl-c`
pressed just after a job finished does not quit; press it again.

On marked rows, actions apply to the marked rows that are visible (marks
hidden by the filter or a chip are left alone), always ask for confirmation
when they affect more than one domain, and the marks are cleared when the
job ends. A failing domain does not stop the others; the message line reports
`n ok / m failed` with the names, details in `:messages`.

Some screens add their own fixed keys (not remappable), listed in the status
bar and on the second page of `?`: the
[hardware editor](views/detail.md#hardware), the [wizard](wizard.md),
[Networks](views/networks.md) and [Storage](views/storage.md).

## Remapping keys

Create `~/.config/virsh-tui/keymap.toml`:

```toml
[normal]
shutdown = ["S", "Space q"]      # several bindings for one action
snapshot_new = ["Space s n"]     # tokens separated by spaces
console = ["C-t"]
```

- Each entry maps an action name (the "Name" column above) to a list of key
  sequences.
- A sequence is a list of tokens separated by spaces. A token is a single
  character (`a`, `A`, `:`), `C-x` for Ctrl, or one of `Space`, `Enter`,
  `Esc`, `Tab`, `S-Tab`, `Backspace`.
- Remapping an action removes its default keys. List them again if you want
  to keep them.
- Sub-tables such as `[normal.domains]` are accepted to group entries, but
  their bindings apply in every view.

Unknown action names, unknown key names (such as `Ctrl-q` instead of `C-q`)
and syntax errors are reported in the message line when virsh-tui starts;
the rest of the file still applies. The help overlay and
the which-key popup show your bindings.

All action names: `move_up`, `move_down`, `go_top`, `go_bottom`, `half_down`,
`half_up`, `open`, `back`, `filter`, `next_match`, `prev_match`, `start`,
`shutdown`, `destroy`, `reboot`, `reset`, `pause`, `managed_save`,
`managed_save_remove`, `autostart`, `console`, `viewer`, `edit_xml`,
`undefine`, `rename`, `clone`, `migrate`, `send_key`, `export_xml`,
`new_domain`, `yank_name`, `yank_uuid`, `yank_ip`, `yank_cmd`, `mark`,
`visual`, `sort_cycle`, `chip_cycle`, `chip_cycle_back`, `window_cycle`,
`lease_focus`, `lease_pin`, `lease_unpin`, `ex`, `palette`, `help`,
`settings`, `refresh`, `redraw`, `quit`, `quit_all`, `snapshot_new`,
`snapshot_revert`, `snapshot_delete`, `insert_media`, `eject_media`,
`disk_attach`, `disk_resize`, `nic_attach`, `nic_link`, `boot_order`,
`prev_tab`, `next_tab`, `undo_field`, `goto_xml`, `view_domains`,
`view_host`, `view_networks`, `view_storage`, `view_events`.

## Active jobs

In live mode, dashboard lifecycle shortcuts, hardware applies, snapshot confirmations, media changes, XML export, selected network and storage
actions, and confirmed command plans run in the background. Raw virsh commands and several command-line
actions also use background jobs. Domain creation from the wizard also runs as a
background job; a requested graphical viewer opens after successful creation.
Their progress appears on the
message line. During a job, the palette shows available cached entries and settings
can be viewed without applying changes; `:messages` and `:help` remain available.
You can close the wizard and open a new draft with `Space n` while a job runs;
submitting the new draft is refused until the active job has finished.
`Ctrl-c` requests cancellation instead of quitting; quit requests wait until
the job finishes. An editor or console deliberately takes over the terminal
until it exits.

Opening a domain's detail view normally shows it immediately while its
configuration and snapshots load. Actions that depend on those details wait
until loading finishes; navigation remains available.
