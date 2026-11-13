# The command palette

![Command palette](images/palette.png)

`Ctrl-p` opens a fuzzy finder over every action virsh-tui has, with its key
binding and the domain it applies to. Type a few letters (`snap`, `iso`,
`setmem`) and press `Enter` to run the highlighted entry.

Matching ignores case, and plain letters also match accented ones: `domain`
finds `dömäin`, while `dömäin` matches only the accented spelling.

Entries with the same displayed title remain separate actions; moving the selection
chooses the corresponding entry.

For entries backed by a `virsh` subcommand, the right-hand pane shows its
synopsis and its options as check boxes, so you can add flags such as
`--live` or `--atomic` without remembering their names.

| Key | Action |
|-----|--------|
| type | filter the entries |
| `↑` / `↓`, `Ctrl-k` / `Ctrl-j` | move through the entries, or through the options when the pane has focus |
| `Ctrl-l` | move the focus between the entries and the options pane |
| `Tab` | toggle the option under the cursor |
| `Ctrl-y` | copy the resulting `virsh` command |
| `Enter` | run the entry |
| `Esc` | close |


Below 140 columns the results and reference share a full-width pane.
`Ctrl-l` switches between them. The reference includes the complete wrapped
command and help text; `PageUp`/`PageDown` scrolls it, while arrows select
options. `Enter` still opens the selected entry in the command line.
