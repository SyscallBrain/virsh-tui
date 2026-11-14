//! User command job ownership and result application.

use super::*;
use crate::command::{
    job::{BatchOutcome, BatchResult, CommandEvent, OwnedBatch, run_batch},
    plan::CommandPlan,
};

#[derive(Clone)]
pub(super) struct EditorDraft {
    pub uri: String,
    pub epoch: u64,
    pub target: String,
    pub xml: String,
    pub network: bool,
    pub wizard: Option<(u64, String)>,
    pub generation: u64,
    /// The object XML this draft was made from: a retained draft is only
    /// reopened while the object still has exactly this XML.
    pub base: String,
}

pub(super) struct ActiveJob {
    pub id: u64,
    pub uri: String,
    pub origin: View,
    pub started: std::time::Instant,
    pub plans: Vec<(String, CommandPlan)>,
    pub cancel: tokio::sync::watch::Sender<bool>,
    pub events: tokio::sync::mpsc::Receiver<CommandEvent>,
    pub progress: Option<CommandEvent>,
    pub raw: bool,
    pub submitted_network: Option<crate::ui::widgets::form::Form>,
    pub submitted_wizard: Option<(u64, String)>,
    pub viewer_after: Option<String>,
    pub snapshot_domain: Option<String>,
    pub refresh_storage: bool,
    pub hardware_snapshot: Option<(String, String)>,
    pub editor_draft: Option<EditorDraft>,
    pub started_ledger: crate::command::job::StartedLedger,
    pub task: tokio::task::JoinHandle<BatchOutcome>,
}

impl Drop for ActiveJob {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        self.task.abort();
    }
}

impl ActiveJob {
    /// A job with no workflow extras (callers set the ones they need).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        id: u64,
        uri: String,
        origin: View,
        plans: Vec<(String, CommandPlan)>,
        cancel: tokio::sync::watch::Sender<bool>,
        events: tokio::sync::mpsc::Receiver<CommandEvent>,
        started_ledger: crate::command::job::StartedLedger,
        task: tokio::task::JoinHandle<BatchOutcome>,
    ) -> Self {
        Self {
            id,
            uri,
            origin,
            started: std::time::Instant::now(),
            plans,
            cancel,
            events,
            progress: None,
            raw: false,
            submitted_network: None,
            submitted_wizard: None,
            viewer_after: None,
            snapshot_domain: None,
            refresh_storage: false,
            hardware_snapshot: None,
            editor_draft: None,
            started_ledger,
            task,
        }
    }
}

impl App {
    /// One job at a time: say so (and flash the job line) when busy.
    /// Refuse `what` on a read-only connection, saying why. True: refused.
    pub(super) fn refuse_if_read_only(&mut self, what: &str) -> bool {
        let Some(reason) = crate::config::read_only_reason() else {
            return false;
        };
        self.show_message(format!("✗ read-only connection ({reason}): {what} not run"));
        true
    }

    /// Refuse plans with a step that may change something (read-only only).
    pub(super) fn refuse_writes_if_read_only(&mut self, plans: &[(String, CommandPlan)]) -> bool {
        if !crate::config::read_only() {
            return false;
        }
        let Some(step) = plans
            .iter()
            .find_map(|(_, plan)| crate::command::readonly::first_write(plan))
        else {
            return false;
        };
        let what = crate::command::display::display(step);
        self.refuse_if_read_only(&what)
    }

    pub(super) fn refuse_if_busy(&mut self) -> bool {
        let Some(job) = &self.active_job else {
            return false;
        };
        self.show_message(format!(
            "⚠ Job #{} is running; wait or press C-c to cancel",
            job.id
        ));
        self.busy_feedback = Some(std::time::Instant::now());
        true
    }

    pub(super) fn store_editor_draft(&mut self, draft: EditorDraft) {
        let existing = self.editor_retries.iter().position(|current| {
            current.uri == draft.uri
                && current.epoch == draft.epoch
                && current.target == draft.target
                && current.network == draft.network
                && current.wizard == draft.wizard
        });
        if let Some(index) = existing {
            if self.editor_retries[index].generation <= draft.generation {
                self.editor_retries[index] = draft;
            }
        } else {
            self.editor_retries.push(draft);
        }
    }

    pub(super) fn begin_domain_editor(&mut self) {
        if self.refuse_if_read_only("edit XML (the XML tab shows it)") {
            return;
        }
        let Some(detail) = self.detail.as_ref() else {
            return;
        };
        let target = detail.domain.clone();
        // Always re-read the domain: a retained draft is only reused when the
        // domain did not change since (see `take_retained_domain_draft`).
        self.request_domain_editor(target);
    }

    /// A retained draft for `target` on this connection, if the domain still
    /// has the XML it was made from. A stale draft is saved to a file (its
    /// path is reported) instead of being defined over newer changes.
    pub(super) fn take_retained_domain_draft(&mut self, target: &str, fresh: &str) -> Option<EditorDraft> {
        let index = self.editor_retries.iter().position(|draft| {
            !draft.network && draft.wizard.is_none() && draft.uri == self.uri && draft.target == target
        })?;
        let mut draft = self.editor_retries.remove(index);
        if draft.base == fresh {
            draft.epoch = self.connection_epoch;
            return Some(draft);
        }
        match crate::command::tempxml::write("stale", &draft.xml).and_then(|file| {
            file.keep()
                .map(|(_, path)| path)
                .map_err(|e| color_eyre::eyre::eyre!("{e}"))
        }) {
            Ok(path) => self.show_message(format!(
                "⚠ {target} changed since your unsaved XML draft; the draft was saved to {}",
                path.display()
            )),
            Err(_) => self.show_message(format!(
                "⚠ {target} changed since your unsaved XML draft; it was discarded"
            )),
        }
        None
    }

