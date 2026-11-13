# Performance work

Goal: reduce CPU time and allocations in interactive paths using repeatable release benchmarks while preserving the approved UI, command previews, and libvirt safety rules.

Approach: measure before changing code, remove repeated work and quadratic searches, then rerun the same benchmarks. Caching mutable public state would require pervasive invalidation; direct indexed work and viewport rendering avoid that risk. Changing the backend or using unsafe code is unnecessary for these CPU bottlenecks.

1. Establish the full test baseline in an isolated managed worktree. Add dependency-free release benchmarks for chart kernels, fuzzy matching, palette results, metrics ingestion at 10/100/1,000 domains, full dashboard/host/monitor frames, and a 10,000-line XML view.
2. Guard chart output against the original implementation and fixtures, Unicode fuzzy scoring/ties, duplicate palette titles, and large XML scroll offsets. Confirm boundary failures before fixing them.
3. Replace the braille point grid with vertical intervals and lookup masks; interpolate spark samples once per column, emit single-row sparks directly, and reuse Host thread meters; retain palette indices through ranking and reuse one result list per rendered palette; highlight only the XML viewport; use set membership for metrics cleanup, borrow previous counters, and derive totals from the device counters already read.
4. Compare before/after release measurements, inspect regressions and allocation behavior, run formatting, strict Clippy and the full test suite, and record measurements and limitations.

All benchmark workloads are in memory. Do not mutate libvirt objects or use sudo. Existing snapshots remain the UI acceptance reference. User-visible fixes (duplicate palette results and large-document scrolling) must be documented in the book.
