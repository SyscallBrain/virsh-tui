# Responsive Jobs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep keyboard input, redraws, resize handling and live updates responsive while commands and on-demand queries run.

**Architecture:** Keep `App` and terminal rendering on the event-loop task. Submit owned operations to supervised Tokio tasks and apply typed results back on the event loop. Allow one user command job at a time; independent read-only loaders and the existing poller continue concurrently.

**Tech Stack:** Existing Rust (MSRV 1.88), Tokio, ratatui, crossterm and tempfile. Use Tokio channels and task handles; no new runtime or job framework. A small Unix signal dependency may be needed for safe process-group signalling; select an MSRV-compatible version during implementation.

**Spec:** [`docs/PLAN.md`](../../PLAN.md) §§0.2, 3.1, 6.4 and P15; [`docs/DESIGN.md`](../../DESIGN.md); the known limitation in [`docs/DECISIONS.md`](../../DECISIONS.md). The proposed design below resolves the choices left open by those documents.

**Status:** Implementation in progress. The production-loop fake-backend
responsiveness test passes, including a 30-second hold and strict timing.
Full route, platform and release acceptance remains open; unchecked items
below have not yet been signed off.

## Global constraints

- Everything in English: UI, identifiers, comments, documentation and commits.
- Follow P0–P14 in order; this is a P15 follow-up, not a replacement for unfinished acceptance criteria.
- Preserve exact command previews, argv, explicit connection URI, confirmations, bulk ordering and stop-on-first-failure behaviour.
- Never mutate existing libvirt objects during development. Any live mutation must use disposable `vt-test-*` objects created by the implementer; clean them up. Never use sudo.
- Demo mode never spawns backend commands. Dry-run never executes planned mutations, including preparation steps with side effects.
- Use existing message-line space, theme tokens and layout conventions. Keep idle demo snapshots unchanged. Record new busy-state strings and deliberate deviations in DESIGN, FIDELITY and DECISIONS.
- No `unwrap()`/`expect()` outside tests and main setup. Restore the terminal on normal exit, panic, SIGINT and SIGTERM.
- Required gates: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- Keep `book/` consistent with implemented behaviour. Generate screenshots only in demo mode.

## Review focus

1. A delayed result for an old selection or connection must not replace a newer view or draft (Tasks 3, 5, 6).
2. Bulk and multi-step failures/cancellation must stop subsequent steps, retain successful earlier results and refresh partial changes (Tasks 2, 4, 5).
3. Temporary XML must survive until the subprocess exits and disappear after success, failure, cancellation or worker failure (Tasks 2, 5). Exception: if an editor stops after changing the file, or the edited file cannot safely be read into memory, retain that private file for recovery and report its path; deleting it would discard the user's edits. See `docs/DECISIONS.md` and `book/src/safety.md`.
4. Output floods, a full result channel or a panicked worker must not freeze input, leak processes or leave the app permanently busy (Tasks 2, 3, 7).
5. Quit, OS signals and foreground editors must respect process ownership and terminal restoration; stopping a client must not be reported as a rollback of libvirt work (Tasks 2, 7, 8).

## Evidence and scope

The existing note under **Review follow-up (2026-10-06, later)** in DECISIONS explicitly identifies awaited actions in key handling and recommends background execution with message-line progress.

The code has the following call chain:

```text
app::run / tokio::select!
  -> app.on_key(key).await
     -> run_confirmed / area handler / run_ex_line
        -> command::exec::execute(...).await or virsh::exec::run(...).await
           -> subprocess completion (30 s or up to 24 h)
```

While a selected branch awaits its handler, `app::run` does not return to the next `select!` or draw. Tokio can schedule other tasks, but that does not make this event loop consume input or results. Adding threads, increasing timeouts or changing the keyboard poll interval does not address this dependency.