    pub(super) fn open_editor_draft(&mut self, mut draft: EditorDraft) {
        if external::shutdown_requested() {
            self.store_editor_draft(draft);
            return;
        }
        if draft.uri != self.uri || draft.epoch != self.connection_epoch || self.active_job.is_some() {
            self.store_editor_draft(draft);
            self.show_message("⚠ XML draft retained; return to its connection to edit it".into());
            return;
        }
        let temporary = match crate::command::tempxml::write("edit", &draft.xml) {
            Ok(file) => file,
            Err(error) => {
                self.store_editor_draft(draft);
                self.show_message(format!("✗ Cannot prepare XML editor: {error}"));
                return;
            }
        };
        let outcome = external::edit_file_status(temporary.path());
        if outcome != external::Foreground::Success || external::shutdown_requested() {
            let changed = std::fs::metadata(temporary.path()).ok().is_none_or(|meta| {
                meta.len() > 1024 * 1024
                    || std::fs::read(temporary.path())
                        .ok()
                        .is_none_or(|bytes| bytes != draft.xml.as_bytes())
            });
            let retained = if changed {
                temporary.keep().ok().map(|(_, path)| path)
            } else {
                None
            };
            self.store_editor_draft(draft);
            if let Some(path) = retained {
                tracing::warn!(path = %path.display(), "editor stopped; XML retained");
                self.show_message(format!("⚠ Editor stopped; XML retained at {}", path.display()));
            } else if !external::shutdown_requested() {
                let editor = external::editor_argv().join(" ");
                self.show_message(match outcome {
                    external::Foreground::SpawnFailed(e) => {
                        format!("✗ cannot start the editor ({e}); set $EDITOR or Settings › editor")
                    }
                    external::Foreground::Failed(Some(code)) => {
                        format!("✗ editor `{editor}` exited with status {code}; nothing changed")
                    }
                    _ => format!("✗ editor `{editor}` was interrupted; nothing changed"),
                });
            }
            return;
        }
        use std::io::Read;
        let mut bytes = Vec::new();
        let read = std::fs::File::open(temporary.path())
            .and_then(|file| file.take(1024 * 1024 + 1).read_to_end(&mut bytes));
        if read.is_err() || bytes.len() > 1024 * 1024 || std::str::from_utf8(&bytes).is_err() {
            let retained = temporary.keep().ok().map(|(_, path)| path);
            self.store_editor_draft(draft);
            self.show_message(match retained {
                Some(path) => format!("✗ Cannot use edited XML; file retained at {}", path.display()),
                None => "✗ Cannot read edited XML".into(),
            });
            return;
        }
        let edited = String::from_utf8_lossy(&bytes).into_owned();
        // Quitting without changes abandons the draft. For the wizard, an
        // unchanged first draft means "create as generated", but an unchanged
        // retry (it still carries the error comment) is not resubmitted:
        // that would fail again and reopen the editor forever.
        let retry = edited.starts_with("<!-- virsh-tui edit error:");
        if edited == draft.xml && (draft.wizard.is_none() || retry) {
            self.show_message(if draft.wizard.is_some() {
                "no changes to XML; fix the wizard fields or edit the XML again".into()
            } else {
                "no changes to XML".into()
            });
            return;
        }
        draft.xml = edited;
        if !self.submit_xml_editor(draft.clone()) {
            self.store_editor_draft(draft);
        }
    }

    pub(super) fn editor_context_is_current(&self, draft: &EditorDraft) -> bool {
        if draft.uri != self.uri
            || draft.epoch != self.connection_epoch
            || self.active_job.is_some()
            || self.ex_line.is_some()
            || self.palette.is_some()
            || (self.wizard.is_some() && draft.wizard.is_none())
            || self.settings.is_some()
            || self.confirm.is_some()
            || self.help_filter.is_some()
            || self.store.picker.is_some()
            || external::shutdown_requested()
        {
            return false;
        }
        if let Some((revision, snapshot)) = &draft.wizard {
            *revision == self.wizard_revision
                && self
                    .wizard
                    .as_ref()
                    .map(|wizard| wizard.content_snapshot())
                    .as_ref()
                    == Some(snapshot)
        } else if draft.network {
            self.view == View::Networks
                && self.nets.new_form.is_none()
                && self.nets.selected_name() == draft.target
        } else {
            self.view == View::Detail
                && self.detail.as_ref().is_some_and(|detail| {
                    detail.domain == draft.target && detail.tab == ui::views::detail::DetailTab::Xml
                })
        }
    }

    pub(super) fn lifecycle_plans(
        &mut self,
        action: crate::input::engine::KeyAction,
    ) -> Option<Vec<(String, CommandPlan)>> {
        use crate::command::builders::lifecycle;
        use crate::input::engine::KeyAction as A;
        let selected = targets(&self.dash);
        let selected = if action == A::Shutdown {
            let (running, skipped): (Vec<_>, Vec<_>) = selected.into_iter().partition(|name| {
                self.dash
                    .rows
                    .iter()
                    .any(|row| &row.name == name && row.state == crate::model::DomainState::Running)
            });
            if running.is_empty() {
                self.dash.message = format!("✗ {} is not running", skipped.join(", "));
                self.messages.push(self.dash.message.clone());
            }
            running
        } else {
            selected
        };
        let mut plans = Vec::new();
        for target in selected {
            // Direction is decided per target, never from the cursor row.
            let (state, autostart) =
                row_state(&self.dash, &target).unwrap_or((crate::model::DomainState::Other, false));
            let plan = match action {
                A::Start => lifecycle::start(&target),
                A::Shutdown => lifecycle::shutdown(&target),
                A::Reboot => lifecycle::reboot(&target),
                A::Pause if state == crate::model::DomainState::Paused => lifecycle::resume(&target),
                A::Pause => lifecycle::pause(&target),
                A::ManagedSave => lifecycle::managed_save(&target),
                A::Autostart => lifecycle::autostart(&target, !autostart),
                A::SendKey => lifecycle::send_key(&target),
                _ => return None,
            };
            plans.push((target, plan));
        }
        if matches!(
            action,
            A::Start | A::Shutdown | A::Reboot | A::Pause | A::ManagedSave | A::Autostart | A::SendKey
        ) {
            Some(plans)
        } else {
            None
        }
    }

