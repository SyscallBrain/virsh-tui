//! Owned selection loads; only the latest result may update the view.

use super::*;

pub(super) enum SelectionResult {
    WizardEditorSource {
        uri: String,
        epoch: u64,
        serial: u64,
        target: String,
        generation: u64,
        revision: u64,
        snapshot: String,
        result: Result<String, String>,
    },
    EditorSource {
        uri: String,
        epoch: u64,
        serial: u64,
        target: String,
        generation: u64,
        result: Result<String, String>,
    },
    Hardware {
        uri: String,
        serial: u64,
        epoch: u64,
        domain: String,
        fingerprint: String,
        success: bool,
        clear_submitted: bool,
        state: Option<Box<(crate::model::DomainConfig, String)>>,
    },
    Snapshots {
        uri: String,
        serial: u64,
        epoch: u64,
        domain: String,
        state: Box<ui::views::detail::snapshots::SnapshotState>,
    },
    Picker {
        uri: String,
        serial: u64,
        epoch: u64,
        domain: String,
        cdrom: Option<(String, String, String)>,
    },
    Detail {
        uri: String,
        serial: u64,
        epoch: u64,
        selected: String,
        state: Box<ui::views::detail::DetailState>,
    },
    Networks {
        uri: String,
        serial: u64,
        selected: String,
        state: Box<ui::views::networks::NetworksState>,
    },
    Storage {
        uri: String,
        serial: u64,
        selected: String,
        state: Box<ui::views::storage::StorageState>,
    },
}

pub(super) struct SelectionTask(pub tokio::task::JoinHandle<SelectionResult>);
impl Drop for SelectionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) struct ViewLoadTask(pub tokio::task::JoinHandle<()>);
impl Drop for ViewLoadTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl App {
    pub(super) fn request_media_preview(&mut self, domain: String, iso: Option<String>) {
        let config = self
            .detail
            .as_ref()
            .filter(|detail| detail.domain == domain && detail.uri == self.uri)
            .map(|detail| &detail.config)
            .or_else(|| self.dash.configs.get(&domain));
        let Some(config) = config else {
            self.show_message(format!("✗ CD-ROM details for {domain} are not loaded"));
            return;
        };
        let Some(cdrom) = config.disks.iter().find(|disk| disk.device == "cdrom") else {
            self.show_message(format!("✗ {domain} has no CDROM drive: use ␣mi to add one"));
            return;
        };
        let plan =
            crate::command::builders::hardware::change_media(&domain, &cdrom.target, iso.as_deref(), true);
        let command = plan
            .steps
            .first()
            .map(crate::command::display::display)
            .unwrap_or_default();
        self.history.push(&command);
        let message = format!("[dry-run] {}  ── {command}", plan.summary);
        self.messages.push(message.clone());
        self.show_message(message);
    }

