# Storage and media

![Storage view](../images/storage.png)

Storage pools on the left, the selected pool's volumes on the right. Each
volume shows its format, capacity, allocation, backing chain and the domains
that use it. `h` and `l` move the focus between the two lists.

## Pools

| Key | Action |
|-----|--------|
| `s` | start the pool |
| `D` | destroy (stop) the pool, confirms |
| `b` | build the pool (create its directory or format its device) |
| `r` | refresh the volume list |
| `a` | enable autostart |

## Volumes

| Key | Action |
|-----|--------|
| `n` | create a volume (name, size, format) |
| `R` | resize: prefills `:vol-resize` |
| `C` | clone: prefills `:vol-clone` |
| `u` | upload a local file into the volume: prefills `:vol-upload` |
| `W` | wipe the volume with zeros, confirms |
| `X` | delete the volume, confirms |

## Inserting and ejecting ISO media

`Space m i` opens a picker with the ISO images found in your pools. Choosing
one inserts it into the domain's CD-ROM drive with
`virsh change-media … --insert`, on the running domain and its configuration.
If the domain has no CD-ROM drive, virsh-tui offers to attach one.
`Space m e` ejects the current medium. From the `:` line,
`:attach-iso <domain> <path>` does the same with a path you type, with Tab
completion for the file name.