    pub(super) fn network_plans(
        &self,
        action: crate::input::engine::KeyAction,
    ) -> Option<Vec<(String, CommandPlan)>> {
        use crate::command::builders::network;
        use crate::input::engine::KeyAction as A;
        let name = self.nets.selected_name().to_string();
        if name.is_empty() && matches!(action, A::Start | A::Autostart | A::LeasePin | A::LeaseUnpin) {
            return Some(vec![]);
        }
        let plan = match action {
            A::Start => network::start(&name),
            A::Autostart => network::autostart(
                &name,
                !self
                    .nets
                    .networks
                    .get(self.nets.selected)
                    .is_some_and(|net| net.autostart),
            ),
            A::LeasePin => {
                let Some(lease) = self.nets.selected_lease() else {
                    return Some(vec![]);
                };
                let host = if lease.hostname == "—" {
                    ""
                } else {
                    &lease.hostname
                };
                network::pin_lease(&name, &lease.mac, host, &lease.ip)
            }
            A::LeaseUnpin => {
                let Some(lease) = self.nets.selected_lease() else {
                    return Some(vec![]);
                };
                network::unpin_lease(&name, &lease.mac)
            }
            _ => return None,
        };
        Some(vec![(name, plan)])
    }

    pub(super) fn storage_pool_plans(&self, key: char) -> Option<Vec<(String, CommandPlan)>> {
        use crate::command::builders::storage;
        let name = self.store.selected_pool().to_string();
        if name.is_empty() && matches!(key, 'b' | 's' | 'r' | 'a') {
            return Some(vec![]);
        }
        let plan = match key {
            'b' => storage::pool_build(&name),
            's' => storage::pool_start(&name),
            'r' => storage::pool_refresh(&name),
            'a' => storage::pool_autostart(
                &name,
                !self
                    .store
                    .pools
                    .get(self.store.pool_selected)
                    .is_some_and(|pool| pool.autostart),
            ),
            _ => return None,
        };
        Some(vec![(name, plan)])
    }

