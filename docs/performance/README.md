# CPU performance results

Measured on 2026-11-13, against baseline `a1c42067234cfcbb2b2b9e73f94beb490c4c1884`.
All implementation work lives in the managed worktree
`/home/tiago/.codex/worktrees/86b8/virsh_tui`.

## Reproduction

```sh
cargo bench --bench performance
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Rust 1.99.1, Linux x86_64, four virtual CPUs on AMD EPYC 9355P.
The benchmark uses the standard optimized Cargo bench profile, `black_box`, 20
warm-up calls and nine timed batches of at least 150 ms, with ten operations per
clock check. Results are medians of batch mean wall times; the raw files include
minimum and maximum batch means. They are observations, not timing assertions in
CI. Shared-host scheduling, other agents and VMs introduce noise.

The same final benchmark source was used against the baseline product sources and
the optimized product. The initial baseline and optimized runs also identified
these bottlenecks before the final comparison. Build work finishes before each
measurement loop. Fixtures and terminals are constructed outside timed loops.
Metrics measurements use distinct domain names, advance sample timestamps, and
retain constant raw counters; they exercise ingestion, zero-rate computation,
history writes and cleanup, rather than simulating hypervisor traffic.

## Measurements

See `before.txt`, `after.txt`, `before-cpu3.txt` and `after-cpu3.txt` for all
raw results, including min/max values.

Final sequential repeat, both processes pinned to CPU 3 with `taskset -c 3`.
Times below are µs/op; speedup is baseline / optimized. The unpinned run is
also retained because the shared host produces considerable scheduling variance.

| Workload | Before µs | After µs | Speedup |
|---|---:|---:|---:|
| braille Fill 60x8 | 5.441 | 2.724 | 2.00× |
| braille Down 60x8 | 4.034 | 2.736 | 1.47× |
| braille Line 60x8 | 4.095 | 2.862 | 1.43× |
| spark rows 60x8 | 5.549 | 1.128 | 4.92× |
| fuzzy snap 1000 | 132.105 | 116.701 | 1.13× |
| fuzzy dömäin 1000 | 147.843 | 5.165 | 28.62× |
| palette visible 1000 | 938.505 | 111.703 | 8.40× |
| ingest metrics 0 domains | 0.010 | 0.014 | 0.70× |
| ingest metrics 1 domains | 2.839 | 1.578 | 1.80× |
| ingest metrics 10 domains | 28.260 | 16.808 | 1.68× |
| ingest metrics 100 domains | 311.354 | 160.903 | 1.94× |
| ingest metrics 1000 domains | 5731.559 | 2634.534 | 2.18× |
| dashboard 120x36 | 166.201 | 183.634 | 0.91× |
| host 120x36 | 198.808 | 209.042 | 0.95× |
| monitor 120x36 | 241.495 | 255.179 | 0.95× |
| dashboard 174x43 | 414.491 | 399.699 | 1.04× |
| host 174x43 | 501.564 | 347.293 | 1.44× |
| monitor 174x43 | 464.104 | 432.241 | 1.07× |
| dashboard navigation 174x43 | 330.531 | 339.191 | 0.97× |
| palette navigation 1000 174x43 | 2962.242 | 347.547 | 8.52× |
| XML 10000 lines middle page | 2305.638 | 351.185 | 6.57× |

The largest repeatable wins are palette navigation (~8.5×), large XML pages
(~6.6×), multi-row sparks (~4–5×), and metrics ingestion (~2.2× at 1,000
domains; ~2× at one domain). Compact full frames vary around the baseline;
these results do not establish a general frame-rate improvement. The empty
metrics case is a few nanoseconds slower, close to timer-loop overhead; it
performs no domain work. Large-frame medians also vary between runs.

## Implementation and correctness

- Braille charts store one interval per dot column and encode masks directly,
  removing the dot-grid allocation and scan. Row strings reserve their UTF-8
  capacity. All three modes match the original implementation across varied
  widths/heights, random walks, empty data, bounds and nonfinite values.
- Multi-row sparks interpolate once per column. Single-row sparks emit directly.
  Host thread meters reuse each three-row result rather than rendering it three
  times. Existing chart fixtures and screen snapshots remain unchanged.
- Palette ranking retains item indices, reuses Unicode conversion buffers and
  converts the query once. Result lookup no longer searches the full list for
  every title. A frame shares the ranked results with its reference pane and
  command preview. Duplicate titles keep distinct action identities.
- XML highlighting and span allocation are limited to the viewport, and scroll
  positions above 65,535 lines work. Full-document line counting and traversal to
  the viewport remain O(document bytes); there is no mutable-content cache.
- Metrics cleanup uses name membership instead of repeated linear searches.
  Previous counters are borrowed rather than cloned, and totals use the device
  counters already read instead of reformatting/reparsing the same virsh keys.
  Sparse multi-device counters, rates, removal and restart behavior are tested.

Validation: `cargo fmt --check`, strict Clippy for all targets, all 186 tests
(including eight performance regression tests), and `mdbook build book` pass.
The full-screen snapshots required no updates. Independent read-only review
covered the algorithms, command-preview reuse, metrics semantics and benchmark
limitations. No libvirt object was mutated during this work.

## Limits

Steady-state frame cases include ratatui's unchanged-buffer diff path. Navigation
cases change selected rows and include changed-cell writes to the in-memory test
backend. Neither measures a real terminal's output transport, SSH, rendering,
virsh startup/connection latency, or a full live libvirt workload. Do not interpret
kernel speedups as a universal application speedup. No overall full-frame speedup
is claimed where scheduling variance overlaps the observed difference.

Measurements target the expensive CPU paths identified here; they do not establish
a mathematical maximum for application performance. The raw ranges are retained
so future changes can be evaluated without treating noisy medians as guarantees.