| Path | Current problem | Planned treatment |
|---|---|---|
| `src/app/mod.rs`: `run`, `on_key`, `run_confirmed` | Event branch waits for the entire operation and refresh | Submit job, return immediately, handle results in another branch |
| `src/app/domains.rs`: `run_plan_on_targets` | Sequential targets borrow UI/history until completion | Owned ordered batch; per-target completion events |
| `src/app/ex.rs`: `run_ex_line` | Raw virsh bypasses plan execution; stdout is user-visible | Common supervised process runner; preserve stdout and confirmation routing |
| `src/app/detail.rs` | Snapshot/hardware plans, XML preparation, detail loads and refreshes await I/O | Owned command workflows plus separate read requests |
| `src/app/networks.rs`, `storage.rs` | Mutations and selection/picker loads await I/O | Submit jobs; load selections asynchronously |
| `src/app/wizard.rs` | Install, print-XML and define await I/O; `print_xml` has no timeout wrapper | Supervised staged workflow with retained draft and temp-file ownership |
| `src/app/mod.rs`: completion; `src/app/palette.rs` | Blocking `load_index`/`load_doc` call `run_blocking` | Async cached help requests; no synchronous subprocess in input handling |
| `src/app/mod.rs`: initial domain fetch | First frame waits for `list_domains` | Empty/loading state, then poller data |
| `src/app/poller.rs`, `live.rs` | Existing background-task pattern already works; view loads are tagged only by URI | Reuse it; strengthen request identity and refresh invalidation |
| `src/app/external.rs`, `src/main.rs` | Terminal handoff is intentionally foreground; signal handler calls `process::exit` | Retain intentional suspension; coordinate normal shutdown with job cleanup |

Investigation is based on source inspection and the documented limitation. Task 1 must demonstrate the failure with a fake backend before the implementation changes the execution path. Do not reproduce using migration or wipe on a real guest/volume.

## Proposed design

### Alternatives

- **Recommended: owned jobs and result messages.** Fits the existing poller and the original §3.1 architecture. Requires separating command preparation/execution from UI updates, but permits deterministic tests and explicit process ownership.
- **Spawn each existing handler behind a shared `App` mutex.** Smaller initial patch, but handlers hold mutable state across awaits; drawing or input would wait for the same lock. Cloning whole views instead risks overwriting newer state. Reject.
- **Rewrite the whole app as a generic effect/reducer framework.** Aligns with the original architecture, but expands the fix into unrelated input, config and rendering refactors. Defer; use narrow job/read boundaries now.

### Job admission and state

- Scope excludes concurrent mutation scheduling, persistent jobs across app restarts, a new jobs screen and percentage-progress estimation. These are not needed to fix input starvation.
- One active user command workflow for the whole app. A bulk command or hardware/network/wizard sequence is one job. Targets and steps execute in their existing order; the first failure or cancellation stops the remainder.
- Capture URI, target names, confirmed plans/options, demo/dry-run mode and originating view/draft identity at submission. Never rebuild a running plan from the current selection or mutable global settings.
- States: `Idle -> Running -> Cancelling -> Finished`, with success, failure, timeout, cancellation and worker failure represented distinctly. Preparation that can wait belongs to `Running` too.
- Additional command submissions, repeat (`.`), reconnect and foreground terminal handoff are rejected while busy with `⚠ Job #<id> is running; wait or press C-c to cancel`. Do not silently queue them. Allow navigation, filtering, help, palette browsing, settings display, redraw and read-only refresh requests.
- Raw `:` commands take the same exclusive command slot even if they look read-only: arbitrary virsh syntax should not require a new classifier to guarantee safe scheduling. Background read requests are a separate typed API.
- `:connect` and connection-changing settings use the same admission check. Gate submission centrally, including actions whose confirmation is disabled. Draft editing may continue, but completing a previous submission must not clear subsequent edits.
- The poller's existing opt-in balloon-stat setting remains explicit and outside this user-job slot. Do not introduce new automatic mutations or let a command job hold up sampling.

### Ownership and interfaces

