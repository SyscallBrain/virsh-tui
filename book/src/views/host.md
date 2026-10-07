# Host

![Host view](../images/host.png)

The host view describes the machine libvirt runs on: CPU model and load, a
meter per hardware thread, memory used and free, the resident memory of each
domain, hugepages, KSM, swap, storage pools, networks and the virtualization
features the CPU exposes.

The running domains are sorted by CPU use, busiest first.

| Key | Action |
|-----|--------|
| `j` / `k` | select a running domain in the table |
| `gg` / `G` | first / last domain |
| `Enter` | open that domain's detail |

Per-thread CPU meters, temperatures and memory details are read from `/proc`
and `/sys`, so they are only shown for local connections. For a remote URI
the view shows what libvirt itself reports (`virsh nodeinfo`, pools,
networks). Per-thread sampling can be turned off with
`monitoring.thread_sampling = false`.