    pub(super) fn request_wizard_editor(&mut self) {
        let Some(wizard) = self.wizard.as_ref() else {
            return;
        };
        let Ok(snapshot) = toml::to_string(wizard) else {
            return;
        };
        let target = wizard.name.clone();
        let revision = self.wizard_revision;
        self.read_serial = self.read_serial.wrapping_add(1);
        self.selection_task.take();
        if let Some(index) = self.editor_retries.iter().position(|draft| {
            draft.uri == self.uri
                && draft.epoch == self.connection_epoch
                && draft.wizard.as_ref() == Some(&(revision, snapshot.clone()))
        }) {
            let draft = self.editor_retries.remove(index);
            self.open_editor_draft(draft);
            return;
        }
        let mut step = crate::command::builders::install::virt_install(wizard)
            .steps
            .remove(0);
        step.argv.insert(0, self.uri.clone());
        step.argv.insert(0, "--connect".into());
        step.argv.push("--print-xml".into());
        self.editor_generation = self.editor_generation.wrapping_add(1);
        let generation = self.editor_generation;
        let uri = self.uri.clone();
        let epoch = self.connection_epoch;
        let serial = self.read_serial;
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.error = "Preparing XML…".into();
        }
        self.selection_task = Some(SelectionTask(tokio::spawn(async move {
            let (_sender, mut control) = tokio::sync::watch::channel(false);
            let result = match crate::command::process::run_step(&uri, &step, &mut control).await {
                Ok(output) if output.status.success() => output
                    .structured_stdout()
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string())),
                Ok(output) => Err(crate::model::sanitize(&String::from_utf8_lossy(&output.stderr))),
                Err(error) => Err(error.to_string()),
            };
            SelectionResult::WizardEditorSource {
                uri,
                epoch,
                serial,
                target,
                generation,
                revision,
                snapshot,
                result,
            }
        })));
    }

    pub(super) fn request_network_editor(&mut self) {
        let target = self.nets.selected_name().to_string();
        if target.is_empty() {
            return;
        }
        self.read_serial = self.read_serial.wrapping_add(1);
        self.selection_task.take();
        if let Some(index) = self.editor_retries.iter().position(|draft| {
            draft.network
                && draft.uri == self.uri
                && draft.epoch == self.connection_epoch
                && draft.target == target
        }) {
            let draft = self.editor_retries.remove(index);
            self.open_editor_draft(draft);
            return;
        }
        self.editor_generation = self.editor_generation.wrapping_add(1);
        let uri = self.uri.clone();
        let epoch = self.connection_epoch;
        let serial = self.read_serial;
        let generation = self.editor_generation;
        self.nets.message = "Loading network XML…".into();
        self.selection_task = Some(SelectionTask(tokio::spawn(async move {
            let result = crate::backend::virsh::exec::run(&uri, &["net-dumpxml", "--inactive", &target])
                .await
                .map_err(|error| error.to_string());
            SelectionResult::EditorSource {
                uri,
                epoch,
                serial,
                target,
                generation,
                result,
            }
        })));
    }

    pub(super) fn request_hardware_refresh(
        &mut self,
        domain: String,
        fingerprint: String,
        success: bool,
        clear_submitted: bool,
    ) {
        if !self.detail.as_ref().is_some_and(|detail| detail.domain == domain) {
            return;
        }
        self.read_serial = self.read_serial.wrapping_add(1);
        self.selection_task.take();
        let uri = self.uri.clone();
        let serial = self.read_serial;
        let epoch = self.connection_epoch;
        self.selection_task = Some(SelectionTask(tokio::spawn(async move {
            let state =
                match crate::backend::virsh::exec::run(&uri, &["dumpxml", &domain, "--inactive"]).await {
                    Ok(xml) => crate::backend::virsh::parse_xml::parse_domain_xml(&xml)
                        .ok()
                        .map(|cfg| Box::new((cfg, xml))),
                    Err(_) => None,
                };
            SelectionResult::Hardware {
                uri,
                serial,
                epoch,
                domain,
                fingerprint,
                success,
                clear_submitted,
                state,
            }
        })));
    }

    pub(super) fn request_snapshot_refresh(&mut self, domain: &str) {
        let Some(mut detail) = self
            .detail
            .as_ref()
            .filter(|detail| detail.domain == domain)
            .cloned()
        else {
            return;
        };
        self.read_serial = self.read_serial.wrapping_add(1);
        self.selection_task.take();
        let uri = self.uri.clone();
        let serial = self.read_serial;
        let epoch = self.connection_epoch;
        let domain = domain.to_string();
        self.selection_task = Some(SelectionTask(tokio::spawn(async move {
            refresh_snapshots(&uri, &mut detail).await;
            SelectionResult::Snapshots {
                uri,
                serial,
                epoch,
                domain,
                state: Box::new(detail.snaps),
            }
        })));
    }

    pub(super) fn request_refresh(&mut self) {
        if self.demo {
            return;
        }
        if let Some(poller) = &self.poller {
            let _ = poller.tx.send(poller::Request::Now);
        }
        if matches!(self.view, View::Host | View::Networks | View::Storage) {
            if !self.detail_loading && !self.picker_loading {
                self.read_serial = self.read_serial.wrapping_add(1);
                self.selection_task.take();
            }
            self.view_load_refreshing.insert(self.view);
            self.request_view_load(self.view);
        }
    }

    pub(super) fn request_viewer(&mut self, uri: String, domain: String) {
        self.viewer_task = Some(ViewLoadTask(tokio::spawn(async move {
            open_viewer(&uri, &domain).await;
        })));
    }

    pub(super) fn request_snapshot_detail(&mut self, modal: ui::views::detail::SnapModal) {
        self.request_detail_tab(ui::views::detail::DetailTab::Snapshots, None);
        if self.detail_loading {
            self.pending_snapshot_modal = Some((self.read_serial, modal, false));
        } else if let Some(detail) = self.detail.as_mut() {
            detail.snap_modal = modal;
        }
    }

    pub(super) fn request_palette_snapshot(&mut self, modal: ui::views::detail::SnapModal) {
        self.request_snapshot_detail(modal);
        if let Some((_, _, require_rows)) = self.pending_snapshot_modal.as_mut() {
            *require_rows = true;
        } else if let Some(detail) = self.detail.as_mut()
            && detail.snaps.rows.is_empty()
            && modal != ui::views::detail::SnapModal::Create
        {
            detail.snap_modal = ui::views::detail::SnapModal::None;
        }
    }

    pub(super) fn request_detail_tab(&mut self, tab: ui::views::detail::DetailTab, device: Option<&str>) {
        if self.view != View::Detail {
            self.request_detail();
        }
        if let Some(detail) = self.detail.as_mut() {
            detail.tab = tab;
            if let Some(index) = device.and_then(|id| detail.hw.devices.iter().position(|item| item.id == id))
            {
                detail.hw.selected = index;
            }
        }
    }

    pub(super) fn request_picker(&mut self) {
        let domain = self
            .detail
            .as_ref()
            .map(|detail| detail.domain.clone())
            .unwrap_or_else(|| self.dash.selected().name.clone());
        self.request_picker_for(domain);
    }

    pub(super) fn request_storage_picker(&mut self) -> bool {
        if !self
            .store
            .selected_vol()
            .is_some_and(|volume| volume.name.ends_with(".iso"))
        {
            return false;
        }
        let domain = self.dash.selected().name.clone();
        self.request_picker_for(domain);
        true
    }

    fn request_picker_for(&mut self, domain: String) {
        if domain.is_empty() {
            return;
        }
        self.read_serial = self.read_serial.wrapping_add(1);
        self.selection_task.take();
        let mut pools: Vec<_> = self.store.pools.iter().map(|pool| pool.name.clone()).collect();
        if pools.is_empty() {
            pools = vec!["isos".into(), "default".into()];
        }
        self.store.picker = Some(ui::views::storage::MediaPicker {
            domain: domain.clone(),
            target: "sda".into(),
            bus: "sata".into(),
            current: if self.demo {
                "archlinux-2026.09.01-x86_64.iso".into()
            } else {
                String::new()
            },
            query: String::new(),
            pool_idx: 0,
            pools,
            selected: 0,
            live: true,
            config: true,
        });
        if self.demo {
            self.picker_loading = false;
            return;
        }
        self.restart_picker_read();
    }

    pub(super) fn restart_picker_read(&mut self) {
        let Some(domain) = self.store.picker.as_ref().map(|picker| picker.domain.clone()) else {
            self.picker_loading = false;
            return;
        };
        self.read_serial = self.read_serial.wrapping_add(1);
        self.selection_task.take();
        self.picker_loading = true;
        let uri = self.uri.clone();
        let serial = self.read_serial;
        let epoch = self.connection_epoch;
        self.selection_task = Some(SelectionTask(tokio::spawn(async move {
            let cdrom = cdrom_of(&uri, &domain).await;
            SelectionResult::Picker {
                uri,
                serial,
                epoch,
                domain,
                cdrom,
            }
        })));
    }

    pub(super) fn request_detail(&mut self) {
        self.pending_snapshot_modal = None;
        self.hardware_reload_required = None;
        let name = if self.view == View::Detail {
            self.detail
                .as_ref()
                .map(|detail| detail.domain.clone())
                .unwrap_or_default()
        } else if self.view == View::Host {
            self.host
                .domains
                .first()
                .map(|domain| domain.name.clone())
                .unwrap_or_default()
        } else {
            self.dash.selected().name.clone()
        };
        if name.is_empty() {
            return;
        }
        self.read_serial = self.read_serial.wrapping_add(1);
        self.selection_task.take();
        let mut initial = detail_initial(&name, &self.dash, &self.uri, self.demo);
        self.view = View::Detail;
        if self.demo {
            self.detail = Some(initial);
            return;
        }
        initial.message = "Loading domain details…".into();
        self.detail = Some(initial.clone());
        self.detail_loading = true;
        let uri = self.uri.clone();
        let serial = self.read_serial;
        let epoch = self.connection_epoch;
        let selected = name;
        self.selection_task = Some(SelectionTask(tokio::spawn(async move {
            let mut state = initial;
            load_detail(&uri, &mut state).await;
            SelectionResult::Detail {
                uri,
                serial,
                epoch,
                selected,
                state: Box::new(state),
            }
        })));
    }

    pub(super) fn request_view_load(&mut self, view: View) {
        if self
            .view_load_tasks
            .get(&view)
            .is_some_and(|task| !task.0.is_finished())
        {
            self.view_load_pending.insert(view);
            return;
        }
        self.view_load_tasks.remove(&view);
        self.view_load_pending.remove(&view);
        let serial = self.view_load_serial.entry(view).or_insert(0);
        *serial = serial.wrapping_add(1);
        if let Some(task) = spawn_load(&self.uri, view, self.connection_epoch, *serial, &self.loads_tx) {
            self.view_load_tasks.insert(view, ViewLoadTask(task));
        }
    }

    pub(super) fn request_selection(&mut self) {
        self.request_selection_inner(true);
    }

    pub(super) fn request_selection_after_load(&mut self) {
        self.request_selection_inner(false);
    }

    fn request_selection_inner(&mut self, preserve_refresh: bool) {
        self.read_serial = self.read_serial.wrapping_add(1);
        self.selection_task.take();
        if preserve_refresh {
            if let Some(serial) = self.view_load_serial.get_mut(&self.view) {
                *serial = serial.wrapping_add(1);
            }
            self.view_load_tasks.remove(&self.view);
            if self.view_load_pending.remove(&self.view) || self.view_load_refreshing.contains(&self.view) {
                self.view_load_refreshing.insert(self.view);
                self.request_view_load(self.view);
            }
        }
        let uri = self.uri.clone();
        let serial = self.read_serial;
        match self.view {
            View::Networks => {
                let selected = self.nets.selected_name().to_string();
                if selected.is_empty() {
                    return;
                }
                self.nets.leases.clear();
                self.nets.attached.clear();
                self.nets.lease_selected = 0;
                let mut state = self.nets.clone();
                self.selection_task = Some(SelectionTask(tokio::spawn(async move {
                    load_net_selection(&uri, &mut state).await;
                    SelectionResult::Networks {
                        uri,
                        serial,
                        selected,
                        state: Box::new(state),
                    }
                })));
            }
            View::Storage => {
                let selected = self.store.selected_pool().to_string();
                if selected.is_empty() {
                    return;
                }
                self.store.volumes.clear();
                self.store.vol_selected = 0;
                let mut state = self.store.clone();
                self.selection_task = Some(SelectionTask(tokio::spawn(async move {
                    load_pool_volumes(&uri, &mut state).await;
                    SelectionResult::Storage {
                        uri,
                        serial,
                        selected,
                        state: Box::new(state),
                    }
                })));
            }
            _ => {}
        }
    }

    pub(super) fn apply_selection_result(&mut self, result: SelectionResult) {
        match result {
            SelectionResult::WizardEditorSource {
                uri,
                epoch,
                serial,
                target,
                generation,
                revision,
                snapshot,
                result,
            } if self.uri == uri
                && self.connection_epoch == epoch
                && self.read_serial == serial
                && self.wizard_revision == revision
                && self
                    .wizard
                    .as_ref()
                    .and_then(|wizard| toml::to_string(wizard).ok())
                    .as_ref()
                    == Some(&snapshot) =>
            {
                match result {
                    Ok(xml) => {
                        let draft = jobs::EditorDraft {
                            uri,
                            epoch,
                            target,
                            xml,
                            network: false,
                            wizard: Some((revision, snapshot)),
                            generation,
                        };
                        if self.editor_context_is_current(&draft) {
                            self.open_editor_draft(draft);
                        } else {
                            self.store_editor_draft(draft);
                            if let Some(wizard) = self.wizard.as_mut() {
                                wizard.error = "XML draft ready; press C-e to edit it".into();
                            }
                        }
                    }
                    Err(error) => {
                        if let Some(wizard) = self.wizard.as_mut() {
                            wizard.error = format!("Cannot prepare XML: {error}");
                        }
                    }
                }
            }
            SelectionResult::EditorSource {
                uri,
                epoch,
                serial,
                target,
                generation,
                result,
            } if self.uri == uri
                && self.connection_epoch == epoch
                && self.read_serial == serial
                && self.view == View::Networks
                && self.nets.selected_name() == target =>
            {
                match result {
                    Ok(xml) => {
                        let draft = jobs::EditorDraft {
                            uri,
                            epoch,
                            target,
                            xml,
                            network: true,
                            wizard: None,
                            generation,
                        };
                        if self.editor_context_is_current(&draft) {
                            self.open_editor_draft(draft);
                        } else {
                            self.store_editor_draft(draft);
                            self.nets.message = "XML draft ready; press e to edit it".into();
                        }
                    }
                    Err(error) => self.nets.message = format!("✗ Cannot load network XML: {error}"),
                }
            }
            SelectionResult::Hardware {
                uri,
                serial,
                epoch,
                domain,
                fingerprint,
                success,
                clear_submitted,
                state,
            } if self.uri == uri
                && self.read_serial == serial
                && self.connection_epoch == epoch
                && self.view == View::Detail
                && self.detail.as_ref().is_some_and(|detail| detail.domain == domain) =>
            {
                if state.is_none() && success {
                    let message =
                        "⚠ Applied hardware changes; could not refresh the configuration".to_string();
                    self.messages.push(message.clone());
                    self.show_message(message);
                    self.hardware_reload_required = Some(domain.clone());
                }
                if let (Some(detail), Some(data)) = (self.detail.as_mut(), state) {
                    let (config, xml) = *data;
                    if success && clear_submitted && detail.hw.draft_fingerprint() == fingerprint {
                        let selected = detail
                            .hw
                            .devices
                            .get(detail.hw.selected)
                            .map(|device| device.id.clone());
                        let mut hardware = ui::views::detail::hardware::HardwareState::from_config(
                            &config,
                            &xml,
                            detail.state == crate::model::DomainState::Running,
                        );
                        if let Some(index) =
                            selected.and_then(|id| hardware.devices.iter().position(|device| device.id == id))
                        {
                            hardware.selected = index;
                        }
                        detail.hw = hardware;
                        detail.hw_insert = false;
                        self.hardware_reload_required = None;
                    } else if success {
                        if let Some(rebased) = detail.hw.rebase(
                            &config,
                            &xml,
                            detail.state == crate::model::DomainState::Running,
                        ) {
                            detail.hw = rebased;
                            self.hardware_reload_required = None;
                        } else {
                            self.hardware_reload_required = Some(domain.clone());
                            detail.message =
                                "⚠ Hardware changed; reopen details before applying again".into();
                        }
                    }
                    detail.config = config;
                    detail.xml = xml;
                    detail.fold = ui::views::detail::XmlFold::new(detail.xml.lines().count());
                }
            }
            SelectionResult::Snapshots {
                uri,
                serial,
                epoch,
                domain,
                state,
            } if self.uri == uri
                && self.read_serial == serial
                && self.connection_epoch == epoch
                && self.view == View::Detail
                && self.detail.as_ref().is_some_and(|detail| detail.domain == domain) =>
            {
                if let Some(detail) = self.detail.as_mut() {
                    let selected = detail.snaps.selected_name().to_string();
                    detail.snaps.rows = state.rows;
                    detail.snaps.current = state.current;
                    detail.snaps.frozen = state.frozen;
                    detail.snaps.selected = detail
                        .snaps
                        .rows
                        .iter()
                        .position(|row| row.name == selected)
                        .unwrap_or(0);
                }
            }
            SelectionResult::Picker {
                uri,
                serial,
                epoch,
                domain,
                cdrom,
            } if self.uri == uri
                && self.read_serial == serial
                && self.connection_epoch == epoch
                && self
                    .store
                    .picker
                    .as_ref()
                    .is_some_and(|picker| picker.domain == domain) =>
            {
                self.picker_loading = false;
                if let (Some(picker), Some((target, bus, current))) = (self.store.picker.as_mut(), cdrom) {
                    picker.target = target;
                    picker.bus = bus;
                    picker.current = current;
                }
            }
            SelectionResult::Detail {
                uri,
                serial,
                epoch,
                selected,
                state,
            } if self.uri == uri
                && self.read_serial == serial
                && self.connection_epoch == epoch
                && self.view == View::Detail
                && self
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.domain == selected) =>
            {
                self.detail_loading = false;
                if let Some(detail) = self.detail.as_mut() {
                    let selected_device = detail
                        .hw
                        .devices
                        .get(detail.hw.selected)
                        .map(|device| device.id.clone());
                    let mut hardware = state.hw;
                    if let Some(index) = selected_device
                        .and_then(|id| hardware.devices.iter().position(|device| device.id == id))
                    {
                        hardware.selected = index;
                    }
                    detail.uptime = state.uptime;
                    detail.config = state.config;
                    detail.xml = state.xml;
                    detail.fold = state.fold;
                    detail.hw = hardware;
                    detail.snaps = state.snaps;
                    if let Some((pending_serial, modal, require_rows)) = self.pending_snapshot_modal.take()
                        && pending_serial == serial
                        && detail.tab == ui::views::detail::DetailTab::Snapshots
                        && detail.snap_modal == ui::views::detail::SnapModal::None
                        && (!require_rows
                            || !detail.snaps.rows.is_empty()
                            || modal == ui::views::detail::SnapModal::Create)
                    {
                        detail.snap_modal = modal;
                    }
                    if detail.message == "Loading domain details…" {
                        detail.message.clear();
                    }
                }
            }
            SelectionResult::Networks {
                uri,
                serial,
                selected,
                state,
            } if self.uri == uri && self.read_serial == serial && self.nets.selected_name() == selected => {
                self.nets.leases = state.leases;
                self.nets.attached = state.attached;
                self.nets.lease_selected = 0;
            }
            SelectionResult::Storage {
                uri,
                serial,
                selected,
                state,
            } if self.uri == uri && self.read_serial == serial && self.store.selected_pool() == selected => {
                self.store.volumes = state.volumes;
                self.store.vol_selected = 0;
            }
            _ => {}
        }
    }
}