Add `src/app/jobs.rs` for app-specific requests, active state and result application support. Add `src/command/job.rs` for the ordered batch runner and `src/command/process.rs` for subprocess lifetime, output and cancellation. Add `src/app/requests.rs` for on-demand reads and their identity checks.

The interface contract is:

```rust
// app/jobs.rs (crate-private)
struct JobId(u64);
struct JobContext {
    id: JobId,
    uri: String,
    connection_epoch: u64,
    origin: JobOrigin,
    dry_run: bool,
    demo: bool,
}
// JobOrigin identifies view, stable target and optional draft revision.
// JobRequest owns the context fields except id + a typed JobWork payload.
// submit_job allocates id and constructs JobContext after admission.
// No request borrows &mut App/view.
// JobWork covers Plans, RawVirsh, HardwareApply, NetworkCreate,
// WizardCreate, XmlDefine, Description, HostdevAttach and XmlExport.

// app/mod.rs
fn submit_job(&mut self, request: JobRequest) -> Result<JobId, SubmitError>;
fn on_job_event(&mut self, event: JobEvent);
fn cancel_job(&mut self);

// command/process.rs
async fn run_step(
    uri: &str,
    step: &CommandStep,
    control: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<StepOutput, RunError>;

// command/job.rs
async fn run_batch(
    batch: OwnedBatch,
    control: tokio::sync::watch::Receiver<bool>,
    events: tokio::sync::mpsc::Sender<CommandEvent>,
) -> BatchOutcome;
```

`OwnedBatch` owns the captured URI, demo/dry-run mode, ordered `(target, CommandPlan)` entries and `NamedTempFile` guards. `StepOutput` contains status, stdout, stderr and per-stream truncation flags. `CommandEvent` reports target/step start and completion, exact executed argv and sanitized display data. `BatchOutcome` retains completed steps/targets and the terminal result. `JobEvent` adds the captured `JobContext` and typed workflow result. `SubmitError` distinguishes busy/invalid context; `RunError` distinguishes spawn, command failure, timeout, cancellation and output-limit failure. Do not infer outcomes by parsing message prefixes.

Use a bounded event channel (capacity 64), a coalescing cancellation watch channel and a retained task handle. Progress cannot block cancellation; final outcome must also be recoverable from the task handle if delivery fails. `App` is the sole writer of view state, command history and synthetic events. A job owner must clean up its supervised subprocess on drop/error; merely dropping a Tokio join handle is not cleanup.

### Progress, history and refresh

- Render activity in the existing message line across views: `⠋ Job #<id> · <target> · step <i>/<n> · <elapsed>s · C-c cancel`. Use the existing blue token and the §6.4 spinner sequence `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`, updated every 100 ms only while busy. Before steps are known, show `preparing` instead of a step count. Use an ASCII spinner when configured.
- Show real target/step counts and elapsed time. Do not invent percentage progress or add libvirt polling commands solely to estimate progress.
- Keep editing/error feedback visible: transient feedback takes precedence for 2 s, then active-job status resumes. Keep all outcomes in `:messages` and Events. Completion is shown once even if the originating view has closed.
- Record each actually started step once in history; preserve `yc`, dry-run previews and sanitized raw-command stdout. Move synthetic event generation out of the `on_key` before/after-message-length heuristic so asynchronous completions are logged once.
- Request refresh after success, partial failure, timeout or cancellation that could have changed objects. Use `poller::Request::Now`/`RefetchConfig` and async area reloads; applying a job result must never await its refresh. Dry-run must not clear pending edits or pretend to apply changes.
- Attach connection epoch, request serial and stable selection/draft identity to read results. Reject obsolete results, including A -> B -> A URI changes, older refreshes arriving after a mutation and completion of a draft that has since changed. Coalesce repeated refresh requests; at most one in-flight load per request key with one pending refresh.

### Cancellation, exit and terminal handoff