    pub(super) fn submit_plans(&mut self, plans: Vec<(String, CommandPlan)>) -> bool {
        if self.refuse_writes_if_read_only(&plans) {
            return false;
        }
        if self.refuse_if_busy() {
            return false;
        }
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, receiver) = tokio::sync::mpsc::channel(64);
        let batch = OwnedBatch {
            uri: self.uri.clone(),
            demo: self.demo,
            dry_run: self.dry_run,
            plans: plans.clone(),
            temporary_files: vec![],
        };
        let (task, started_ledger) = crate::command::job::spawn_tracked(run_batch(batch, control, events));
        self.busy_feedback = None;
        self.active_job = Some(ActiveJob::new(
            id,
            self.uri.clone(),
            self.view,
            plans,
            cancel,
            receiver,
            started_ledger,
            task,
        ));
        true
    }

    pub(super) fn submit_network_create(&mut self, name: String, xml: String) -> bool {
        if self.refuse_if_read_only("network create") {
            return false;
        }
        if self.refuse_if_busy() {
            return false;
        }
        let draft = self.nets.new_form.take();
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, receiver) = tokio::sync::mpsc::channel(64);
        let uri = self.uri.clone();
        let (task, started_ledger) =
            crate::command::job::spawn_tracked(crate::command::job::run_network_create(
                uri.clone(),
                name.clone(),
                xml,
                "virsh".into(),
                control,
                events,
            ));
        let plan = CommandPlan {
            summary: format!("Created network {name}"),
            steps: vec![
                crate::command::plan::CommandStep::new("virsh", vec!["net-define", "<private-xml>"]),
                crate::command::plan::CommandStep::new("virsh", vec!["net-start", &name]),
                crate::command::plan::CommandStep::new("virsh", vec!["net-autostart", &name]),
            ],
        };
        self.busy_feedback = None;
        self.active_job = Some(ActiveJob::new(
            id,
            uri,
            self.view,
            vec![(name, plan)],
            cancel,
            receiver,
            started_ledger,
            task,
        ));
        if let Some(job) = self.active_job.as_mut() {
            job.submitted_network = draft;
        }
        true
    }

    pub(super) fn submit_raw(&mut self, step: crate::command::plan::CommandStep) -> bool {
        let target = step.argv.get(1).cloned().unwrap_or_default();
        let plan = CommandPlan {
            summary: "done".into(),
            steps: vec![step],
        };
        if !self.submit_plans(vec![(target, plan)]) {
            return false;
        }
        if let Some(job) = self.active_job.as_mut() {
            job.raw = true;
        }
        true
    }

    pub(super) fn submit_wizard(&mut self, draft: ui::overlays::wizard::WizardState) -> bool {
        if self.refuse_if_read_only("virt-install") {
            return false;
        }
        let mut plan = crate::command::builders::install::virt_install(&draft);
        plan.steps[0].argv.insert(0, self.uri.clone());
        plan.steps[0].argv.insert(0, "--connect".into());
        let snapshot = Some(draft.content_snapshot());
        let viewer_after = (draft.start_after && draft.open_console).then(|| draft.name.clone());
        if draft.start_after {
            if !self.submit_plans(vec![(draft.name, plan)]) {
                return false;
            }
        } else {
            if let Some(job) = &self.active_job {
                self.show_message(format!(
                    "⚠ Job #{} is running; wait or press C-c to cancel",
                    job.id
                ));
                self.busy_feedback = Some(std::time::Instant::now());
                return false;
            }
            // One persistent phase, never the install phase alone (issue #5).
            plan = crate::command::builders::install::define_xml(&draft);
            plan.steps[0].argv.insert(0, self.uri.clone());
            plan.steps[0].argv.insert(0, "--connect".into());
            plan.steps.push(crate::command::plan::CommandStep::new(
                "virsh",
                vec!["define", "<private-xml>", "--validate"],
            ));
            plan.summary = format!("Defined {} (not started)", draft.name);
            let id = self.next_job_id;
            self.next_job_id += 1;
            let (cancel, control) = tokio::sync::watch::channel(false);
            let (events, receiver) = tokio::sync::mpsc::channel(64);
            let (task, started_ledger) =
                crate::command::job::spawn_tracked(crate::command::job::run_wizard_define(
                    self.uri.clone(),
                    draft.name.clone(),
                    plan.steps[0].clone(),
                    "virsh".into(),
                    control,
                    events,
                ));
            self.busy_feedback = None;
            self.active_job = Some(ActiveJob::new(
                id,
                self.uri.clone(),
                self.view,
                vec![(draft.name, plan)],
                cancel,
                receiver,
                started_ledger,
                task,
            ));
        }
        if let Some(job) = self.active_job.as_mut() {
            job.submitted_wizard = snapshot.map(|snapshot| (self.wizard_revision, snapshot));
            job.viewer_after = viewer_after;
        }
        true
    }

    pub(super) fn cancel_job(&mut self) {
        if let Some(job) = &self.active_job {
            let _ = job.cancel.send(true);
        }
    }

    pub(super) fn submit_snapshot(&mut self, key: KeyEvent) -> bool {
        use ui::views::detail::SnapModal;
        if !key.modifiers.is_empty() {
            return false;
        }
        let Some(detail) = self.detail.as_ref() else {
            return false;
        };
        if detail.tab != ui::views::detail::DetailTab::Snapshots {
            return false;
        }
        if detail.snap_modal == SnapModal::Create
            && key.code == KeyCode::Enter
            && detail.snaps.create_name.trim().is_empty()
        {
            self.show_message("✗ snapshot name is required".into());
            return true;
        }
        if matches!(detail.snap_modal, SnapModal::Revert | SnapModal::Delete)
            && key.code == KeyCode::Char('y')
            && detail.snaps.selected_name().is_empty()
        {
            self.show_message("✗ no snapshot selected".into());
            return true;
        }
        let domain = detail.domain.clone();
        let plan = match (detail.snap_modal, key.code) {
            (SnapModal::Create, KeyCode::Enter) => crate::command::builders::snapshot::create(
                &domain,
                &detail.snaps.create_name,
                &detail.snaps.create_desc,
                detail.snaps.create_atomic,
                detail.snaps.create_quiesce,
            ),
            (SnapModal::Revert, KeyCode::Char('y')) => crate::command::builders::snapshot::revert(
                &domain,
                detail.snaps.selected_name(),
                detail.snaps.after.flag(),
                detail.snaps.force,
                detail.snaps.safety,
            ),
            (SnapModal::Delete, KeyCode::Char('y')) => crate::command::builders::snapshot::delete(
                &domain,
                detail.snaps.selected_name(),
                detail.snaps.del_children,
                detail.snaps.del_children_only,
                detail.snaps.del_metadata,
            ),
            _ => return false,
        };
        if self.submit_plans(vec![(domain.clone(), plan)]) {
            if let Some(job) = self.active_job.as_mut() {
                job.snapshot_domain = Some(domain);
            }
            if let Some(detail) = self.detail.as_mut() {
                detail.snap_modal = SnapModal::None;
            }
        }
        true
    }

    pub(super) fn submit_picker(&mut self, key: KeyEvent) -> bool {
        let insert = key.code == KeyCode::Enter && key.modifiers.is_empty();
        let eject = key.code == KeyCode::Char('e') && key.modifiers.contains(KeyModifiers::CONTROL);
        if (!insert && !eject) || self.picker_loading {
            return false;
        }
        let Some(picker) = self.store.picker.as_ref() else {
            return false;
        };
        let domain = picker.domain.clone();
        let path = if insert {
            let isos = state_isos(&self.store, picker);
            let Some(iso) = isos.get(picker.selected.min(isos.len().saturating_sub(1))) else {
                self.store.picker = None;
                self.show_message("no ISO selected".into());
                return true;
            };
            Some(iso.path.clone())
        } else {
            None
        };
        let plan = crate::command::builders::hardware::change_media(
            &domain,
            &picker.target,
            path.as_deref(),
            picker.live,
        );
        if self.submit_plans(vec![(domain, plan)]) {
            if let Some(job) = self.active_job.as_mut() {
                job.refresh_storage = true;
            }
            self.store.picker = None;
        }
        true
    }

    pub(super) fn submit_media_eject(&mut self) -> bool {
        if self.refuse_if_read_only("eject") {
            return false;
        }
        let (domain, running) = if self.view == View::Detail {
            match self.detail.as_ref() {
                Some(detail) => (
                    detail.domain.clone(),
                    detail.state == crate::model::DomainState::Running,
                ),
                None => return false,
            }
        } else {
            let selected = self.dash.selected();
            (
                selected.name.clone(),
                selected.state == crate::model::DomainState::Running,
            )
        };
        self.submit_media_change(domain, None, running)
    }

    pub(super) fn submit_description(&mut self, domain: String, description: String) -> bool {
        if self.refuse_if_read_only("description change") {
            return false;
        }
        if self.refuse_if_busy() {
            return false;
        }
        if domain.is_empty() {
            return false;
        }
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, receiver) = tokio::sync::mpsc::channel(64);
        let uri = self.uri.clone();
        let (task, started_ledger) =
            crate::command::job::spawn_tracked(crate::command::job::run_description_update(
                uri.clone(),
                domain.clone(),
                description,
                "virsh".into(),
                control,
                events,
            ));
        let plan = CommandPlan {
            summary: format!("Described {domain}"),
            steps: vec![
                crate::command::plan::CommandStep::new(
                    "virsh",
                    vec!["dumpxml", &domain, "--inactive", "--security-info"],
                ),
                crate::command::plan::CommandStep::new(
                    "virsh",
                    vec!["define", "<private-xml>", "--validate"],
                ),
            ],
        };
        self.busy_feedback = None;
        self.active_job = Some(ActiveJob::new(
            id,
            uri,
            self.view,
            vec![(domain, plan)],
            cancel,
            receiver,
            started_ledger,
            task,
        ));
        true
    }

    pub(super) fn submit_hostdev(&mut self, domain: String, node: String) -> bool {
        if self.refuse_if_read_only("attach-device") {
            return false;
        }
        if self.refuse_if_busy() {
            return false;
        }
        if domain.is_empty() || node.is_empty() {
            return false;
        }
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, receiver) = tokio::sync::mpsc::channel(64);
        let uri = self.uri.clone();
        let (task, started_ledger) =
            crate::command::job::spawn_tracked(crate::command::job::run_hostdev_attach(
                uri.clone(),
                domain.clone(),
                node.clone(),
                "virsh".into(),
                control,
                events,
            ));
        let plan = CommandPlan {
            summary: format!("Attached {node} on {domain}"),
            steps: vec![
                crate::command::plan::CommandStep::new("virsh", vec!["nodedev-dumpxml", &node]),
                crate::command::plan::CommandStep::new(
                    "virsh",
                    vec!["attach-device", &domain, "<private-xml>", "--config"],
                ),
            ],
        };
        self.busy_feedback = None;
        self.active_job = Some(ActiveJob::new(
            id,
            uri,
            self.view,
            vec![(domain, plan)],
            cancel,
            receiver,
            started_ledger,
            task,
        ));
        true
    }

    pub(super) fn submit_hardware(&mut self) -> bool {
        if self.refuse_if_read_only("hardware changes (:w)") {
            return false;
        }
        if self.refuse_if_busy() {
            return false;
        }
        let Some(detail) = self.detail.as_ref() else {
            return false;
        };
        if self.hardware_reload_required.as_deref() == Some(detail.domain.as_str()) {
            self.show_message("⚠ Hardware refresh pending or failed; reopen details if needed".into());
            return true;
        }
        let unsupported = detail.hw.unsupported_changes();
        if !unsupported.is_empty() {
            let message = format!(
                "✗ cannot apply {} yet (read-only): undo with u or :q!",
                unsupported.join(", ")
            );
            self.messages.push(message.clone());
            self.show_message(message);
            return true;
        }
        let problems = detail.hw.problems();
        if !problems.is_empty() {
            let message = format!("✗ {}", problems.join("; "));
            self.messages.push(message.clone());
            self.show_message(message);
            return true;
        }
        let domain = detail.domain.clone();
        let hardware = detail.hw.clone();
        let plans = hardware.will_run(&domain);
        if plans.is_empty() {
            self.show_message("nothing to apply".into());
            return true;
        }
        let notes = hardware.restart_notes();
        if !notes.is_empty() {
            self.messages.push(format!(
                "⚠ {domain}: {} — saved to the config, applies after a restart",
                notes.join(", ")
            ));
        }
        let fingerprint = hardware.draft_fingerprint();
        let mut steps: Vec<_> = plans.iter().flat_map(|plan| plan.steps.iter().cloned()).collect();
        if steps
            .iter()
            .any(|step| step.program == "virsh" && step.argv.first().is_some_and(|arg| arg == "define"))
        {
            steps.insert(
                0,
                crate::command::plan::CommandStep::new(
                    "virsh",
                    vec!["dumpxml", &domain, "--inactive", "--security-info"],
                ),
            );
        }
        let plan = CommandPlan {
            summary: format!("Applied hardware changes on {domain}"),
            steps,
        };
        // The hardware form is about to change on the host: drop reads that
        // would refill it with the old state.
        self.cancel_read(ReadKind::Hardware);
        self.cancel_read(ReadKind::Detail);
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, receiver) = tokio::sync::mpsc::channel(64);
        let uri = self.uri.clone();
        let (task, started_ledger) =
            crate::command::job::spawn_tracked(crate::command::job::run_hardware_apply(
                uri.clone(),
                domain.clone(),
                hardware,
                plans,
                "virsh".into(),
                control,
                events,
            ));
        self.busy_feedback = None;
        self.active_job = Some(ActiveJob::new(
            id,
            uri,
            self.view,
            vec![(domain.clone(), plan)],
            cancel,
            receiver,
            started_ledger,
            task,
        ));
        if let Some(job) = self.active_job.as_mut() {
            job.hardware_snapshot = Some((domain, fingerprint));
        }
        true
    }

    pub(super) fn submit_xml_export(&mut self) -> bool {
        if self.refuse_if_busy() {
            return false;
        }
        let domain = self.dash.selected().name.clone();
        let mut components = std::path::Path::new(&domain).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_))) || components.next().is_some()
        {
            self.show_message("✗ Cannot use the domain name as an export filename".into());
            return false;
        }
        let path = match std::env::current_dir() {
            Ok(dir) => dir.join(format!("{domain}.xml")),
            Err(error) => {
                self.show_message(format!("✗ Cannot resolve export directory: {error}"));
                return false;
            }
        };
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, receiver) = tokio::sync::mpsc::channel(64);
        let uri = self.uri.clone();
        let (task, started_ledger) = crate::command::job::spawn_tracked(crate::command::job::run_xml_export(
            uri.clone(),
            domain.clone(),
            path.clone(),
            "virsh".into(),
            control,
            events,
        ));
        let plan = CommandPlan {
            summary: format!("Exported {domain} to {}", path.display()),
            steps: vec![crate::command::plan::CommandStep::new(
                "virsh",
                vec!["dumpxml", &domain],
            )],
        };
        self.busy_feedback = None;
        self.active_job = Some(ActiveJob::new(
            id,
            uri,
            self.view,
            vec![(domain, plan)],
            cancel,
            receiver,
            started_ledger,
            task,
        ));
        true
    }

    pub(super) fn submit_xml_editor(&mut self, draft: EditorDraft) -> bool {
        if self.refuse_if_read_only("define") {
            return false;
        }
        if self.refuse_if_busy() {
            return false;
        }
        if draft.uri != self.uri || draft.epoch != self.connection_epoch {
            self.show_message("⚠ Connection changed; the XML draft was not submitted".into());
            return false;
        }
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, receiver) = tokio::sync::mpsc::channel(64);
        let uri = draft.uri.clone();
        let target = draft.target.clone();
        let network = draft.network;
        let (task, started_ledger) = crate::command::job::spawn_tracked(crate::command::job::run_xml_define(
            uri.clone(),
            target.clone(),
            draft.xml.clone(),
            network,
            "virsh".into(),
            control,
            events,
        ));
        let plan = CommandPlan {
            summary: format!("Defined {target}"),
            steps: vec![crate::command::plan::CommandStep::new(
                "virsh",
                vec![
                    if network { "net-define" } else { "define" },
                    "<private-xml>",
                    "--validate",
                ],
            )],
        };
        self.busy_feedback = None;
        self.active_job = Some(ActiveJob::new(
            id,
            uri,
            self.view,
            vec![(target, plan)],
            cancel,
            receiver,
            started_ledger,
            task,
        ));
        if let Some(job) = self.active_job.as_mut() {
            job.editor_draft = Some(draft);
        }
        true
    }

    pub(super) fn submit_media_change(&mut self, domain: String, iso: Option<String>, running: bool) -> bool {
        if self.refuse_if_read_only("change-media") {
            return false;
        }
        if self.refuse_if_busy() {
            return false;
        }
        if domain.is_empty() {
            return false;
        }
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (cancel, control) = tokio::sync::watch::channel(false);
        let (events, receiver) = tokio::sync::mpsc::channel(64);
        let uri = self.uri.clone();
        let (task, started_ledger) =
            crate::command::job::spawn_tracked(crate::command::job::run_media_change(
                uri.clone(),
                domain.clone(),
                iso.clone(),
                running,
                "virsh".into(),
                control,
                events,
            ));
        let change =
            crate::command::builders::hardware::change_media(&domain, "<cdrom>", iso.as_deref(), running);
        let mut steps = vec![crate::command::plan::CommandStep::new(
            "virsh",
            vec!["dumpxml", &domain],
        )];
        steps.extend(change.steps);
        let plan = CommandPlan {
            summary: change.summary,
            steps,
        };
        self.busy_feedback = None;
        self.active_job = Some(ActiveJob::new(
            id,
            uri,
            self.view,
            vec![(domain, plan)],
            cancel,
            receiver,
            started_ledger,
            task,
        ));
        if let Some(job) = self.active_job.as_mut() {
            job.refresh_storage = true;
        }
        true
    }

    pub(super) async fn shutdown_active_job(&mut self) {
        self.cancel_job();
        if let Some(job) = self.active_job.as_mut() {
            let _ = tokio::time::timeout(Duration::from_secs(4), &mut job.task).await;
        }
        // Dropping the job aborts a worker that did not finish within the deadline.
        self.active_job.take();
    }

    pub(super) fn finish_job(&mut self, outcome: Result<BatchOutcome, tokio::task::JoinError>) {
        let Some(job) = self.active_job.take() else {
            return;
        };
        self.job_finished_at = Some(std::time::Instant::now());
        self.rerun_doctor_if_shown();
        let id = job.id;
        // A finished bulk job consumes its marks (they would otherwise stay
        // armed for the next, unrelated action).
        if job.plans.len() > 1 && job.uri == self.uri {
            let done: std::collections::HashSet<&str> = job.plans.iter().map(|(t, _)| t.as_str()).collect();
            self.dash.marks.retain(|m| !done.contains(m.as_str()));
            self.dash.visual_anchor = None;
        }
        let mut last_message = None;
        let wizard_matches = job.submitted_wizard.as_ref().is_some_and(|(revision, snapshot)| {
            *revision == self.wizard_revision
                && self
                    .wizard
                    .as_ref()
                    .map(|wizard| wizard.content_snapshot())
                    .as_ref()
                    == Some(snapshot)
        });
        let job_success = matches!(&outcome, Ok(result) if matches!(result.result, BatchResult::Success));
        let editor_validation_failed = matches!(
            &outcome,
            Ok(result) if matches!(result.result, BatchResult::Failed { .. })
        );
        let editor_error = match &outcome {
            Ok(result) => match &result.result {
                BatchResult::Failed { message, .. } => Some(message.clone()),
                _ => None,
            },
            Err(error) => Some(error.to_string()),
        };
        let hardware_uncertain = match &outcome {
            Ok(result) => !job_success && !result.started_steps.is_empty(),
            Err(_) => true,
        };
        if (job_success || hardware_uncertain)
            && job.uri == self.uri
            && self.view == View::Detail
            && let Some((domain, _)) = job.hardware_snapshot.as_ref()
            && self
                .detail
                .as_ref()
                .is_some_and(|detail| &detail.domain == domain)
        {
            self.hardware_reload_required = Some(domain.clone());
        }
        if (job_success || hardware_uncertain)
            && job.uri == self.uri
            && self.view == View::Detail
            && let Some(draft) = job.editor_draft.as_ref()
            && !draft.network
            && self
                .detail
                .as_ref()
                .is_some_and(|detail| detail.domain == draft.target)
        {
            self.hardware_reload_required = Some(draft.target.clone());
        }
        if job_success && let Some(domain) = job.viewer_after.clone() {
            self.request_viewer(job.uri.clone(), domain);
        }
        let restore_network = match &outcome {
            Ok(result) => !matches!(result.result, BatchResult::Success),
            Err(_) => true,
        };
        if restore_network && let Some(draft) = job.submitted_network.clone() {
            if self.uri == job.uri && self.nets.new_form.is_none() {
                self.nets.new_form = Some(draft);
            } else {
                self.network_retries.push((job.uri.clone(), draft));
                self.messages
                    .push("⚠ A network draft is retained; press n after closing the current form".into());
            }
        }
        match outcome {
            Ok(outcome) => {
                let active_target = outcome
                    .started_steps
                    .last()
                    .filter(|step| !outcome.completed_targets.contains(&step.target))
                    .map(|step| step.target.clone())
                    .unwrap_or_default();
                for step in &outcome.started_steps {
                    self.history.push(&step.command);
                }
                if (job.submitted_network.is_some()
                    || job.snapshot_domain.is_some()
                    || job.hardware_snapshot.is_some())
                    && !matches!(outcome.result, BatchResult::Success)
                {
                    for step in &outcome.completed_steps {
                        let message = format!("✓ completed  ── {}", step.command);
                        self.messages.push(message.clone());
                        self.record_action_event(&message, &step.target, true);
                    }
                }
                if job.raw
                    && !outcome.completed_targets.is_empty()
                    && let Some(output) = outcome.outputs.first()
                {
                    let text = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<_> = text
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .map(crate::model::sanitize)
                        .collect();
                    for line in &lines {
                        self.messages.push(line.clone());
                    }
                    if output.stdout_truncated {
                        self.messages.push("[output truncated]".into());
                    }
                    let shown = outcome
                        .started_steps
                        .first()
                        .map(|step| step.command.as_str())
                        .unwrap_or("");
                    let extra = lines.len().saturating_sub(1) + usize::from(output.stdout_truncated);
                    let first = lines.first().map_or("done", |line| line.trim());
                    let message = if extra > 0 {
                        format!("✓ {first}  ── {shown} ({extra} more lines in :messages)")
                    } else {
                        format!("✓ {first}  ── {shown}")
                    };
                    self.record_action_event(
                        &message,
                        job.plans.first().map(|(target, _)| target.as_str()).unwrap_or(""),
                        true,
                    );
                    last_message = Some(message);
                }
                for target in &outcome.completed_targets {
                    if job.raw {
                        continue;
                    }
                    if let Some((_, plan)) = job.plans.iter().find(|(name, _)| name == target) {
                        let first = if job.submitted_network.is_some() {
                            outcome
                                .started_steps
                                .first()
                                .map(|step| step.command.clone())
                                .unwrap_or_default()
                        } else {
                            plan.steps
                                .first()
                                .map(crate::command::display::display)
                                .unwrap_or_default()
                        };
                        let message = if plan.steps.first().is_some_and(|step| {
                            step.program == "virsh" && step.argv.first().is_some_and(|arg| arg == "shutdown")
                        }) {
                            self.dash.track_shutdown(target);
                            format!(
                                "⏻ Shutdown requested for {target} (ACPI) — waiting for the guest…  ── {first}"
                            )
                        } else {
                            format!("✓ {}  ── {first}", plan.summary)
                        };
                        self.messages.push(message.clone());
                        self.record_action_event(&message, target, true);
                        last_message = Some(message);
                    }
                }
                let bulk = job.plans.len() > 1;
                let steps_of = |target: &str| {
                    job.plans
                        .iter()
                        .find(|(name, _)| name == target)
                        .map_or(0, |(_, plan)| plan.steps.len())
                };
                let failures: Vec<(String, usize, String)> = if outcome.failures.is_empty() {
                    match &outcome.result {
                        BatchResult::Failed {
                            target,
                            step,
                            message,
                        } => {
                            vec![(target.clone(), *step, message.clone())]
                        }
                        _ => vec![],
                    }
                } else {
                    outcome.failures.clone()
                };
                if bulk {
                    for (target, step, message) in &failures {
                        let line = format!("✗ {target}: step {step}/{} failed: {message}", steps_of(target));
                        self.messages.push(line.clone());
                        self.record_action_event(&line, target, false);
                    }
                }
                let error = match outcome.result {
                    BatchResult::Success => None,
                    BatchResult::Failed { target, .. } if bulk => {
                        let failed: Vec<&str> = failures.iter().map(|(t, _, _)| t.as_str()).collect();
                        let ok = outcome.completed_targets.len();
                        let summary = format!(
                            "✗ {ok} ok / {} failed: {} (details in :messages)",
                            failed.len(),
                            failed.join(", ")
                        );
                        last_message = Some(summary.clone());
                        self.messages.push(summary);
                        let _ = target;
                        None
                    }
                    BatchResult::Failed {
                        target,
                        step,
                        message,
                    } => {
                        let total = steps_of(&target);
                        Some((target, format!("✗ step {step}/{total} failed: {message}")))
                    }
                    BatchResult::Timeout => Some((
                        active_target,
                        format!("✗ Job #{id} timed out; verify the object state"),
                    )),
                    BatchResult::Cancelled => Some((
                        active_target,
                        format!("⚠ Job #{id} stopped locally; verify the object state"),
                    )),
                };
                if let Some((target, message)) = error {
                    self.messages.push(message.clone());
                    self.record_action_event(&message, &target, false);
                    last_message = Some(message);
                }
            }
            Err(error) => {
                for step in job
                    .started_ledger
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .iter()
                {
                    self.history.push(&step.command);
                }
                let message = format!("✗ Job #{id} failed: {error}");
                self.messages.push(message.clone());
                self.record_action_event(&message, "", false);
                last_message = Some(message);
            }
        }
        if wizard_matches {
            if job_success {
                self.wizard = None;
                ui::overlays::wizard::WizardState::discard_draft();
            } else if let Some(wizard) = self.wizard.as_mut() {
                wizard.error = last_message.clone().unwrap_or_default();
            }
        }
        if let Some(draft) = job.editor_draft.as_ref()
            && let Some((revision, snapshot)) = &draft.wizard
            && *revision == self.wizard_revision
            && let Some(wizard) = self.wizard.as_mut()
            && &wizard.content_snapshot() == snapshot
        {
            wizard.error = last_message.clone().unwrap_or_default();
        }
        if !job_success && let Some(mut draft) = job.editor_draft.clone() {
            if let Some(error) = &editor_error {
                draft.xml = ui::views::detail::edit_error_comment(&draft.xml, error);
            }
            self.store_editor_draft(draft);
        }
        self.show_message(last_message.unwrap_or_else(|| "✓ done".into()));
        // Reads that started before the job finished may hold stale state:
        // drop them and restart the ones the user is waiting for.
        let reading_networks = self.selection_task.contains_key(&ReadKind::Networks);
        self.cancel_read(ReadKind::Detail);
        self.cancel_read(ReadKind::Picker);
        self.cancel_read(ReadKind::Snapshots);
        self.cancel_read(ReadKind::Networks);
        if reading_networks && self.view == View::Networks {
            self.request_selection_after_load();
        }
        if self.detail_loading && self.view == View::Detail {
            let tab = self.detail.as_ref().map(|detail| detail.tab);
            let pending = self
                .pending_snapshot_modal
                .map(|(_, modal, require_rows)| (modal, require_rows));
            self.request_detail();
            self.pending_snapshot_modal =
                pending.map(|(modal, require_rows)| (self.read_serial, modal, require_rows));
            if let (Some(detail), Some(tab)) = (self.detail.as_mut(), tab) {
                detail.tab = tab;
            }
        } else if self.picker_loading {
            self.restart_picker_read();
        } else if job.uri == self.uri
            && self.view == View::Detail
            && let Some(domain) = job.snapshot_domain.as_deref()
        {
            self.request_snapshot_refresh(domain);
        } else if job.uri == self.uri
            && self.view == View::Detail
            && let Some((domain, fingerprint)) = job.hardware_snapshot.clone()
        {
            self.request_hardware_refresh(domain, fingerprint, job_success, true);
        } else if (job_success || hardware_uncertain)
            && job.uri == self.uri
            && self.view == View::Detail
            && let Some(draft) = job.editor_draft.as_ref()
            && !draft.network
            && let Some(detail) = self
                .detail
                .as_ref()
                .filter(|detail| detail.domain == draft.target)
        {
            self.request_hardware_refresh(
                draft.target.clone(),
                detail.hw.draft_fingerprint(),
                job_success,
                false,
            );
        }
        // Loads that started before the job may return pre-job data: drop
        // them all, and restart every one that was in flight or queued (not
        // only Storage/Networks), so no view is left on "loading…".
        let interrupted: Vec<View> = self
            .view_load_tasks
            .iter()
            .filter(|(_, task)| !task.0.is_finished())
            .map(|(view, _)| *view)
            .chain(self.view_load_pending.iter().copied())
            .collect();
        for serial in self.view_load_serial.values_mut() {
            *serial = serial.wrapping_add(1);
        }
        self.view_load_tasks.clear();
        self.view_load_pending.clear();
        self.view_load_refreshing.clear();
        for view in interrupted {
            self.request_view_load(view);
        }
        if job.uri == self.uri {
            if job.refresh_storage && job.origin != View::Storage {
                self.request_view_load(View::Storage);
            }
            if matches!(job.origin, View::Networks | View::Storage) {
                self.request_view_load(job.origin);
            }
            if self.view != job.origin && matches!(self.view, View::Networks | View::Storage) {
                self.request_view_load(self.view);
            }
        }
        if job.uri == self.uri
            && let Some(poller) = &self.poller
        {
            let _ = poller.tx.send(poller::Request::Now);
            for (target, _) in &job.plans {
                let _ = poller.tx.send(poller::Request::RefetchConfig(target.clone()));
            }
        }
        if editor_validation_failed
            && let Some(submitted) = job.editor_draft.as_ref()
            && let Some(index) = self.editor_retries.iter().position(|draft| {
                draft.generation == submitted.generation
                    && draft.uri == submitted.uri
                    && draft.epoch == submitted.epoch
                    && draft.target == submitted.target
                    && draft.network == submitted.network
                    && draft.wizard == submitted.wizard
                    && self.editor_context_is_current(draft)
            })
        {
            let draft = self.editor_retries.remove(index);
            self.open_editor_draft(draft);
        }
    }
}