pub(super) enum HelpData {
    Index(Vec<crate::backend::virsh::parse_help::HelpEntry>),
    Doc(String, Option<crate::backend::virsh::parse_help::HelpDoc>),
}

pub(super) struct HelpTask {
    pub uri: String,
    pub epoch: u64,
    pub line: String,
    pub cursor: usize,
    pub input_revision: u64,
    pub back: bool,
    pub command: Option<String>,
    pub palette: bool,
    pub task: tokio::task::JoinHandle<HelpData>,
}
impl Drop for HelpTask {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl App {
    pub(super) fn request_help(&mut self, line: String, back: bool, command: Option<String>) {
        if let Some(task) = self.help_task.as_mut()
            && task.uri == self.uri
            && task.epoch == self.connection_epoch
            && task.command == command
            && !task.palette
        {
            task.line = line;
            task.cursor = self.ex_line.as_ref().map_or(0, |input| input.cursor());
            task.input_revision = self.input_revision;
            task.back = back;
            return;
        }
        self.help_task.take();
        let uri = self.uri.clone();
        let epoch = self.connection_epoch;
        let request_uri = uri.clone();
        let requested_command = command.clone();
        let task = tokio::spawn(async move {
            match command {
                Some(command) => {
                    let doc = crate::backend::virsh::parse_help::fetch_doc(&request_uri, &command).await;
                    HelpData::Doc(command, doc)
                }
                None => HelpData::Index(crate::backend::virsh::parse_help::fetch_index(&request_uri).await),
            }
        });
        self.help_task = Some(HelpTask {
            uri,
            epoch,
            line,
            cursor: self.ex_line.as_ref().map_or(0, |input| input.cursor()),
            input_revision: self.input_revision,
            back,
            command: requested_command,
            palette: false,
            task,
        });
        self.show_message("loading help…".into());
    }