- With a job active, keyboard `C-c` requests cancellation and keeps the TUI running. Repeated `C-c` is idempotent. Without a job, preserve current `C-c` quit behaviour.
- On Unix, signal the owned command process group with SIGINT; after 3 s, kill remaining owned processes and reap the child. Drain stdout/stderr concurrently so a full pipe cannot deadlock completion. Never signal a libvirt daemon or unrelated process.
- Preserve `timeout_for`: 30 s for ordinary virsh calls, 24 h for the existing long-job/program list. Timeouts kill/reap the owned process. This intentionally retains the later DECISIONS policy over §6.4's original unlimited timeout; do not change duration policy in this fix.
- Retain up to 1 MiB per stream while continuing to drain pipes. Raw output may be displayed with an explicit truncation notice. Structured XML output that exceeds the limit must fail preparation and must never be used to define an object. Test both behaviours.
- While a job runs, actual quit requests (`q` where it quits, `ZZ`, `:q`, `:qa`) show `⚠ Job #<id> is running; press C-c to cancel before quitting`. Preserve `q` as Back or text input where it already has that meaning. Do not detach a job or silently cancel on navigation.
- Route OS SIGINT/SIGTERM to coordinated shutdown: stop accepting jobs, cancel the active one, finish bounded process cleanup, then restore the terminal and return the appropriate exit status. Remove the normal signal path's immediate `process::exit`, which skips destructors. Retain panic terminal restoration and a best-effort owned-process kill fallback; SIGKILL cannot be handled.
- Read loaders and the poller also need retained cancellation/task ownership on shutdown. Cancel their supervised queries and reap children; do not wait for their normal 30 s deadlines or leave detached load tasks alive.
- A stopped local client is not proof that a remote/libvirt operation stopped or rolled back. Cancellation text: `⚠ Job #<id> stopped locally; verify the object state`. Refresh and retain already completed steps; do not retry mutations automatically after disconnects or uncertain outcomes.
- `$EDITOR`, console and existing foreground terminal commands remain intentional handoffs. Allow them only when no command job is active, using the current input pause and full-redraw protocol. The suspended-terminal wait must also observe OS shutdown intent and clean up its owned child; routing signals solely to a UI branch that is not being polled would lose shutdown handling. Slow preparation before handoff and backend validation/define after it run as jobs/reads. Preserve the XML validation-error/reopen workflow without stealing focus from a different view.

## Implementation tasks (in order)

### Task 1: Capture the freeze with an isolated regression harness

**Files:** Create `src/app/tests.rs`, `tests/support/slow_command.py`, `tests/job_process.rs`; modify `src/app/mod.rs` to provide test construction/event-loop injection without changing production behaviour.

**Interfaces:** A test `App` with fixture data, an injectable command runner and injectable input/live/load events. Keep `App` private; put its tests in a child module. The helper process supports start/release markers, stdout/stderr floods, chosen exit status and SIGINT handling without invoking virsh.

- [ ] Add `slow_command_does_not_hold_input`: hold a fake command on a release barrier, inject `j`, view navigation, redraw and resize, and assert their state/frame changes before releasing it. Add a live sample and assert it is applied while the command is still pending.
- [ ] Run `cargo test --lib app::tests::slow_command_does_not_hold_input -- --nocapture`; confirm failure is the awaited-handler path, using a bounded 1 s watchdog rather than a hanging test. Retain this test for Task 3 to turn green.
- [x] Add helper lifecycle tests in `tests/job_process.rs` using per-child executable paths/env. Use temporary directories; never mutate process-wide PATH or demo flags in parallel tests. Prove release and cleanup controls work.
- [ ] Run `cargo test --test job_process`; all harness tests pass. Keep the intentionally failing regression local until Task 3; do not commit a red branch independently.

### Task 2: Build a supervised cancellable process and batch runner

**Files:** Create `src/command/process.rs`, `src/command/job.rs`; modify `src/command/mod.rs`, `src/command/exec.rs`, `src/backend/virsh/exec.rs`, `Cargo.toml`/`Cargo.lock` only if the Unix signal dependency is necessary; extend `tests/job_process.rs` and `tests/p4_exec.rs`.

