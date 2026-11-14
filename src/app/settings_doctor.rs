//! Settings › Doctor keys, background runs and fixes.

use super::*;
use crate::doctor::{Fix, Report};
use ui::views::settings::{Category, SettingsPane};
use ui::views::settings_doctor::PendingFix;

impl App {
    /// Settings is open on the Doctor category.
    fn doctor_shown(&self) -> bool {
        self.settings
            .as_ref()
            .is_some_and(|st| st.category() == Category::Doctor)
    }

    /// Doctor pane keys. True when the key was used.
    pub(super) fn doctor_key(&mut self, key: KeyEvent) -> bool {
        let Some(st) = self.settings.as_mut() else {
            return false;
        };
        if st.category() != Category::Doctor || st.pane != SettingsPane::Options {
            return false;
        }
        let doc = &mut st.doctor;
        if let Some(pending) = doc.pending.take() {
            match key.code {
                KeyCode::Char('y') if key.modifiers.is_empty() => self.apply_doctor_fix(pending.fix),
                KeyCode::Char('n') | KeyCode::Esc => {}
                _ => doc.pending = Some(pending),
            }
            return true;
        }
        let plain = key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT;
        match key.code {
            KeyCode::Char('j') | KeyCode::Down if plain => doc.move_by(1),
            KeyCode::Char('k') | KeyCode::Up if plain => doc.move_by(-1),
            KeyCode::Char('r') if plain => self.start_doctor(),
            KeyCode::Char('y') if plain => {
                if let Some(command) = doc.current().and_then(|c| c.command.clone()) {
                    copy(&command);
                    doc.message = Some(String::from("✓ command copied"));
                }
            }
            KeyCode::Char('Y') if plain => {
                if let Some(report) = &doc.report {
                    copy(&report.to_text(false));
                    doc.message = Some(String::from("✓ report copied"));
                }
            }
            KeyCode::Char('f') if plain => {
                let Some(fix) = doc.current().and_then(|c| c.fix.clone()) else {
                    return true;
                };
                if let Some(job) = &self.active_job {
                    doc.message = Some(format!(
                        "⚠ Job #{} is running; wait or press C-c to cancel",
                        job.id
                    ));
                    return true;
                }
                let question = match &fix {
                    Fix::Set { key, value } => format!("Set {key} to {value}?"),
                    Fix::Connect(uri) => format!("Connect to {uri}?"),
                    // The confirm modal and the editor ask by themselves.
                    Fix::Virsh(_) | Fix::Edit(_) => {
                        self.apply_doctor_fix(fix);
                        return true;
                    }
                };
                doc.message = None;
                doc.pending = Some(PendingFix { fix, question });
            }
            _ => return false,
        }
        true
    }

    fn doctor_message(&mut self, message: String) {
        if let Some(st) = self.settings.as_mut() {
            st.doctor.message = Some(message);
        }
    }

    /// Apply `fix` (already confirmed for Set/Connect).
    fn apply_doctor_fix(&mut self, fix: Fix) {
        match fix {
            Fix::Set { key, value } => {
                let message = if self.demo {
                    format!("set {key}={value} · demo: not saved")
                } else if let Some(error) = &self.config.load_error {
                    // Refuse before changing anything (`:set` would apply it in
                    // memory and then fail to save).
                    format!("✗ not saved: fix the config file first ({error})")
                } else {
                    let uri = self.uri.clone();
                    apply_setting(&uri, &mut self.config, &mut self.theme, key, &value)
                };
                self.doctor_message(message);
                self.start_doctor();
            }
            Fix::Connect(uri) => {
                let message = match self.connect_to(uri.clone()) {
                    Ok(()) => format!("connecting to {uri}"),
                    Err(refused) => refused,
                };
                self.doctor_message(message);
                self.start_doctor();
            }
            Fix::Virsh(plan) => {
                let target = plan
                    .steps
                    .first()
                    .and_then(|s| s.argv.last().cloned())
                    .unwrap_or_default();
                self.confirm_plans("Doctor", "fix", vec![(target, plan)]);
            }
            Fix::Edit(path) => {
                let message = match external::edit_file_status(&path) {
                    external::Foreground::Success => {
                        if crate::input::keymap::path().as_deref() == Some(path.as_path()) {
                            self.keymap_file.refresh();
                            self.engine.set_map(self.keymap_file.map());
                            String::from("✓ keymap.toml reloaded")
                        } else {
                            String::from("restart virsh-tui to load config.toml")
                        }
                    }
                    external::Foreground::SpawnFailed(e) => format!("✗ cannot start {e}"),
                    external::Foreground::Failed(_) => String::from("✗ the editor exited with an error"),
                };
                self.doctor_message(message);
                self.start_doctor();
            }
        }
    }

