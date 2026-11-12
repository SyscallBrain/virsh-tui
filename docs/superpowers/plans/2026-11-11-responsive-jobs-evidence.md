# Responsive jobs: route evidence

All process and terminal tests use fake backends. No libvirt object is mutated.
The real-PTY route cases live in `tests/support/responsive_routes.py` and run
through `command_routes_keep_real_ui_responsive_while_jobs_are_held`.

| Route or workflow | Evidence |
| --- | --- |
| Lifecycle keyboard, `:`, palette, repeat and confirmation on/off | PTY cases `keyboard`, `command`, `palette`, `repeat`, `confirm`, `no-confirm`; app tests `lifecycle_actions_capture_expected_commands_for_sorted_bulk_targets` and `partial_batch_records_success_before_failure` |
| Network actions and network creation | PTY cases `network`, `network-create`; `network_workflow_stops_after_start_failure_and_removes_xml`; draft restoration tests in `app::tests` |
| Storage pool and volume actions | PTY cases `pool`, `volume`; captured pool plans and confirmation routes in `app::tests` |
| Snapshots and ISO media | PTY cases `snapshot`, `media`; `failed_snapshot_safety_step_prevents_revert` and media workflow tests in `job_process` |
| Wizard create, define and editor | App tests for captured create/define/viewer and newer drafts; `wizard_define_owns_xml_until_define_finishes`, cancellation/overflow tests, and `delayed_wizard_xml_keeps_draft_editable_and_does_not_open_stale_editor` |
| Hardware apply and XML editor | `hardware_apply_keeps_xml_through_define_and_records_partial_failure`, readback/draft reconciliation tests in `app::tests`, and the three editor retry PTY tests |
| Description, host-device attach and XML export | Workflow tests in `job_process` and delayed PTY cases `description-prep`, `hostdev-prep`, `export-prep` |
| Startup, detail, selection, picker, viewer, help and refresh reads | Ten delayed PTY cases in `delayed_reads_keep_real_ui_loop_responsive`; stale identity, connection reuse and coalescing tests in `app::tests` |
| Completion after selection change, disconnect, cancellation and busy admission | PTY cases `completion`, `disconnect` and all held route cases; `cancelling_second_target_keeps_first_and_never_starts_third`; timeout and partial-batch result tests in `app::tests` |
| Terminal handoff, signals and process descendants | `signals_stop_held_job_and_read_and_restore_terminal`, `signal_during_foreground_editor_restores_terminal_and_stops_descendants`, and process-group tests in `job_process` and `backend::virsh::events` |
| Demo and dry-run | `preview_cli_modes_do_not_execute_command_workflows` exercises the real executable with fake programs; `preview_modes_do_not_schedule_ex_command_workflows`, `preview_batch_modes_do_not_spawn_preparation_or_mutation_steps`, and the isolated global-demo safety-net test cover internal admission |

The tests establish representative routing and shared batch behavior; they do not
individually exercise every possible virsh subcommand. The XML editor retains a
private recovery file when its changed content cannot safely be recovered in
memory. This deliberate exception to the plan's unconditional temporary-file
removal rule is recorded in `docs/DECISIONS.md` and `book/src/safety.md`.
Linux PTY execution has been tested; macOS, FreeBSD and NetBSD have been
cross-compiled but have not been run on those platforms.