**Interfaces:** Implement `run_step` and `run_batch` above. Keep the existing `execute(uri, plan, dry_run) -> Result<String>` wrapper for compatibility, delegating to the new runner. Keep backend `run` returning stdout while sharing supervised process lifetime. Pass execution mode explicitly in new jobs and retain the existing demo safety net.

- [ ] Write failing tests for sequential steps/targets, stop-on-first-failure, spawn failure, captured URI/argv/LC_ALL, null stdin, cancellation before spawn/between steps/during output, timeout, and child reaping. Assert no later marker is created after failure/cancellation.
- [ ] Add a temp-XML test asserting the file exists when the helper opens it and is removed on every terminal outcome. Add simultaneous stdout/stderr flood tests, structured-output overflow and sanitized display output. Fill the event channel and assert cancellation still completes.
- [ ] Run `cargo test --test job_process --test p4_exec` and record the expected missing-behaviour failures.
- [x] Implement supervision with a process-group guard, concurrently drained streams, injected deadlines for tests and cancellation-aware event delivery. Preserve the production 30 s/24 h defaults; use test deadlines instead of real long waits.
- [ ] Re-run the same tests; require PASS. Include these changes in the first green checkpoint after Task 3.

### Task 3: Integrate jobs with input, rendering and result application

**Files:** Create `src/app/jobs.rs`; modify `src/app/mod.rs`, `src/app/render.rs`, `src/ui/chrome.rs`, `src/app/domains.rs`; extend `src/app/tests.rs`. Create `tests/job_render.rs` for busy-state rendering.

**Interfaces:** Implement `submit_job`, `on_job_event`, `cancel_job`, `JobRequest`, `JobContext`, `JobOrigin`, `JobEvent` and active-state/task ownership. Add a jobs result branch and conditional 100 ms animation branch to `run`. Migrate one lifecycle path (`start`) end-to-end first; legacy paths are migrated next.

- [ ] Add tests that duplicate submissions are rejected, job identity/URI stays fixed while navigating, a dropped result sender or panicked task releases busy state, and late completion does not target the current selection. Add exact-once history/event assertions.
- [ ] Run `cargo test --lib app::tests` and `cargo test --test job_render`; confirm the new cases fail before integration.
- [x] Submit the start operation synchronously and return to `select!`. Apply results on `App`, draw progress using theme tokens, and schedule refresh without awaiting it. Bound result draining per loop iteration so input is still serviced during bursts.
- [ ] Re-run those tests, including Task 1's held-command regression. Snapshot busy/preparing/cancelling/success/failure states, ASCII mode and narrow widths; existing idle snapshots must remain unchanged.
- [ ] Run the full quality gates and commit the first working checkpoint with an English message describing responsive lifecycle execution. Tasks 1–3 form this checkpoint.

### Task 4: Migrate all ordinary command entry points

**Files:** Modify `src/app/domains.rs`, `ex.rs`, `networks.rs`, `storage.rs`, `detail.rs`, `palette.rs`, `mod.rs`, `jobs.rs`; extend `src/app/tests.rs`, `tests/p4_input_command.rs`, `tests/review_regressions.rs`.

**Interfaces:** Use `submit_job(JobRequest)` for lifecycle/bulk, generic confirmations, raw `:`, palette execution, snapshots, network/pool/volume plans and media changes. Reuse the runner's structured outcomes and immutable captured plans.

- [x] Add a table of action-route tests covering keyboard, `:`, palette, repeat and confirmations enabled/disabled. Hold each submitted job and assert navigation still works; a second submission starts no process.
- [x] Add bulk tests with targets A/B/C: B fails, A remains successful, C never starts, and refresh/history/events reflect partial completion. Add snapshot safety-step failure and raw-command multiline stdout/confirmation tests. Preserve shutdown-request tracking until the guest actually stops.
- [ ] Run `cargo test --lib app::tests --test p4_input_command --test review_regressions`; observe failures for unmigrated routes.
- [x] Replace awaited execution and inline refresh with job submission/result handling. Ensure admission occurs before side-effecting preparation. Preserve current per-target plan building, ordering and confirmation text.
- [ ] Re-run the targeted tests and full quality gates; commit the migration checkpoint. Complex preparation paths remain explicitly covered by Task 5.