    pub(super) fn open_async_palette(&mut self) {
        let context = self
            .detail
            .as_ref()
            .map(|detail| detail.domain.clone())
            .unwrap_or_else(|| self.dash.selected().name.clone());
        let entries: Vec<_> = self
            .virsh_index
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|(name, summary)| crate::backend::virsh::parse_help::HelpEntry {
                name: name.clone(),
                summary: summary.clone(),
            })
            .collect();
        self.palette = Some(ui::overlays::palette::PaletteState::open_live(&context, &entries));
        if self.virsh_index.is_none() {
            self.request_palette_help(None);
        } else {
            self.schedule_palette_doc();
        }
    }

    pub(super) fn schedule_palette_doc(&mut self) {
        if self
            .help_task
            .as_ref()
            .is_some_and(|task| task.palette && task.command.is_none())
        {
            return;
        }
        if let Some(command) = self.palette.as_ref().and_then(|palette| palette.needs_doc()) {
            if let Some(cached) = self.help_docs.get(&command) {
                if let Some(document) = cached
                    && let Some(palette) = self.palette.as_mut()
                {
                    palette.docs.push(document.clone());
                }
                return;
            }
            self.request_palette_help(Some(command));
        }
    }

    fn request_palette_help(&mut self, command: Option<String>) {
        if self.help_task.as_ref().is_some_and(|task| {
            task.palette
                && task.uri == self.uri
                && task.epoch == self.connection_epoch
                && task.command == command
        }) {
            return;
        }
        self.help_task.take();
        let uri = self.uri.clone();
        let request_uri = uri.clone();
        let requested_command = command.clone();
        let task = tokio::spawn(async move {
            match command {
                Some(command) => {
                    let doc = crate::backend::virsh::parse_help::fetch_doc(&request_uri, &command).await;
                    HelpData::Doc(command, doc)
                }
                None => HelpData::Index(crate::backend::virsh::parse_help::fetch_index(&request_uri).await),
            }
        });
        self.help_task = Some(HelpTask {
            uri,
            epoch: self.connection_epoch,
            line: String::new(),
            cursor: 0,
            input_revision: self.input_revision,
            back: false,
            command: requested_command,
            palette: true,
            task,
        });
    }

