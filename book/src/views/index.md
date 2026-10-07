# Views

virsh-tui has five top-level views. Press the number key to switch, from
anywhere outside a text field.

| Key | View | What it shows |
|-----|------|---------------|
| `1` | [Domains](domains.md) | every domain, with live stats for the selected one |
| `2` | [Host](host.md) | the hypervisor host: CPU threads, memory, pools, networks |
| `3` | [Networks](networks.md) | virtual networks, DHCP leases and attached interfaces |
| `4` | [Storage](storage.md) | storage pools and volumes, ISO media |
| `5` | [Events](events.md) | the libvirt event stream |

`Enter` on a domain opens the [domain detail](detail.md), which has its own
tabs.

Host, Networks and Storage load in the background when virsh-tui starts and
refresh each time you enter them, so the first screen appears straight away
even on hosts with many domains and volumes.