### Task 5: Move owned preparation workflows and draft reconciliation

**Files:** Modify `src/app/jobs.rs`, `detail.rs`, `wizard.rs`, `networks.rs`, `storage.rs`, `domains.rs`, `external.rs`, `src/command/tempxml.rs` as needed; extend `src/app/tests.rs`, `tests/job_process.rs`, `tests/p7_hardware.rs`, `tests/p11_wizard.rs`.

**Interfaces:** Implement the typed `JobWork` workflows defined above. Their requests own source XML/form data, revisions and temp guards; their results contain produced data and completed-step records. Keep backend preparation separate from terminal-handoff requests.

- [x] Add delayed preparation tests for inactive XML, host-device XML, wizard print-XML and XML export. Assert the UI processes input during preparation, no define follows failed/cancelled/oversized preparation, and export never overwrites an existing file.
- [x] Add tests for hardware define followed by failing live step, partial network creation, wizard create/define-only/viewer-after-success, closing a submitted wizard and editing a new draft while an older submission completes. Assert newer draft content is retained and partial failures trigger read-back without pretending rollback.
- [ ] Run `cargo test --lib app::tests --test job_process --test p7_hardware --test p11_wizard`; confirm failures before migrating these flows.
- [x] Move temp guards and workflow inputs into the worker. Keep demo/dry-run preparation free of mutation and external execution. Preserve validation failures and XML-editor retry state, and apply draft completion only to the submitted revision.
- [ ] Re-run targeted tests and full quality gates; commit the workflow checkpoint.

### Task 6: Remove remaining backend waits from interactive reads

**Files:** Create `src/app/requests.rs`; modify `src/app/mod.rs`, `live.rs`, `poller.rs`, `detail.rs`, `networks.rs`, `storage.rs`, `palette.rs`, `domains.rs`, `ex.rs`, `src/backend/virsh/parse_help.rs`; extend `src/app/tests.rs`.

**Interfaces:** `ReadKey` identifies resource type and stable target; `ReadContext` carries URI, connection epoch, serial and relevant selection/draft revision. `request_read(&mut self, request: ReadRequest)` submits/coalesces work; `on_read_result(&mut self, result: ReadResult)` applies only matching results. Provide async `fetch_doc(uri: &str, cmd: &str) -> Option<HelpDoc>` alongside `fetch_index`; hot-path completion only reads caches.

- [x] Add delayed-read cases for startup, detail/snapshots, network selection/leases, pool volumes, ISO picker, viewer lookup, explicit refresh, palette and completion cache miss. Assert input is serviced before any delayed response arrives.
- [x] Add out-of-order selection, A -> B -> A connection, edited completion-line, draft revision, and pre-mutation stale-refresh tests. Repeated refresh requests must not create an unbounded number of tasks.
- [ ] Run `cargo test --lib app::tests`; confirm missing read scheduling/identity behaviour fails.
- [x] Implement typed async requests and cache loading. Show loading state immediately; apply completion results only if the input/revision still matches. Load initial domains via the poller after first draw. Invalidate older requests when mutations complete.
- [x] Make normal `on_key`/`on_tick` dispatch synchronous once backend work is extracted. Audit reachable `run_blocking`, `.output()`, `.status()` and `.await` calls; only the explicit foreground terminal-handoff path may wait for a process on the UI task. Do not use `spawn_blocking` to hide an unbounded subprocess wait.
- [ ] Re-run tests and full gates; commit the interactive-read checkpoint.

### Task 7: Complete cancellation, quit and signal coordination

