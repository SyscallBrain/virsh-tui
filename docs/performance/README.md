# CPU performance results

Measured on 2026-11-13 against baseline `a1c42067234cfcbb2b2b9e73f94beb490c4c1884`,
the commit this work originally started from. The branch was later rebased onto
`0e43523`, where `main` had already changed metrics ingestion to move previous
counters out of the map instead of cloning them. These numbers were not
re-measured on the rebased tree.

## Reproduction

```sh
cargo bench --bench performance
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

To compare against the baseline, check out the baseline commit in a separate
worktree, copy `benches/performance.rs` and the `[[bench]]` section of
`Cargo.toml` into it, build both with `cargo bench --bench performance --no-run`,
then alternate the two binaries (before, after, before, after, …) pinned to the
same CPU with `taskset -c 3`.

Rust 1.99.1, Linux x86_64, four virtual CPUs on AMD EPYC 9355P. The host was
shared and not idle (load average 2–5 during the runs), which affects the
full-frame results most.

The benchmark uses the optimized Cargo bench profile, `black_box`, 20 warm-up
calls and nine timed batches of at least 150 ms, with ten operations per clock
check. Each line reports the median of the batch means plus the minimum and
maximum batch means. These are observations, not CI timing assertions.

The same benchmark source was compiled against the baseline and the optimized
sources (it only uses APIs that exist in both). Fixtures and terminals are built
outside the timed loops. Before measuring, the benchmark asserts the result size
of each fuzzy/palette workload and that every metrics domain was ingested, so a
case cannot silently time an empty early-exit path. Metrics measurements use
distinct domain names, advance sample timestamps and keep raw counters constant:
they exercise ingestion, zero-rate computation, history writes and cleanup, not
realistic hypervisor traffic.

## Measurements

Three interleaved rounds (before, after, before, after, before, after), both
binaries pinned to CPU 3. Raw output: `before-r{1,2,3}.txt` and
`after-r{1,2,3}.txt`. Values are µs/op: the median over the three rounds of each
round's median. Speedup is baseline / optimized, with each round's speedup also
shown so you can see how much it varies.

`baseline-20261113.txt` is the shared baseline for the follow-up pass (extended
to every view, overlay and the `:` Tab-completion path);
`after-parallel-views-cpu3.txt` is its counterpart, both processes pinned to
CPU 3. `after-parallel-views.txt` is the unpinned run, kept to show how much the
shared host inflates medians.

| Workload | Before µs | After µs | Speedup | Per-round speedups |
|---|---:|---:|---:|---|
| braille Fill 60x8 | 5.132 | 2.781 | 1.85× | 2.38 / 1.89 / 1.85 |
| braille Down 60x8 | 3.958 | 2.795 | 1.42× | 1.95 / 1.42 / 1.42 |
| braille Line 60x8 | 3.760 | 2.979 | 1.26× | 1.56 / 1.25 / 1.28 |
| spark rows 60x8 | 5.218 | 1.103 | 4.73× | 5.25 / 4.68 / 4.73 |
| fuzzy snap 1000 | 118.845 | 120.868 | 0.98× | 1.01 / 0.98 / 0.99 |
| fuzzy snap 1000 unicode haystacks | 658.426 | 625.022 | 1.05× | 1.42 / 1.03 / 1.05 |
| fuzzy dömäin 1000 no match | 135.531 | 5.774 | 23.47× | 25.15 / 23.41 / 24.68 |
| palette visible 1000 | 903.909 | 117.082 | 7.72× | 7.41 / 8.03 / 7.60 |
| ingest metrics 0 domains | 0.009 | 0.011 | 0.90× | 0.79 / 0.89 / 0.90 |
| ingest metrics 1 domains | 2.781 | 1.602 | 1.74× | 1.24 / 1.70 / 1.74 |
| ingest metrics 10 domains | 28.644 | 16.383 | 1.75× | 1.23 / 1.70 / 1.76 |
| ingest metrics 100 domains | 319.743 | 163.901 | 1.95× | 2.29 / 1.76 / 1.96 |
| ingest metrics 1000 domains | 5009.846 | 2030.762 | 2.47× | 3.56 / 2.47 / 2.59 |
| dashboard 120x36 | 166.274 | 165.205 | 1.01× | 1.10 / 0.95 / 1.04 |
| host 120x36 | 199.149 | 195.899 | 1.02× | 1.19 / 1.02 / 1.04 |
| monitor 120x36 | 235.810 | 271.985 | 0.87× | 0.75 / 0.90 / 0.86 |
| dashboard 174x43 | 357.187 | 460.783 | 0.78× | 0.61 / 0.76 / 1.06 |
| host 174x43 | 320.240 | 318.305 | 1.01× | 0.96 / 0.98 / 1.11 |
| monitor 174x43 | 413.982 | 393.567 | 1.05× | 1.49 / 1.03 / 1.04 |
| dashboard navigation 174x43 | 311.203 | 304.730 | 1.02× | 1.05 / 1.02 / 1.01 |
| palette navigation 1000 174x43 | 2837.170 | 343.504 | 8.26× | 8.57 / 8.56 / 8.18 |
| XML 10000 lines middle page | 2247.410 | 332.746 | 6.75× | 7.34 / 6.75 / 6.47 |

### Interpretation

Consistent improvements (every round agrees):

- Palette navigation with 1,000 entries: ~8.3×. Previously each frame ranked
  the list three times and looked up every title linearly, which was O(n²).
- `PaletteState::visible` with 1,000 entries: ~7.7×.
- XML page in a 10,000-line document: ~6.7×.
- Multi-row sparks: ~4.7×.
- Braille charts: 1.3–1.9×.
- Metrics ingestion: ~1.7× at 1–10 domains and ~2.5× at 1,000 domains, where
  the earlier cleanup was quadratic.

Results that need care:

- **Fuzzy matching itself did not get faster** (`fuzzy snap`: ~1.0× for both
  ASCII and Unicode haystacks). The earlier report's 28× for `fuzzy dömäin` was
  a **no-match path**: an ASCII haystack can never match a non-ASCII needle, so
  nucleo returns immediately. The baseline still converted the needle for every
  item, so the gain there is only the cost of that conversion. The case keeps
  its explicit "no match" label and is not representative of palette search.
- **No full-frame improvement is claimed.** Steady-state dashboard, host and
  monitor frames are within noise. An earlier single pinned run reported
  `host 174x43` at 1.44×, but neither the unpinned run nor these interleaved
  rounds reproduce it. `monitor 120x36` was slower in all three rounds (~0.87×)
  and `dashboard 174x43` varied from 0.61× to 1.06×. That spread is consistent
  with a busy host, but the monitor slowdown has not been explained and needs to
  be rechecked on an idle machine before drawing conclusions.
- The empty metrics case is a couple of nanoseconds slower, because it now
  builds an empty `HashSet`. No domain work happens in that case.

### Functional issue found while validating (fixed separately)

While the benchmark was being fixed, `fuzzy_rank` turned out to pass the raw
query to `nucleo_matcher`, which expects a prepared needle. Accented queries
(`dömäin`, `ö`) never matched, and uppercase letters matched case-sensitively.
The baseline had the same bug. It is now fixed: the query is prepared once as a
`nucleo_matcher::pattern::Atom` (case ignored, smart normalization), and
`tests/p12_palette.rs` covers the behavior. A pinned re-run on a quiet host
showed the fuzzy cases unchanged within noise (`fuzzy snap 1000`: 126 µs; the
no-match case: ~8 µs, because building the atom costs a little up front). The
table above was measured before this fix.

## Implementation and correctness

- Braille charts store one interval per dot column and encode masks directly,
  removing the dot-grid allocation and scan. Row strings reserve their UTF-8
  capacity. All three modes match the original implementation across varied
  widths/heights, random walks, empty data, bounds and nonfinite values.
- Multi-row sparks interpolate once per column. Single-row sparks emit directly.
  Host thread meters reuse each three-row result rather than rendering it three
  times. Existing chart fixtures and screen snapshots remain unchanged.
- Palette ranking retains item indices, reuses Unicode conversion buffers and
  converts the query once (this only matters for non-ASCII queries; see above). Result lookup no longer searches the full list for
  every title. A frame shares the ranked results with its reference pane and
  command preview. Duplicate titles keep distinct action identities.
- XML highlighting and span allocation are limited to the viewport, and scroll
  positions above 65,535 lines work. Full-document line counting and traversal to
  the viewport remain O(document bytes); there is no mutable-content cache.
- Metrics cleanup uses name membership instead of repeated linear searches.
  Previous counters are borrowed rather than cloned, and totals use the device
  counters already read instead of reformatting/reparsing the same virsh keys.
  Sparse multi-device counters, rates, removal and restart behavior are tested.

## Parallel view loads

Every `virsh` invocation is a fresh process that pays startup and connection
cost (~20 ms on this host). The Host, Networks and Storage loads ran those
processes serially, so opening a view with N networks or N active pools cost
roughly N × 20 ms before the data appeared.

- `net-dumpxml` per network (Networks view and Host view) and `vol-list` per
  active pool (Storage view) now go through the existing bounded-concurrency
  helper (`READ_CONCURRENCY = 6`), preserving input order and error reporting.
- The four independent Host reads (system facts, pools, networks, domains) and
  the two independent Storage reads (domains, pools) run concurrently with
  `tokio::join!`.
- The volume-detail reads (`vol-dumpxml`) were already concurrent and are
  unchanged.

The effect is a latency win that scales with the number of objects (up to ~6×
for many networks or pools), not a CPU win: each workload is unchanged in
kernel time. With a single network or pool the wall-clock difference is inside
the noise.

`tests/support/parallel_views.py` (driven by `view_loads_overlap_per_object_reads`)
runs the real binary against a fake `virsh` that reports 12 networks and counts
how many `net-dumpxml` processes are in flight. It asserts the peak is above 1 and
within `READ_CONCURRENCY`; the serial implementation peaked at 1.

Validation: `cargo fmt --check`, strict Clippy for all targets, all 479 tests
(including nine performance regression tests), and `mdbook build book` pass.
The full-screen snapshots required no updates. Independent read-only review
covered the algorithms, command-preview reuse, metrics semantics and benchmark
limitations. No libvirt object was mutated during this work.

## Limits

Steady-state frame cases include ratatui's unchanged-buffer diff path. Navigation
cases change selected rows and include changed-cell writes to the in-memory test
backend. Neither measures a real terminal's output transport, SSH, rendering,
virsh startup/connection latency, or a full live libvirt workload. The parallel
view loads are justified analytically (N serial process spawns become at most
`READ_CONCURRENCY` in flight) and were not re-timed against a live libvirt with
many networks or pools. Do not interpret kernel speedups as a universal
application speedup. No overall full-frame speedup is claimed where scheduling
variance overlaps the observed difference.

Measurements target the expensive CPU paths identified here; they do not establish
a mathematical maximum for application performance. The raw ranges are retained
so future changes can be evaluated without treating noisy medians as guarantees.