    pub(super) fn apply_help_result(&mut self, result: Result<HelpData, tokio::task::JoinError>) {
        let Some(task) = self.help_task.take() else {
            return;
        };
        if task.uri != self.uri || task.epoch != self.connection_epoch {
            return;
        }
        match result {
            Ok(HelpData::Index(entries)) => {
                if task.palette
                    && let Some(palette) = self.palette.as_mut()
                {
                    palette.items =
                        ui::overlays::palette::PaletteState::open_live(&palette.context, &entries).items;
                }
                self.virsh_index = Some(
                    entries
                        .into_iter()
                        .map(|entry| (entry.name, entry.summary))
                        .collect(),
                )
            }
            Ok(HelpData::Doc(command, doc)) => {
                self.help_docs.insert(command.clone(), doc.clone());
                if task.palette
                    && let Some(palette) = self.palette.as_mut()
                    && let Some(document) = &doc
                {
                    palette.docs.push(document.clone());
                }
                self.virsh_flags.insert(
                    command,
                    doc.map(|doc| {
                        doc.options
                            .into_iter()
                            .map(|option| (option.flag, option.desc))
                            .collect()
                    })
                    .unwrap_or_default(),
                );
            }
            Err(_) => return,
        }
        if task.palette {
            if self.palette.is_some() {
                self.schedule_palette_doc();
            }
            return;
        }
        if self.input_revision == task.input_revision
            && self
                .ex_line
                .as_ref()
                .is_some_and(|input| input.value() == task.line && input.cursor() == task.cursor)
        {
            self.tab_complete(task.back);
        }
    }
}