    /// `:doctor`: Settings on Doctor, report focused.
    pub(super) fn open_doctor(&mut self) {
        let st = match self.settings.take() {
            Some(mut st) => {
                st.cat = Category::ALL.len() - 1;
                st.pane = SettingsPane::Options;
                st
            }
            None => ui::views::settings::SettingsState::open_doctor(&self.config),
        };
        self.settings = Some(st);
        self.maybe_start_doctor();
    }

    /// Start a run when the Doctor category is shown and has no report yet.
    pub(super) fn maybe_start_doctor(&mut self) {
        if self.doctor_shown()
            && let Some(st) = &self.settings
            && st.doctor.report.is_none()
            && !st.doctor.running
        {
            self.start_doctor();
        }
    }

    /// Re-run after a job when the report is on screen (a fix may have run).
    pub(super) fn rerun_doctor_if_shown(&mut self) {
        if self.doctor_shown() {
            self.start_doctor();
        }
    }

    /// Run every check in the background (the demo uses the fixed report).
    pub(super) fn start_doctor(&mut self) {
        let Some(st) = self.settings.as_mut() else {
            return;
        };
        st.doctor.serial += 1;
        st.doctor.running = true;
        let serial = st.doctor.serial;
        if let Some((_, old)) = self.doctor_task.take() {
            old.abort();
        }
        // Tests never run the real checks (processes, clipboard).
        let task = if self.demo || cfg!(test) {
            tokio::spawn(async { crate::doctor::demo_report() })
        } else {
            let source = self.doctor_uri_source();
            let size = self
                .viewport
                .get()
                .map(|r| (r.width, r.height))
                .or_else(|| crossterm::terminal::size().ok());
            let ctx = crate::doctor::Ctx::new(&self.uri, source, &self.config, size);
            tokio::spawn(async move { crate::doctor::run(&ctx, &crate::doctor::runner::SystemRunner).await })
        };
        self.doctor_task = Some((serial, task));
    }

    /// The background task ended (a panic is reported, not swallowed).
    pub(super) fn on_doctor_done(&mut self, serial: u64, result: Result<Report, tokio::task::JoinError>) {
        match result {
            Ok(report) => self.on_doctor(serial, report),
            Err(e) if !e.is_cancelled() => {
                if let Some(st) = self.settings.as_mut()
                    && st.doctor.serial == serial
                {
                    st.doctor.running = false;
                    st.doctor.message = Some(format!("✗ checks failed: {e}; r to retry"));
                }
            }
            Err(_) => {}
        }
    }

    /// Where the current URI came from, for the report header.
    pub(super) fn doctor_uri_source(&self) -> crate::doctor::UriSource {
        use crate::doctor::UriSource;
        if self.flag_uri.as_deref() == Some(self.uri.as_str()) {
            UriSource::Flag
        } else if self.uri == self.config.general.default_uri {
            UriSource::Config
        } else {
            UriSource::Session
        }
    }

    /// A finished run: kept only if Settings still waits for this serial.
    pub(super) fn on_doctor(&mut self, serial: u64, report: Report) {
        let Some(st) = self.settings.as_mut() else {
            return;
        };
        let doc = &mut st.doctor;
        if doc.serial != serial {
            return;
        }
        doc.running = false;
        doc.selected = doc.selected.min(report.checks.len().saturating_sub(1));
        doc.report = Some(report);
    }
}