/// Route non-destructive raw virsh commands through the job slot.
pub(super) fn raw_job_step(line: &str) -> Option<Result<crate::command::plan::CommandStep, String>> {
    use crate::input::ex::ExCommand;
    let raw = match crate::input::ex::parse(line).ok()? {
        ExCommand::RawVirsh(raw) | ExCommand::Shell(raw) => raw,
        _ => return None,
    };
    let raw = raw.trim();
    let raw = raw.strip_prefix("virsh ").unwrap_or(raw);
    let Some(argv) = shlex::split(raw).filter(|args| !args.is_empty()) else {
        return Some(Err("✗ cannot parse the command (unbalanced quotes?)".into()));
    };
    if argv[0].starts_with('-') {
        return Some(Err("✗ the connection is set with :connect, not -c".into()));
    }
    if crate::ui::overlays::confirm::Confirm::destructive_argv(&argv) {
        return None;
    }
    Some(Ok(crate::command::plan::CommandStep {
        program: "virsh".into(),
        argv,
        stdin: None,
    }))
}

/// Capture command-line plans that need no asynchronous preparation.
/// Short ex forms as job plans. `active` tells whether a domain is running or
/// paused: `--live` is only valid then (virsh rejects it for a shut-off domain).
pub(super) fn ex_job_plans(
    line: &str,
    active: impl Fn(&str) -> bool,
) -> Option<Result<Vec<(String, CommandPlan)>, String>> {
    use crate::command::builders::{hardware, lifecycle};
    use crate::input::ex::ExCommand;
    let (target, plan) = match crate::input::ex::parse(line).ok()? {
        ExCommand::Start(target) => {
            let plan = lifecycle::start(&target);
            (target, plan)
        }
        ExCommand::Rename(target, name) => {
            let plan = lifecycle::rename(&target, &name);
            (target, plan)
        }
        ExCommand::AttachDisk(target, src, dev) => {
            let Some(format) = hardware::image_format(&src) else {
                return Some(Err(format!(
                    "✗ cannot tell the image format of {src}: run :attach-disk {target} {src} {dev} --driver qemu --subdriver <qcow2|raw> --config"
                )));
            };
            let live = active(&target);
            let plan = hardware::attach_disk_formatted(&target, &src, &dev, &format, live, true);
            (target, plan)
        }
        ExCommand::DetachDisk(target, dev) => {
            let live = active(&target);
            let plan = hardware::detach_disk(&target, &dev, live);
            (target, plan)
        }
        ExCommand::AttachNic(target, src, model) => {
            let live = active(&target);
            let plan = hardware::attach_nic(&target, &src, &model, None, live);
            (target, plan)
        }
        ExCommand::Resize(target, dev, size) => {
            if !active(&target) {
                return Some(Err(format!(
                    "✗ block-resize needs a running domain; {target} is not running (use vol-resize)"
                )));
            }
            let Some(kib) = crate::ui::views::detail::hardware::size_to_kib(&size) else {
                return Some(Err(format!(
                    "✗ invalid size `{size}`: use a unit (512M, 8G, 1.5T)"
                )));
            };
            let size = crate::ui::views::detail::hardware::kib_arg(kib);
            let plan = hardware::block_resize(&target, &dev, &size);
            (target, plan)
        }
        ExCommand::Media(target, dev, iso) if !dev.is_empty() => {
            let live = active(&target);
            let plan = hardware::change_media(&target, &dev, iso.as_deref(), live);
            (target, plan)
        }
        _ => return None,
    };
    Some(Ok(vec![(target, plan)]))
}
