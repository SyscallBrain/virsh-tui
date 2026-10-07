# Networks

![Networks view](../images/networks.png)

The list of virtual networks with their state, autostart, bridge, forward
mode and address range. The selected network's DHCP leases appear next to
it, joined with the static host entries and with the domain that owns each
MAC address, followed by the interfaces attached to the network.

| Key | Action |
|-----|--------|
| `j` / `k` | select a network |
| `l` | move the focus to the leases table (and back) |
| `Enter` on a lease | open the domain that owns it |
| `Space l` | pin the selected lease as a static DHCP host entry |
| `Space L` | remove the static host entry |
| `yi` | copy the lease's IP address |
| `s` | start the network |
| `D` | destroy (stop) the network, confirms |
| `a` | toggle autostart |
| `e` | edit the network XML in your editor |
| `n` | create a network from a short form |
| `X` | undefine the network, confirms |

Pinning a lease runs `virsh net-update … add ip-dhcp-host` on both the live
network and its persistent definition, so the guest keeps its address after a
restart.