/// Copy to the clipboard (not from tests: they must not touch the desktop).
fn copy(text: &str) {
    if !cfg!(test) {
        domains::copy_text(text);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::fixture_app;
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn key(c: char) -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    fn code(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    async fn finish(app: &mut App) {
        let (serial, task) = app.doctor_task.take().expect("doctor running");
        let report = task.await.unwrap();
        app.on_doctor(serial, report);
    }

    fn doctor(app: &App) -> &ui::views::settings_doctor::DoctorState {
        &app.settings.as_ref().unwrap().doctor
    }

    /// Demo app with Settings open on Doctor, report loaded, pane focused.
    async fn on_doctor_pane() -> App {
        let mut app = fixture_app();
        app.demo = true;
        app.on_key(key(',')).await;
        app.on_key(key('k')).await;
        finish(&mut app).await;
        app.on_key(key('l')).await;
        app
    }

    fn select(app: &mut App, label: &str) {
        let st = &mut app.settings.as_mut().unwrap().doctor;
        st.selected = st
            .report
            .as_ref()
            .unwrap()
            .checks
            .iter()
            .position(|c| c.label == label)
            .unwrap();
    }

    #[tokio::test]
    async fn settings_has_eight_categories_and_doctor_last() {
        assert_eq!(Category::ALL.len(), 8);
        let mut app = fixture_app();
        app.demo = true;
        app.on_key(key(',')).await;
        app.on_key(key('k')).await;
        assert_eq!(app.settings.as_ref().unwrap().category(), Category::Doctor);
    }

    #[tokio::test]
    async fn doctor_starts_on_first_open_and_reruns() {
        let mut app = fixture_app();
        app.demo = true;
        app.on_key(key(',')).await;
        assert!(app.doctor_task.is_none(), "not started on General");
        app.on_key(key('k')).await;
        assert!(doctor(&app).running);
        finish(&mut app).await;
        assert!(!doctor(&app).running);
        assert_eq!(doctor(&app).report, Some(crate::doctor::demo_report()));
        // Moving away and back keeps the report.
        app.on_key(key('j')).await;
        app.on_key(key('k')).await;
        assert!(app.doctor_task.is_none());
        app.on_key(key('l')).await;
        let serial = doctor(&app).serial;
        app.on_key(key('r')).await;
        assert!(doctor(&app).running);
        assert_eq!(doctor(&app).serial, serial + 1);
    }

    #[tokio::test]
    async fn doctor_result_after_close_is_ignored() {
        let mut app = fixture_app();
        app.demo = true;
        app.on_key(key(',')).await;
        app.on_key(key('k')).await;
        app.on_key(code(KeyCode::Esc)).await;
        assert!(app.settings.is_none());
        finish(&mut app).await;
        assert!(app.settings.is_none());
    }

    #[tokio::test]
    async fn stale_result_is_dropped() {
        let mut app = on_doctor_pane().await;
        let serial = doctor(&app).serial;
        app.on_key(key('r')).await;
        app.on_doctor(
            serial,
            Report {
                checks: Vec::new(),
                ..crate::doctor::demo_report()
            },
        );
        assert!(doctor(&app).running, "old serial ignored");
    }

    #[tokio::test]
    async fn doctor_keys_move_and_copy() {
        let mut app = on_doctor_pane().await;
        assert_eq!(doctor(&app).selected, 0);
        app.on_key(key('j')).await;
        assert_eq!(doctor(&app).selected, 1);
        app.on_key(key('k')).await;
        app.on_key(key('k')).await;
        assert_eq!(doctor(&app).selected, 0, "clamped at the top");
        // No command on `virsh`: y does nothing.
        app.on_key(key('y')).await;
        assert_eq!(doctor(&app).message, None);
        select(&mut app, "permissions");
        app.on_key(key('y')).await;
        assert_eq!(doctor(&app).message.as_deref(), Some("✓ command copied"));
        app.on_key(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT))
            .await;
        assert_eq!(doctor(&app).message.as_deref(), Some("✓ report copied"));
        assert!(app.settings.is_some());
    }

    #[tokio::test]
    async fn doctor_fix_set_asks_then_applies() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = on_doctor_pane().await;
        app.demo = false;
        app.config.source = Some(dir.path().join("config.toml"));
        app.config.appearance.theme = String::from("nope");
        let st = &mut app.settings.as_mut().unwrap().doctor;
        st.report.as_mut().unwrap().checks[0].fix = Some(Fix::Set {
            key: "theme",
            value: String::from("tokyo-night"),
        });
        app.on_key(key('f')).await;
        let pending = doctor(&app).pending.clone().unwrap();
        assert_eq!(pending.question, "Set theme to tokyo-night?");
        app.on_key(key('n')).await;
        assert!(doctor(&app).pending.is_none());
        assert_eq!(app.config.appearance.theme, "nope");
        app.on_key(key('f')).await;
        app.on_key(key('y')).await;
        assert_eq!(app.config.appearance.theme, "tokyo-night");
        assert_eq!(doctor(&app).message.as_deref(), Some("set theme=tokyo-night"));
        assert!(
            std::fs::read_to_string(dir.path().join("config.toml"))
                .unwrap()
                .contains("tokyo-night")
        );
        assert!(app.doctor_task.is_some(), "re-runs after a fix");
    }

    #[tokio::test]
    async fn doctor_fix_set_demo_not_saved() {
        let mut app = on_doctor_pane().await;
        app.config.appearance.theme = String::from("nope");
        let st = &mut app.settings.as_mut().unwrap().doctor;
        st.report.as_mut().unwrap().checks[0].fix = Some(Fix::Set {
            key: "theme",
            value: String::from("tokyo-night"),
        });
        app.on_key(key('f')).await;
        app.on_key(key('y')).await;
        assert_eq!(app.config.appearance.theme, "nope");
        assert_eq!(
            doctor(&app).message.as_deref(),
            Some("set theme=tokyo-night · demo: not saved")
        );
    }

    #[tokio::test]
    async fn doctor_fix_set_refused_when_config_invalid() {
        let mut app = on_doctor_pane().await;
        app.demo = false;
        app.config.load_error = Some(String::from("config.toml: bad"));
        let st = &mut app.settings.as_mut().unwrap().doctor;
        st.report.as_mut().unwrap().checks[0].fix = Some(Fix::Set {
            key: "viewer",
            value: String::from("remote-viewer"),
        });
        app.on_key(key('f')).await;
        app.on_key(key('y')).await;
        assert!(
            doctor(&app)
                .message
                .as_deref()
                .unwrap()
                .contains("fix the config file first"),
            "{:?}",
            doctor(&app).message
        );
        assert_eq!(
            app.config.console.viewer, "virt-viewer",
            "nothing changed in memory"
        );
    }

    #[tokio::test]
    async fn doctor_fix_virsh_opens_confirm() {
        let mut app = on_doctor_pane().await;
        select(&mut app, "networks");
        app.on_key(key('f')).await;
        let (confirm, _, targets) = app.confirm.as_ref().expect("confirm");
        assert_eq!(
            confirm.commands,
            ["virsh net-start default", "virsh net-autostart default"]
        );
        assert_eq!(targets, &["default"]);
        assert!(app.settings.is_some(), "Settings stays open behind the modal");
    }

    #[tokio::test]
    async fn doctor_fix_virsh_blocked_during_job() {
        let mut app = on_doctor_pane().await;
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.demo = false;
        app.submit_plans(vec![("guest".into(), plan)]);
        select(&mut app, "networks");
        app.on_key(key('f')).await;
        assert!(app.confirm.is_none());
        assert!(doctor(&app).message.as_deref().unwrap().contains("is running"));
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn doctor_fix_connect() {
        let mut app = on_doctor_pane().await;
        select(&mut app, "permissions");
        app.on_key(key('f')).await;
        assert_eq!(
            doctor(&app).pending.as_ref().unwrap().question,
            "Connect to qemu:///session?"
        );
        app.on_key(key('y')).await;
        assert_eq!(app.uri, "qemu:///session");
    }

    #[tokio::test]
    async fn ex_doctor_opens_category() {
        let mut app = fixture_app();
        app.demo = true;
        app.on_key(key(':')).await;
        for c in "doctor".chars() {
            app.on_key(key(c)).await;
        }
        app.on_key(code(KeyCode::Enter)).await;
        let st = app.settings.as_ref().expect("settings open");
        assert_eq!(st.category(), Category::Doctor);
        assert_eq!(st.pane, SettingsPane::Options);
        assert!(app.doctor_task.is_some());
    }

    fn render(app: &App, w: u16, h: u16) -> String {
        let backend = ratatui::backend::TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                ui::views::settings::render(
                    f,
                    &app.theme,
                    &app.config,
                    app.settings.as_ref().unwrap(),
                    app.engine.map(),
                    &app.keymap_file,
                )
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .replacen(env!("CARGO_PKG_VERSION"), "[version]", 1)
    }

    #[tokio::test]
    async fn doctor_renders_wide_and_compact() {
        let mut app = on_doctor_pane().await;
        select(&mut app, "permissions");
        insta::assert_snapshot!("doctor_wide", render(&app, 174, 43));
        insta::assert_snapshot!("doctor_compact", render(&app, 120, 40));
    }

    #[tokio::test]
    async fn doctor_fix_confirm_is_drawn_over_settings() {
        let mut app = on_doctor_pane().await;
        select(&mut app, "networks");
        app.on_key(key('f')).await;
        let backend = ratatui::backend::TestBackend::new(174, 43);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = (0..43)
            .map(|y| {
                (0..174)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("virsh net-start default"), "{text}");
        assert!(text.contains("y fix"), "{text}");
    }

    #[tokio::test]
    async fn doctor_hint_visible_on_short_terminals() {
        let mut app = on_doctor_pane().await;
        select(&mut app, "permissions");
        for (w, h) in [(100, 28), (80, 24), (174, 20), (60, 20)] {
            let text = render(&app, w, h);
            assert!(text.contains("❯ ⚠ permissions"), "{w}x{h} row\n{text}");
            assert!(text.contains("run: sudo usermod"), "{w}x{h} command\n{text}");
            assert!(text.contains("fix: connect to"), "{w}x{h} fix\n{text}");
        }
        select(&mut app, "glyphs");
        let text = render(&app, 80, 24);
        assert!(text.contains("❯ · glyphs"), "last row\n{text}");
    }

    #[tokio::test]
    async fn confirmed_fix_survives_a_rerun() {
        // A re-run can land between `f` and `y`: `y` applies what was asked.
        let mut app = on_doctor_pane().await;
        app.config.appearance.theme = String::from("nope");
        let uri = app.uri.clone();
        let st = &mut app.settings.as_mut().unwrap().doctor;
        st.report.as_mut().unwrap().checks[0].fix = Some(Fix::Set {
            key: "theme",
            value: String::from("tokyo-night"),
        });
        app.on_key(key('f')).await;
        let st = &mut app.settings.as_mut().unwrap().doctor;
        st.report.as_mut().unwrap().checks[0].fix = Some(Fix::Connect(String::from("qemu:///session")));
        app.on_key(key('y')).await;
        assert_eq!(app.uri, uri, "the other fix must not run");
        assert_eq!(
            doctor(&app).message.as_deref(),
            Some("set theme=tokyo-night · demo: not saved")
        );
    }

    #[tokio::test]
    async fn compact_status_bar_lists_doctor_keys() {
        let app = on_doctor_pane().await;
        let text = render(&app, 120, 40);
        let bar = text.lines().nth(38).unwrap();
        assert!(bar.contains("r re-run") && bar.contains("Y copy report"), "{bar}");
    }

    #[tokio::test]
    async fn refused_connect_says_why() {
        let mut app = on_doctor_pane().await;
        let mut detail = ui::views::detail::DetailState::demo("vm");
        detail.hw.set_field("memory", "current", "8G");
        app.detail = Some(detail);
        let uri = app.uri.clone();
        select(&mut app, "permissions");
        app.on_key(key('f')).await;
        app.on_key(key('y')).await;
        assert_eq!(app.uri, uri);
        assert!(
            doctor(&app)
                .message
                .as_deref()
                .unwrap()
                .contains("Unsaved hardware changes"),
            "{:?}",
            doctor(&app).message
        );
    }

    #[tokio::test]
    async fn failed_run_stops_checking() {
        let mut app = on_doctor_pane().await;
        app.on_key(key('r')).await;
        let serial = doctor(&app).serial;
        let (_, task) = app.doctor_task.take().unwrap();
        task.abort();
        let _ = task.await;
        let failed: tokio::task::JoinHandle<Report> = tokio::spawn(async { panic!("boom") });
        app.on_doctor_done(serial, failed.await);
        assert!(!doctor(&app).running);
        assert!(
            doctor(&app)
                .message
                .as_deref()
                .unwrap()
                .starts_with("✗ checks failed")
        );
    }

    #[tokio::test]
    async fn tests_never_run_real_checks() {
        let mut app = on_doctor_pane().await;
        app.demo = false;
        app.on_key(key('r')).await;
        finish(&mut app).await;
        assert_eq!(doctor(&app).report, Some(crate::doctor::demo_report()));
    }

    #[test]
    fn uri_source_follows_how_the_uri_was_chosen() {
        use crate::doctor::UriSource;
        let mut app = fixture_app();
        app.config.general.default_uri = String::from("qemu:///system");
        app.uri = String::from("qemu:///system");
        assert_eq!(app.doctor_uri_source(), UriSource::Config);
        app.flag_uri = Some(String::from("qemu:///session"));
        app.uri = String::from("qemu:///session");
        assert_eq!(app.doctor_uri_source(), UriSource::Flag);
        app.uri = String::from("test:///default");
        assert_eq!(app.doctor_uri_source(), UriSource::Session);
    }
}