**Files:** Modify `src/main.rs`, `src/app/mod.rs`, `jobs.rs`, `external.rs`, `src/command/process.rs`; extend `src/app/tests.rs` and `tests/job_process.rs`.

**Interfaces:** Route terminal quit requests through active-job admission. Deliver OS shutdown intent through a channel to the app; reuse the cancellation control and retained task handle. Separate keyboard cancellation from OS-requested application shutdown.

- [x] Add tests for `C-c` while busy/idle, repeated cancellation, `q` as Back/text/quit, `ZZ`, `:q`, `:qa`, reconnect through both command/settings paths, and terminal handoff admission.
- [x] Add helper-process tests for a cooperative SIGINT exit, an ignored SIGINT requiring kill after 3 s, child/grandchild cleanup and cancellation racing successful exit. Test OS signal shutdown during jobs, pending reads and foreground terminal handoff in a separate process so the test runner is never signalled.
- [ ] Run `cargo test --lib app::tests --test job_process`; confirm lifecycle cases fail before wiring policy.
- [x] Implement the documented key/exit policy, coordinated signal handling and bounded cleanup. Preserve terminal handoff input ownership; after editor return, async validation must not block the TUI or reopen an editor over an unrelated view.
- [ ] Re-run tests and quality gates; commit the lifecycle checkpoint.

### Task 8: Acceptance, documentation and release evidence

**Files:** Update `docs/PLAN.md` §6.4/P15, `docs/DESIGN.md`, `docs/DECISIONS.md`, `docs/FIDELITY.md`, `book/src/troubleshooting.md`, `book/src/keys.md`, `book/src/safety.md`, `book/src/command-line.md`, `CHANGELOG.md`; update `scripts/screenshots.py` only if a deterministic busy-demo fixture is required.

**Interfaces:** Document the implemented contracts and exact UI strings from this plan. Keep limitations explicit until the complete route inventory is green.

- [x] Exercise a fake command held for at least 30 s in a CPR-capable terminal/tmux harness, with isolated config/cache and a fake backend for every process. During it, navigate, filter, switch views, resize, receive a fake live sample, view help, cancel and quit. No real virsh mutations.
- [x] Acceptance: each scripted input/resize produces its expected state/frame within 250 ms on an unloaded local host; assert responsiveness before releasing the command, not just after it ends. Spinner advances independently of the monitoring interval. Automated tests use barriers and a generous 1 s watchdog to avoid CI timing flakes.
- [x] Check error/timeout/disconnect/partial success, all entry routes, stale-result rejection, no late execution of refused commands, preserved temporary-file lifetime, exact-once logs and normal/signal terminal restoration. Verify `--demo` and `--dry-run` cannot spawn mutation workflows. Evidence and the editor-recovery exception are recorded in `2026-11-11-responsive-jobs-evidence.md`.
- [x] Update the book for responsive navigation, one active job, cancellation semantics, busy quit/reconnect behaviour and possible server-side continuation. Replace the old limitation note only after acceptance; retain the historical diagnosis and document the 24 h timeout decision.
- [x] Run `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`, `cargo build --release`, and `mdbook build book`. Review snapshot diffs individually; generate any screenshots with demo mode only. Record unavailable optional tooling explicitly.
- [x] Mark P15 complete only when the entire route inventory and criteria pass. Commit with `phase 15: keep the UI responsive during long operations`.

## Definition of done

The held-command test proves that the real app loop continues to process input, resize, redraw, live samples and job results. All noninteractive backend work reached from the UI is scheduled off that loop. Partial results, cancellation, process lifetime, drafts and connection identity have regression coverage. Idle visuals and existing safety behaviour remain intact, the quality gates pass, and the book describes the shipped behaviour.

The unchecked steps that call for observing historical pre-implementation failures
or creating intermediate commits were not reconstructed after implementation.
The final evidence and known platform limits are recorded in the companion
evidence document. The editor-recovery exception above preserves user edits.
