//! App: main loop, tick, and input.

pub mod backoff;
pub mod external;
pub mod mode;
pub mod poller;

mod detail;
mod domains;
mod ex;
mod jobs;
mod live;
mod networks;
mod palette;
mod render;
mod requests;
mod settings;
mod storage;
mod wizard;

#[cfg(test)]
mod review_regressions;

use color_eyre::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::time::Duration;
use tokio::time::interval;

use crate::cli::Cli;
use crate::config::Config;
use crate::theme::Theme;
use crate::ui::{self, ChromeState};
use detail::*;
use domains::*;
use ex::*;
use jobs::*;
use live::*;
use networks::*;
use palette::*;
use ratatui::layout::Rect;
use ratatui::style::Style;
use render::*;
use requests::ReadKind;
use requests::*;
use settings::*;
use storage::*;
use wizard::*;

/// Build the working theme from config (builtin/custom + appearance flags).
fn current_theme(config: &Config) -> Theme {
    let (base, warn) = ui::views::settings::resolve(&config.appearance.theme);
    if let Some(w) = warn {
        tracing::warn!("{w}");
    }
    let mut theme = ui::views::settings::apply_appearance(base, config);
    if !crate::theme::truecolor() {
        tracing::warn!("no truecolor detected: using xterm-256 fallback");
        theme = crate::theme::downgrade(theme);
    }
    theme
}

/// Run the TUI until quit.
pub async fn run(cli: &Cli, config: &Config) -> Result<()> {
    let (_sender, shutdown) = tokio::sync::watch::channel(None);
    run_with_shutdown(cli, config, shutdown).await
}

/// Run the TUI with a channel for coordinated OS shutdown requests.
/// Owns the ratatui terminal. ratatui's `Drop` shows the cursor and, when
/// that fails, `eprintln!`s; after a hangup (`SIGHUP`) stderr is gone too,
/// so that print panics inside the drop and the process aborts. The guard
/// shows the cursor itself and skips ratatui's drop when the tty is dead.
struct TerminalGuard<B: ratatui::backend::Backend>(Option<Terminal<B>>);

impl<B: ratatui::backend::Backend> std::ops::Deref for TerminalGuard<B> {
    type Target = Terminal<B>;
    fn deref(&self) -> &Terminal<B> {
        self.0.as_ref().expect("terminal present until drop")
    }
}

impl<B: ratatui::backend::Backend> std::ops::DerefMut for TerminalGuard<B> {
    fn deref_mut(&mut self) -> &mut Terminal<B> {
        self.0.as_mut().expect("terminal present until drop")
    }
}

impl<B: ratatui::backend::Backend> Drop for TerminalGuard<B> {
    fn drop(&mut self) {
        if let Some(mut terminal) = self.0.take()
            && terminal.show_cursor().is_err()
        {
            std::mem::forget(terminal);
        }
    }
}

pub async fn run_with_shutdown(
    cli: &Cli,
    config: &Config,
    mut shutdown: tokio::sync::watch::Receiver<Option<i32>>,
) -> Result<()> {
    if shutdown.borrow().is_some() {
        return Ok(());
    }
    let mut config = config.clone();
    if let Some(theme) = &cli.theme {
        config.appearance.theme = theme.clone();
    }
    crate::command::exec::set_demo(cli.demo);
    if !cli.demo {
        crate::ui::chrome::set_live(crate::ui::chrome::LiveChrome {
            hostname: connection_hostname(&cli.connect),
            refresh_secs: config.general.refresh_interval_secs.max(1),
        });
    }
    let theme: Theme = current_theme(&config);
    let state = ChromeState {
        uri: cli.connect.clone(),
        ..Default::default()
    };
    let mut dash = if cli.demo {
        ui::views::dashboard::DashboardState::demo()
    } else {
        let mut dashboard = ui::views::dashboard::DashboardState::from_summaries(&[]);
        dashboard.message = String::from("loading…");
        dashboard.hostname = connection_hostname(&cli.connect);
        dashboard
    };

    // Host/Networks/Storage need dozens of virsh calls: they load in the
    // background after the first frame (see `spawn_load`).
    let loading_note = || String::from("loading…");
    let mut host = if cli.demo {
        ui::views::host::HostState::demo()
    } else {
        ui::views::host::HostState {
            message: loading_note(),
            ..Default::default()
        }
    };
    let mut nets = if cli.demo {
        ui::views::networks::NetworksState::demo()
    } else {
        ui::views::networks::NetworksState {
            message: loading_note(),
            ..Default::default()
        }
    };
    let mut store = if cli.demo {
        ui::views::storage::StorageState::demo()
    } else {
        ui::views::storage::StorageState {
            message: loading_note(),
            ..Default::default()
        }
    };
    let mut events_state = ui::views::events::EventsState::demo();
    if !cli.demo {
        // Live mode starts with an empty log; the stream fills it.
        events_state.events.clear();
        for (u, c) in [
            (&mut dash.uri, &mut dash.connected),
            (&mut host.uri, &mut host.connected),
            (&mut nets.uri, &mut nets.connected),
            (&mut store.uri, &mut store.connected),
            (&mut events_state.uri, &mut events_state.connected),
        ] {
            *u = cli.connect.clone();
            *c = true;
        }
    }
    let wizard: Option<ui::overlays::wizard::WizardState> = None;
    let settings: Option<ui::views::settings::SettingsState> = None;
    let palette: Option<ui::overlays::palette::PaletteState> = None;
    let help_filter: Option<String> = None;
    let events_rx = if cli.demo {
        None
    } else {
        Some(crate::backend::virsh::events::subscribe(&cli.connect))
    };
    crate::config::set_runtime(&config);
    let start_view = if cli.screen.is_some() {
        ""
    } else {
        config.general.start_view.as_str()
    };
    let view = match cli.screen.as_deref().unwrap_or(start_view) {
        "events" => View::Events,
        "host" => View::Host,
        "networks" => View::Networks,
        "storage" => View::Storage,
        "gallery" => View::Gallery,
        _ => View::Dashboard,
    };

    // --dump: render one frame to an ANSI file and exit.
    if let Some(path) = &cli.dump {
        let (w, h) = parse_size(&cli.size);
        dump_frame(
            &theme,
            &state,
            &dash,
            &host,
            &nets,
            &store,
            cli.screen.as_deref(),
            path,
            w,
            h,
        )?;
        return Ok(());
    }

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = TerminalGuard(Some(Terminal::new(backend)?));
    terminal.clear()?;

    let mut ticker = interval(Duration::from_secs(1));
    let mut events = external::spawn_input_thread();
    let pending_quit_z = false;
    let (keymap, keymap_warnings) = crate::input::keymap::load();
    let engine = crate::input::engine::KeyEngine::with_map(keymap);
    // Settings › Keybindings edits this file; the demo reads it but never saves.
    let keymap_file = crate::input::keymap_file::KeymapFile::load(
        if cli.demo {
            None
        } else {
            crate::input::keymap::path()
        },
        crate::input::keymap::path().as_deref(),
    );
    let history = crate::command::history::History::new(200);
    let uri = cli.connect.clone();
    let dry_run = cli.dry_run;
    let demo = cli.demo;
    let confirm: Option<crate::command::plan::PendingConfirm> = None;
    let ex_line: Option<crate::input::textinput::TextInput> = None;
    let mut messages = MessageLog::default();
    messages.push(dash.message.clone());
    if let Some(w) = keymap_warnings.first() {
        dash.message = format!("✗ {w}");
        messages.extend(keymap_warnings.iter().cloned());
    }
    if let Some(error) = &config.load_error {
        dash.message = format!("✗ invalid config, using defaults (changes are not saved): {error}");
        messages.push(dash.message.clone());
    }
    let last_key = std::time::Instant::now();
    let which_key_since: Option<std::time::Instant> = None;
    let detail: Option<ui::views::detail::DetailState> = None;
    let poller: Option<poller::Poller> = None;
    let poller_uri = String::new();
    let sent_selection: Option<(String, bool)> = None;
    let prev_view = view;
    let mut entered: std::collections::HashSet<View> = std::collections::HashSet::new();
    entered.insert(view);
    let (loads_tx, mut loads_rx) = tokio::sync::mpsc::unbounded_channel();

    let mut app = App {
        theme,
        dash,
        host,
        nets,
        store,
        events_state,
        wizard,
        settings,
        palette,
        help_filter,
        messages_view: None,
        job_finished_at: None,
        help_page: 0,
        help_scroll: 0,
        confirm_scroll: std::cell::Cell::new(0),
        viewport: std::cell::Cell::new(None),
        ex_history: Vec::new(),
        ex_history_pos: None,
        events_rx,
        view,
        pending_quit_z,
        replaying_z: false,
        engine,
        keymap_file,
        history,
        uri,
        dry_run,
        demo,
        confirm,
        ex_line,
        messages,
        last_key,
        which_key_since,
        detail,
        poller,
        poller_uri,
        sent_selection,
        prev_view,
        entered,
        loads_tx,
        config,
        completion: None,
        virsh_index: None,
        virsh_flags: std::collections::HashMap::new(),
        active_job: None,
        next_job_id: 1,
        busy_feedback: None,
        read_serial: 0,
        selection_task: std::collections::HashMap::new(),
        read_serials: std::collections::HashMap::new(),
        connection_epoch: 0,
        epoch_uri: cli.connect.clone(),
        view_load_serial: std::collections::HashMap::new(),
        view_load_tasks: std::collections::HashMap::new(),
        view_load_pending: std::collections::HashSet::new(),
        view_load_refreshing: std::collections::HashSet::new(),
        help_task: None,
        help_docs: std::collections::HashMap::new(),
        input_revision: 0,
        wizard_revision: 0,
        viewer_task: None,
        detail_loading: false,
        picker_loading: false,
        pending_snapshot_modal: None,
        hardware_reload_required: None,
        editor_retries: Vec::new(),
        editor_generation: 0,
        network_retries: Vec::new(),
    };
    if !cli.demo {
        for view in [View::Host, View::Networks, View::Storage] {
            app.request_view_load(view);
        }
    }
    let mut shutdown_open = true;
    loop {
        if let Some(job) = app.active_job.as_mut() {
            for _ in 0..64 {
                match job.events.try_recv() {
                    Ok(progress) => job.progress = Some(progress),
                    Err(_) => break,
                }
            }
        }
        // After $EDITOR / console: repaint every cell, not just the diff.
        if external::take_full_redraw() {
            terminal.clear()?;
        }
        terminal.draw(|f| app.draw(f))?;
        app.sync_poller();
        tokio::select! {
            changed = shutdown.changed(), if shutdown_open => {
                if changed.is_err() { shutdown_open = false; }
                else if shutdown.borrow().is_some() {
                    app.shutdown_active_job().await;
                    break;
                }
            }
            Some(msg) = async {
                match app.poller.as_mut() {
                    Some(p) => p.rx.recv().await,
                    None => std::future::pending().await,
                }
            } => app.on_live(msg),
            Some(loaded) = loads_rx.recv() => app.on_loaded(loaded),
            help = async {
                match app.help_task.as_mut() {
                    Some(task) => (&mut task.task).await,
                    None => std::future::pending().await,
                }
            } => app.apply_help_result(help),
            (kind, loaded) = std::future::poll_fn(|cx| {
                use std::future::Future;
                for (kind, task) in app.selection_task.iter_mut() {
                    if let std::task::Poll::Ready(result) = std::pin::Pin::new(&mut task.0).poll(cx) {
                        return std::task::Poll::Ready((*kind, result));
                    }
                }
                std::task::Poll::Pending
            }) => {
                app.selection_task.remove(&kind);
                if let Ok(result) = loaded { app.apply_selection_result(result); }
                else if kind == ReadKind::Detail && app.detail_loading {
                    app.detail_loading = false;
                    app.pending_snapshot_modal = None;
                    if let Some(detail) = app.detail.as_mut() { detail.message = "Failed to load domain details".into(); }
                } else if kind == ReadKind::Picker && app.picker_loading {
                    app.picker_loading = false;
                    app.store.message = "Failed to detect the CD-ROM drive".into();
                }
            },
            finished = async {
                match app.active_job.as_mut() {
                    Some(job) => (&mut job.task).await,
                    None => std::future::pending().await,
                }
            } => app.finish_job(finished),
            _ = tokio::time::sleep(Duration::from_millis(100)), if app.active_job.is_some() => {},
            _ = ticker.tick() => app.on_tick(),
            _ = tokio::time::sleep_until((app.last_key + Duration::from_millis(180)).into()), if app.engine.has_pending_digits() => {
                app.on_digit_timeout();
            }
            // Wake up to draw the which-key popup instead of waiting for the tick.
            _ = tokio::time::sleep_until(
                (app.which_key_since.unwrap_or(app.last_key) + WHICH_KEY_DELAY).into()
            ), if app.which_key_since.is_some_and(|t| t.elapsed() < WHICH_KEY_DELAY) => {}
            _ = tokio::time::sleep_until((app.last_key + Z_TIMEOUT).into()), if app.pending_quit_z => {
                app.on_z_timeout();
            }
            maybe = events.recv() => {
                match maybe {
                    Some(Event::Key(key)) => {
                        if app.handle_key_event(key) {
                            break;
                        }
                        if std::mem::take(&mut app.dash.redraw_requested) {
                            terminal.clear()?;
                        }
                    }
                    // Terminal resized: repaint everything at the new size.
                    Some(Event::Resize(..)) => terminal.clear()?,
                    // Bracketed paste: text, never keys (a pasted `D` must not destroy).
                    Some(Event::Paste(text)) => app.on_paste(&text),
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct MessageLog {
    lines: Vec<String>,
    actions: Vec<(String, String, bool)>,
}

/// History kept for `:messages` (older lines are dropped).
const MESSAGE_HISTORY: usize = 2000;

impl MessageLog {
    /// Drop the oldest lines beyond [`MESSAGE_HISTORY`].
    fn trim(&mut self) {
        if self.lines.len() > MESSAGE_HISTORY {
            let excess = self.lines.len() - MESSAGE_HISTORY;
            self.lines.drain(..excess);
        }
        if self.actions.len() > MESSAGE_HISTORY {
            let excess = self.actions.len() - MESSAGE_HISTORY;
            self.actions.drain(..excess);
        }
    }

    fn push_action(&mut self, message: String, target: &str, success: bool) {
        self.lines.push(message.clone());
        self.actions.push((message, target.to_string(), success));
    }
}

impl std::ops::Deref for MessageLog {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.lines
    }
}

impl std::ops::DerefMut for MessageLog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.lines
    }
}

/// Everything the event loop owns (views, overlays, input, connection).
struct App {
    theme: Theme,
    dash: ui::views::dashboard::DashboardState,
    host: ui::views::host::HostState,
    nets: ui::views::networks::NetworksState,
    store: ui::views::storage::StorageState,
    events_state: ui::views::events::EventsState,
    wizard: Option<ui::overlays::wizard::WizardState>,
    settings: Option<ui::views::settings::SettingsState>,
    palette: Option<ui::overlays::palette::PaletteState>,
    help_filter: Option<String>,
    /// `:messages` overlay: lines scrolled up from the newest.
    messages_view: Option<usize>,
    /// Help overlay page (0 global keys, 1 view keys) and scroll.
    help_page: u8,
    help_scroll: usize,
    confirm_scroll: std::cell::Cell<usize>,
    viewport: std::cell::Cell<Option<Rect>>,
    /// When the last job finished (a late C-c must not quit).
    job_finished_at: Option<std::time::Instant>,
    /// `:` lines entered this session (Up/Down recall) and the recall position.
    ex_history: Vec<String>,
    ex_history_pos: Option<usize>,
    events_rx: Option<tokio::sync::mpsc::Receiver<crate::model::LibvirtEvent>>,
    view: View,
    pending_quit_z: bool,
    /// True while a timed-out lone `Z` is replayed through the engine.
    replaying_z: bool,
    engine: crate::input::engine::KeyEngine,
    /// `keymap.toml` as edited from Settings › Keybindings.
    keymap_file: crate::input::keymap_file::KeymapFile,
    history: crate::command::history::History,
    uri: String,
    dry_run: bool,
    demo: bool,
    confirm: Option<crate::command::plan::PendingConfirm>,
    ex_line: Option<crate::input::textinput::TextInput>,
    messages: MessageLog,
    last_key: std::time::Instant,
    which_key_since: Option<std::time::Instant>,
    detail: Option<ui::views::detail::DetailState>,
    poller: Option<poller::Poller>,
    poller_uri: String,
    sent_selection: Option<(String, bool)>,
    prev_view: View,
    entered: std::collections::HashSet<View>,
    /// Background Host/Networks/Storage loads report here.
    loads_tx: tokio::sync::mpsc::UnboundedSender<Loaded>,
    config: Config,
    /// Tab completion in progress on the `:` line.
    completion: Option<ExCompletion>,
    /// Cache: virsh subcommands (from `virsh help`) and their flags.
    virsh_index: Option<Vec<(String, String)>>,
    virsh_flags: std::collections::HashMap<String, Vec<(String, String)>>,
    active_job: Option<ActiveJob>,
    next_job_id: u64,
    busy_feedback: Option<std::time::Instant>,
    read_serial: u64,
    selection_task: std::collections::HashMap<requests::ReadKind, SelectionTask>,
    /// Current serial per read kind (see [`requests::ReadKind`]).
    read_serials: std::collections::HashMap<requests::ReadKind, u64>,
    connection_epoch: u64,
    epoch_uri: String,
    view_load_serial: std::collections::HashMap<View, u64>,
    view_load_tasks: std::collections::HashMap<View, ViewLoadTask>,
    view_load_pending: std::collections::HashSet<View>,
    view_load_refreshing: std::collections::HashSet<View>,
    help_task: Option<HelpTask>,
    help_docs: std::collections::HashMap<String, Option<crate::backend::virsh::parse_help::HelpDoc>>,
    input_revision: u64,
    wizard_revision: u64,
    viewer_task: Option<ViewLoadTask>,
    detail_loading: bool,
    picker_loading: bool,
    pending_snapshot_modal: Option<(u64, ui::views::detail::SnapModal, bool)>,
    hardware_reload_required: Option<String>,
    editor_retries: Vec<EditorDraft>,
    editor_generation: u64,
    network_retries: Vec<(String, crate::ui::widgets::form::Form)>,
}

/// Active Tab completion: the line before the token, candidates, selection.
#[derive(Debug, Clone)]
pub(crate) struct ExCompletion {
    pub base: String,
    pub items: Vec<crate::input::complete::Item>,
    pub idx: usize,
}

impl App {
    /// What `:` completion can offer right now.
    fn completion_context(&mut self) -> crate::input::complete::Context {
        let mut themes: Vec<String> = crate::theme::builtin::all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        themes.extend(ui::views::settings::custom_names());
        crate::input::complete::Context {
            domains: self
                .dash
                .rows
                .iter()
                .map(|r| (r.name.clone(), r.state_label.to_string()))
                .collect(),
            networks: self.nets.networks.iter().map(|n| n.name.clone()).collect(),
            pools: self.store.pools.iter().map(|p| p.name.clone()).collect(),
            themes,
            uris: self.config.connections.uris.clone(),
            virsh: self.virsh_index.clone().unwrap_or_default(),
        }
    }

    /// Tab (or Shift-Tab) on the `:` line: start or cycle completion.
    fn tab_complete(&mut self, back: bool) {
        let Some(ex) = self.ex_line.as_mut() else { return };
        if let Some(c) = self.completion.as_mut() {
            let n = c.items.len();
            c.idx = if back {
                (c.idx + n - 1) % n
            } else {
                (c.idx + 1) % n
            };
            ex.set(&format!("{}{}", c.base, c.items[c.idx].value));
            return;
        }
        let line = ex.value().to_string();
        let ctx = self.completion_context();
        if !self.demo && self.virsh_index.is_none() {
            self.request_help(line, back, None);
            return;
        }
        if !self.demo {
            let tokens: Vec<_> = line.split_whitespace().collect();
            if !line.ends_with(char::is_whitespace)
                && tokens.last().is_some_and(|token| token.starts_with('-'))
                && let Some(first) = tokens.first()
            {
                let command = if first.trim_start_matches('!') == "virsh" {
                    tokens.get(1).copied().unwrap_or("")
                } else {
                    *first
                };
                if !command.is_empty() && !self.virsh_flags.contains_key(command) {
                    self.request_help(line.clone(), back, Some(command.to_string()));
                    return;
                }
            }
        }
        let flags = |cmd: &str| self.virsh_flags.get(cmd).cloned().unwrap_or_default();
        let (start, items) = crate::input::complete::complete(&line, &ctx, &flags);
        let Some(ex) = self.ex_line.as_mut() else { return };
        match items.len() {
            0 => self.dash.message = String::from("no completions"),
            1 => {
                // A single match is applied directly (with a trailing space
                // unless it is a directory or a `key=`).
                let v = &items[0].value;
                let sep = if v.ends_with('/') || v.ends_with('=') {
                    ""
                } else {
                    " "
                };
                ex.set(&format!("{}{v}{sep}", &line[..start]));
            }
            _ => {
                let idx = if back { items.len() - 1 } else { 0 };
                ex.set(&format!("{}{}", &line[..start], items[idx].value));
                self.completion = Some(ExCompletion {
                    base: line[..start].to_string(),
                    items,
                    idx,
                });
            }
        }
    }

    /// Run the staged confirmation's plan and refresh the view that asked.
    fn run_confirmed(&mut self) {
        let Some((_, build, targets)) = self.confirm.take() else {
            return;
        };
        if !self.demo && !self.dry_run {
            self.submit_plans(
                targets
                    .iter()
                    .map(|target| (target.clone(), build(target)))
                    .collect(),
            );
            return;
        }
        for target in &targets {
            let plan = build(target);
            let first = plan
                .steps
                .first()
                .map(crate::command::display::display)
                .unwrap_or_default();
            self.history.push(&first);
            let prefix = if self.demo { "✓" } else { "[dry-run]" };
            let message = format!("{prefix} {}  ── {first}", plan.summary);
            self.dash.message = message.clone();
            self.messages.push_action(message, target, true);
            if self.demo {
                apply_demo_transition(&mut self.dash, target, &plan.summary);
            }
        }
        let msg = self.dash.message.clone();
        match self.view {
            View::Networks => {
                self.nets.message = msg;
            }
            View::Storage => {
                self.store.message = msg;
            }
            View::Detail => {
                if let Some(d) = self.detail.as_mut() {
                    d.message = msg;
                }
            }
            _ => {}
        }
    }

    /// Settings › Confirmations: skip the modal for actions the user turned off,
    /// and drop the type-the-name step when that is disabled.
    fn apply_confirm_settings(&mut self) {
        let Some((c, _, _)) = self.confirm.as_mut() else {
            return;
        };
        let cfg = crate::config::confirmations();
        use crate::ui::overlays::confirm::ConfirmKind as K;
        if c.kind == K::DomainUndefine && !cfg.type_name_for_undefine {
            c.type_name = None;
        }
        let ask = match c.kind {
            K::DomainDestroy => cfg.destroy,
            K::DomainReset => cfg.reset,
            K::DomainUndefine => cfg.undefine,
            K::VolumeDelete => cfg.delete_volume,
            K::VolumeWipe => cfg.wipe,
            K::Always => true,
        };
        if !ask {
            self.run_confirmed();
        }
    }

    /// Apply a `/` filter to the current view.
    fn apply_filter(&mut self, text: String) {
        match self.view {
            View::Events => {
                self.events_state.query = text;
                self.events_state.selected = 0;
            }
            _ => self.dash.set_filter(&text),
        }
    }

    /// Show a message in the message line of the current view.
    fn show_message(&mut self, msg: String) {
        match self.view {
            View::Networks => self.nets.message = msg,
            View::Storage => self.store.message = msg,
            View::Host => self.host.message = msg,
            View::Detail => {
                if let Some(d) = self.detail.as_mut() {
                    d.message = msg;
                }
            }
            _ => self.dash.message = msg,
        }
    }

    /// Draw one frame (views, overlays, chrome).
    fn draw(&self, f: &mut ratatui::Frame) {
        self.viewport.set(Some(f.area()));
        // Paint the whole frame first: views leave margins and gutters
        // untouched, and a cell left at the terminal default shows the host
        // terminal's own background instead of the theme's.
        f.render_widget(
            ratatui::widgets::Block::default().style(self.theme.base()),
            f.area(),
        );
        match self.view {
            View::Gallery => ui::views::gallery::render(f, &self.theme),
            View::Host => ui::views::host::render(f, &self.theme, &self.host),
            View::Networks => ui::views::networks::render(f, &self.theme, &self.nets),
            View::Events => ui::views::events::render(f, &self.theme, &self.events_state),
            View::Storage => {
                ui::views::storage::render(f, &self.theme, &self.store, self.store.picker.is_some())
            }
            View::Dashboard => ui::views::dashboard::render(f, &self.theme, &self.dash),
            View::Detail => {
                if let Some(d) = &self.detail {
                    ui::views::detail::render(f, &self.theme, d);
                }
            }
        }
        if self.view != View::Storage
            && let Some(picker) = &self.store.picker
            && crate::ui::layout::breakpoint(f.area().width, f.area().height)
                != crate::ui::layout::Breakpoint::TooSmall
        {
            crate::ui::widgets::modal::dim_background(f, &self.theme);
            ui::views::storage::render_picker(f, &self.theme, &self.store, picker);
        }
        // Below the smallest layout only the "too small" panel is drawn:
        // overlays would not fit (and their geometry would underflow).
        let too_small = crate::ui::layout::breakpoint(f.area().width, f.area().height)
            == crate::ui::layout::Breakpoint::TooSmall;
        if !matches!(self.view, View::Gallery) && !too_small {
            if self.confirm.is_some()
                || self.wizard.is_some()
                || self.palette.is_some()
                || self.help_filter.is_some()
                || self.messages_view.is_some()
            {
                crate::ui::widgets::modal::dim_background(f, &self.theme);
            }
            if let Some((c, _, _)) = &self.confirm {
                let max = crate::ui::overlays::confirm::render_scrolled(
                    f,
                    &self.theme,
                    f.area(),
                    c,
                    self.confirm_scroll.get(),
                );
                self.confirm_scroll.set(self.confirm_scroll.get().min(max));
            }
            if let Some(ex) = &self.ex_line {
                render_ex_line(f, &self.theme, ex);
                if let Some(c) = &self.completion {
                    render_completion(f, &self.theme, c);
                }
            }
            if let Some(prefix) = self.engine.pending_prefix()
                && self
                    .which_key_since
                    .is_some_and(|t| t.elapsed() >= WHICH_KEY_DELAY)
            {
                render_which_key(f, &self.theme, &prefix, self.engine.map());
            }
            if let Some(wz) = &self.wizard {
                crate::ui::overlays::wizard::render(f, &self.theme, wz);
                render_wizard_status(f, &self.theme, wz);
            }
            if let Some(pal) = &self.palette {
                crate::ui::overlays::palette::render(f, &self.theme, pal);
            }
            if let Some(st) = &self.settings {
                crate::ui::views::settings::render(
                    f,
                    &self.theme,
                    &self.config,
                    st,
                    self.engine.map(),
                    &self.keymap_file,
                );
            }
            if let Some(filter) = &self.help_filter {
                crate::ui::overlays::help::render_page(
                    f,
                    &self.theme,
                    filter,
                    self.engine.map(),
                    self.help_page,
                    self.help_scroll,
                );
            }
            if let Some(scroll) = self.messages_view {
                crate::ui::overlays::messages::render(f, &self.theme, &self.messages, scroll);
            }
        }
        if let Some(job) = &self.active_job
            && self.ex_line.is_none()
            && self
                .busy_feedback
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(2))
        {
            let elapsed = job.started.elapsed();
            let ascii = self.config.appearance.icons == "ascii";
            let spinner = if ascii {
                ["|", "/", "-", "\\"][(elapsed.as_millis() / 100) as usize % 4]
            } else {
                ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"][(elapsed.as_millis() / 100) as usize % 10]
            };
            let (target, step) = match &job.progress {
                Some(progress) => (
                    progress.target.as_str(),
                    format!("step {}/{}", progress.step, progress.total),
                ),
                None => (
                    job.plans.first().map(|(target, _)| target.as_str()).unwrap_or(""),
                    "preparing".into(),
                ),
            };
            let step = if *job.cancel.borrow() {
                "cancelling".to_string()
            } else {
                step
            };
            let text = format!(
                "{spinner} Job #{} · {target} · {step} · {}s · C-c cancel",
                job.id,
                elapsed.as_secs()
            );
            let area = f.area();
            if area.height > 0 {
                let row = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
                f.render_widget(ratatui::widgets::Clear, row);
                f.render_widget(
                    ratatui::widgets::Paragraph::new(text).style(Style::default().fg(self.theme.tokens.blue)),
                    row,
                );
            }
        }
        crate::ui::widgets::modal::apply_transparency(f, &self.theme);
    }

    /// Keep the background poller in sync with the URI and the selection.
    fn sync_poller(&mut self) {
        // Restart the poller when the connection URI changes (`:connect`).
        if self.epoch_uri != self.uri {
            self.connection_epoch = self.connection_epoch.wrapping_add(1);
            self.epoch_uri.clone_from(&self.uri);
            self.view_load_tasks.clear();
            self.view_load_pending.clear();
            self.view_load_refreshing.clear();
            self.help_task.take();
            self.viewer_task.take();
            self.virsh_index = None;
            self.virsh_flags.clear();
            self.help_docs.clear();
            if !self.demo {
                self.events_rx = Some(crate::backend::virsh::events::subscribe(&self.uri));
            }
        }
        if !self.demo && self.poller_uri != self.uri {
            self.cancel_all_reads();
            self.poller = Some(poller::spawn(
                &self.uri,
                self.config.general.refresh_interval_secs.max(1),
                (!self.dry_run && self.config.monitoring.balloon_on)
                    .then_some(self.config.monitoring.balloon_period_secs.max(1)),
            ));
            self.poller_uri.clone_from(&self.uri);
            self.sent_selection = None;
        }
        // Tell the poller which domain is selected (config + IP are fetched for it).
        if let Some(p) = &self.poller {
            let sel = self
                .current_detail()
                .map(|d| (d.domain.clone(), d.state == crate::model::DomainState::Running))
                .unwrap_or_else(|| {
                    let r = self.dash.selected();
                    (r.name.clone(), r.state == crate::model::DomainState::Running)
                });
            if !sel.0.is_empty() && self.sent_selection.as_ref() != Some(&sel) {
                let _ = p.tx.send(poller::Request::Selected {
                    name: sel.0.clone(),
                    running: sel.1,
                });
                self.sent_selection = Some(sel);
            }
        }
    }

    /// Apply live data from the poller.
    fn on_live(&mut self, msg: LiveMsg) {
        apply_live_msg(&self.uri, &mut self.dash, msg);
        // Bound coalescing even when the producer keeps refilling the channel.
        for _ in 0..64 {
            let Some(msg) = self.poller.as_mut().and_then(|p| p.rx.try_recv().ok()) else {
                break;
            };
            apply_live_msg(&self.uri, &mut self.dash, msg);
        }
        if self.view == View::Host {
            self.host.apply_live(&self.dash);
            if !self.config.monitoring.thread_sampling {
                self.host.threads.clear();
            }
        }
        if self.view == View::Networks && self.nets.live {
            let bridge = self
                .nets
                .networks
                .get(self.nets.selected)
                .map(|n| n.cfg.bridge.clone())
                .unwrap_or_default();
            self.nets.traffic = self.dash.metrics.iface(&bridge).map(|s| {
                let both: Vec<f32> = s.rx.values().into_iter().chain(s.tx.values()).collect();
                let max = both.iter().copied().fold(125_000.0, f32::max);
                ui::views::networks::NetTraffic {
                    iface: bridge.clone(),
                    rx: crate::metrics::store::normalize(&s.rx.values(), max),
                    tx: crate::metrics::store::normalize(&s.tx.values(), max),
                    rx_bps: s.rx_bps,
                    tx_bps: s.tx_bps,
                }
            });
        }
        if let Some(d) = self.detail.as_mut() {
            d.live_series = self.dash.metrics.domain(&d.domain).cloned();
            if let Some(r) = self.dash.rows.iter().find(|r| r.name == d.domain) {
                d.state = r.state;
                d.uptime.clone_from(&r.uptime);
            }
        }
    }

    /// One-second housekeeping: events, first-enter refreshes.
    fn on_tick(&mut self) {
        self.messages.trim();
        // Flush a throttled wizard draft.
        if let Some(wizard) = &self.wizard {
            wizard.autosave();
        }
        for notice in external::take_notices() {
            self.messages.push(notice.clone());
            self.show_message(notice);
        }
        if let Some(rx) = self.events_rx.as_mut() {
            for _ in 0..512 {
                let Ok(ev) = rx.try_recv() else { break };
                self.dash.recent_events.push(ev.clone());
                if self.dash.recent_events.len() > 50 {
                    self.dash.recent_events.remove(0);
                }
                self.events_state.push(ev);
            }
        }
        self.enter_view_if_changed();
    }

    /// Refresh Host/Networks/Storage every time they are entered (they are
    /// not polled every second like the domain list). Called right after a
    /// key, so entering a view does not wait for the next tick.
    fn enter_view_if_changed(&mut self) {
        if self.view != self.prev_view {
            self.prev_view = self.view;
            if !self.demo {
                self.entered.insert(self.view);
                self.request_view_load(self.view);
            }
        }
    }

    /// A background view load finished: swap it in (keeping the status
    /// message and, when the item still exists, the selection).
    fn on_loaded(&mut self, loaded: Loaded) {
        let (view, uri, epoch, serial) = match &loaded {
            Loaded::Host(uri, epoch, serial, _) => (View::Host, uri, *epoch, *serial),
            Loaded::Nets(uri, epoch, serial, _) => (View::Networks, uri, *epoch, *serial),
            Loaded::Store(uri, epoch, serial, _) => (View::Storage, uri, *epoch, *serial),
            Loaded::Failed(view, uri, epoch, serial) => (*view, uri, *epoch, *serial),
        };
        let current = uri == &self.uri
            && epoch == self.connection_epoch
            && self.view_load_serial.get(&view) == Some(&serial);
        let followup = current && self.view_load_pending.remove(&view);
        let keep = |old: &mut String, fresh: &mut String| {
            if old != "loading…" && !old.is_empty() {
                *fresh = std::mem::take(old);
            }
        };
        match loaded {
            Loaded::Failed(view, uri, epoch, serial)
                if uri == self.uri
                    && epoch == self.connection_epoch
                    && self.view_load_serial.get(&view) == Some(&serial) =>
            {
                self.view_load_tasks.remove(&view);
                let message = "✗ Failed to load view".to_string();
                match view {
                    View::Host => self.host.message = message,
                    View::Networks => self.nets.message = message,
                    View::Storage => self.store.message = message,
                    _ => {}
                }
            }
            Loaded::Host(uri, epoch, serial, mut fresh)
                if uri == self.uri
                    && epoch == self.connection_epoch
                    && self.view_load_serial.get(&View::Host) == Some(&serial) =>
            {
                self.view_load_tasks.remove(&View::Host);
                keep(&mut self.host.message, &mut fresh.message);
                fresh.selected = self.host.selected.min(fresh.domains.len().saturating_sub(1));
                self.host = *fresh;
            }
            Loaded::Nets(uri, epoch, serial, mut fresh)
                if uri == self.uri
                    && epoch == self.connection_epoch
                    && self.view_load_serial.get(&View::Networks) == Some(&serial) =>
            {
                self.view_load_tasks.remove(&View::Networks);
                let selected = self.nets.selected_name().to_string();
                keep(&mut self.nets.message, &mut fresh.message);
                fresh.focus = self.nets.focus;
                fresh.new_form = self.nets.new_form.take();
                let previous_loaded = fresh.selected_name().to_string();
                if let Some(index) = fresh.networks.iter().position(|net| net.name == selected) {
                    fresh.selected = index;
                }
                let needs_selection = fresh.selected_name() != previous_loaded;
                if needs_selection {
                    fresh.leases.clear();
                    fresh.attached.clear();
                    fresh.lease_selected = 0;
                }
                self.nets = *fresh;
                if needs_selection && self.view == View::Networks {
                    self.request_selection_after_load();
                }
            }
            Loaded::Store(uri, epoch, serial, mut fresh)
                if uri == self.uri
                    && epoch == self.connection_epoch
                    && self.view_load_serial.get(&View::Storage) == Some(&serial) =>
            {
                self.view_load_tasks.remove(&View::Storage);
                let selected = self.store.selected_pool().to_string();
                // Keep the cursor on the same volume across refreshes and jobs:
                // the next X/W/R must not silently target the first row.
                let selected_volume = self.store.selected_vol().map(|v| v.name.clone());
                keep(&mut self.store.message, &mut fresh.message);
                fresh.focus_pools = self.store.focus_pools;
                fresh.picker = self.store.picker.take();
                if let Some(index) = fresh.pools.iter().position(|pool| pool.name == selected) {
                    fresh.pool_selected = index;
                }
                fresh.show_pool_volumes(selected_volume.as_deref());
                self.store = *fresh;
            }
            _ => {} // stale: the connection changed meanwhile
        }
        if followup {
            self.view_load_refreshing.insert(view);
            self.request_view_load(view);
        } else if current && self.view_load_serial.get(&view) == Some(&serial) {
            self.view_load_refreshing.remove(&view);
        }
    }

    /// Pasted text goes into the line being edited; elsewhere it is ignored.
    fn on_paste(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if let Some(ex) = self.ex_line.as_mut() {
            for c in text.chars() {
                ex.insert(c);
            }
            self.input_revision = self.input_revision.wrapping_add(1);
        } else if let Some(filter) = self.help_filter.as_mut() {
            filter.push_str(&text);
        } else if let Some(palette) = self.palette.as_mut() {
            palette.query.push_str(&text);
        } else if let Some(picker) = self.store.picker.as_mut() {
            picker.query.push_str(&text);
            picker.selected = 0;
        } else {
            self.show_message(String::from(
                "paste ignored: open the : line (or a text field) first",
            ));
        }
    }

    /// The domain is running or paused (so `--live` applies).
    pub(super) fn domain_is_active(&self, name: &str) -> bool {
        self.dash.rows.iter().any(|row| {
            row.name == name
                && matches!(
                    row.state,
                    crate::model::DomainState::Running | crate::model::DomainState::Paused
                )
        })
    }

    /// Stage an always-asked confirmation for ready-made plans.
    pub(super) fn confirm_plans(
        &mut self,
        title: &str,
        verb: &str,
        plans: Vec<(String, crate::command::plan::CommandPlan)>,
    ) {
        let commands = plans
            .iter()
            .flat_map(|(_, plan)| plan.steps.iter().map(crate::command::display::display))
            .collect();
        let targets: Vec<String> = plans.iter().map(|(target, _)| target.clone()).collect();
        let by_target: std::collections::HashMap<String, crate::command::plan::CommandPlan> =
            plans.into_iter().collect();
        let build: crate::command::plan::PlanBuilder =
            std::sync::Arc::new(move |target: &str| by_target.get(target).cloned().unwrap_or_default());
        self.confirm = Some((
            crate::ui::overlays::confirm::Confirm {
                kind: crate::ui::overlays::confirm::ConfirmKind::Always,
                title: title.to_string(),
                question: format!("{} {}?", capitalize(verb), targets.join(", ")),
                commands,
                confirm_label: format!("y {verb}"),
                type_name: None,
                typed: String::new(),
            },
            build,
            targets,
        ));
    }

    /// The detail state, only while the Detail view is shown. Outside it
    /// `self.detail` may hold a domain the user has left: never a target.
    pub(super) fn current_detail(&self) -> Option<&ui::views::detail::DetailState> {
        self.detail.as_ref().filter(|_| self.view == View::Detail)
    }

    /// Domain the user is acting on: the Detail domain, else the dashboard cursor.
    pub(super) fn current_domain_name(&self) -> String {
        self.current_detail()
            .map(|detail| detail.domain.clone())
            .unwrap_or_else(|| self.dash.selected().name.clone())
    }

    /// True (and says so) when leaving Detail would drop unsaved hardware edits.
    fn blocks_leaving_detail(&mut self) -> bool {
        let pending = self
            .current_detail()
            .map_or(0, |detail| detail.hw.pending_count());
        if pending == 0 {
            return false;
        }
        self.show_message(String::from("unsaved changes (:w apply, :q! discard)"));
        true
    }

    /// A lone `Z` (no second `Z` within [`Z_TIMEOUT`]) resolves through the
    /// keymap (managed-save by default, behind a confirmation).
    fn on_z_timeout(&mut self) {
        if !self.pending_quit_z || self.last_key.elapsed() < Z_TIMEOUT {
            return;
        }
        self.pending_quit_z = false;
        self.replaying_z = true;
        self.handle_key_event(KeyEvent::new(KeyCode::Char('Z'), KeyModifiers::SHIFT));
        self.replaying_z = false;
    }

    /// Resolve pending count digits after a short idle timeout.
    fn on_digit_timeout(&mut self) {
        if self.last_key.elapsed() >= Duration::from_millis(180) {
            let previous_view = self.view;
            for resolved in self.engine.flush() {
                if is_view_switch(resolved.action) && self.blocks_leaving_detail() {
                    continue;
                }
                switch_view(resolved.action, &mut self.view);
            }
            self.cancel_detail_load_if_left(previous_view);
            self.enter_view_if_changed();
        }
    }

    fn cancel_detail_load_if_left(&mut self, previous_view: View) {
        if previous_view == View::Detail && self.view != View::Detail && self.detail_loading {
            self.detail_loading = false;
            self.pending_snapshot_modal = None;
            self.cancel_read(ReadKind::Detail);
        }
    }

    /// Handle one key press. Returns true when the app should quit.
    #[cfg(test)]
    async fn on_key(&mut self, key: KeyEvent) -> bool {
        self.handle_key_event(key)
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return false;
        }
        if let Some(job) = &self.active_job {
            let id = job.id;
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                self.cancel_job();
                return false;
            }
            if self.wizard.is_none()
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == KeyCode::Char('r')
            {
                self.request_refresh();
                return false;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && key.code == KeyCode::Char('p')
                && self.palette.is_none()
                && self.wizard.is_none()
            {
                self.open_async_palette();
                return false;
            }
            if let Some(ex) = self.ex_line.as_ref()
                && !ex.filter
                && key.code == KeyCode::Enter
            {
                let line = ex.value().to_string();
                self.ex_line = None;
                self.input_revision = self.input_revision.wrapping_add(1);
                match crate::input::ex::parse(&line) {
                    Ok(crate::input::ex::ExCommand::Messages) => self.messages_view = Some(0),
                    Ok(crate::input::ex::ExCommand::Help(topic)) => self.help_filter = Some(topic),
                    Ok(crate::input::ex::ExCommand::Quit | crate::input::ex::ExCommand::QuitAll) => self
                        .show_message(format!(
                            "⚠ Job #{id} is running; press C-c to cancel before quitting"
                        )),
                    _ => self.show_message(format!("⚠ Job #{id} is running; wait or press C-c to cancel")),
                }
                self.busy_feedback = Some(std::time::Instant::now());
                return false;
            }
            let editing_ex = self.ex_line.as_ref().is_some_and(|ex| !ex.filter);
            let filtering = self.ex_line.as_ref().is_some_and(|ex| ex.filter);
            let navigation = matches!(
                key.code,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Tab
                    | KeyCode::BackTab
                    | KeyCode::Esc
                    | KeyCode::Char(
                        'j' | 'k' | 'g' | 'G' | 'H' | 'L' | '[' | ']' | '1'..='9' | '/' | '?' | ',' | ':'
                    )
            );
            let help_input = self.help_filter.is_some()
                && matches!(key.code, KeyCode::Char(_) | KeyCode::Esc | KeyCode::Backspace)
                && key.modifiers.is_empty();
            let redraw = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l');
            let back =
                self.view == View::Detail && key.code == KeyCode::Char('q') && key.modifiers.is_empty();
            let network_draft = self.view == View::Networks
                && ((self.nets.new_form.is_some() && key.code != KeyCode::Enter)
                    || (self.nets.new_form.is_none()
                        && key.code == KeyCode::Char('n')
                        && key.modifiers.is_empty()));
            let palette_input = self.palette.is_some() && !matches!(key.code, KeyCode::Enter);
            let settings_input = self.settings.is_some()
                && key.modifiers.is_empty()
                && matches!(key.code, KeyCode::Char('h' | 'l'));
            let wizard_input = self.wizard.as_ref().is_some_and(|wizard| {
                wizard.command_review
                    || (!(key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('e'))
                        && (key.code != KeyCode::Enter || wizard.step < 5 || !wizard.problems().is_empty()))
            });
            let wizard_shortcut = if self.wizard.is_none()
                && self.settings.is_none()
                && self.palette.is_none()
                && self.help_filter.is_none()
                && self.ex_line.is_none()
                && (self.view != View::Networks || self.nets.new_form.is_none())
                && self.store.picker.is_none()
                && !self.engine.has_pending_digits()
            {
                let token = crate::input::key::KeySeq::from_event(&key);
                let mut candidate: Vec<String> = self
                    .engine
                    .pending_prefix()
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(str::to_string)
                    .collect();
                candidate.push(token.token().to_string());
                match self.engine.map().get(&candidate) {
                    Some(action) => *action == crate::input::engine::KeyAction::NewDomain,
                    None => self.engine.map().iter().any(|(keys, action)| {
                        *action == crate::input::engine::KeyAction::NewDomain && keys.starts_with(&candidate)
                    }),
                }
            } else {
                false
            };
            if !settings_input
                && !wizard_input
                && !wizard_shortcut
                && !network_draft
                && !editing_ex
                && !filtering
                && !help_input
                && !redraw
                && !navigation
                && !back
                && !palette_input
            {
                if self.engine.pending_prefix().is_some() || self.engine.has_pending_digits() {
                    let escape = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
                    self.engine.feed(&crate::input::key::KeySeq::from_event(&escape));
                }
                let message = if matches!(key.code, KeyCode::Char('q' | 'Z')) {
                    format!("⚠ Job #{id} is running; press C-c to cancel before quitting")
                } else {
                    format!("⚠ Job #{id} is running; wait or press C-c to cancel")
                };
                self.show_message(message);
                self.busy_feedback = Some(std::time::Instant::now());
                return false;
            }
        }
        if self.view == View::Detail && self.detail_loading && self.help_filter.is_none() {
            let allowed = key.modifiers.is_empty()
                && matches!(
                    key.code,
                    KeyCode::Esc
                        | KeyCode::Up
                        | KeyCode::Down
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::Tab
                        | KeyCode::BackTab
                        | KeyCode::Char('q' | 'j' | 'k' | 'h' | 'l' | 'g' | 'G' | '?' | '1'..='9')
                );
            let redraw = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l');
            if !allowed && !redraw {
                self.show_message("Loading domain details…".into());
                return false;
            }
        }
        if self.picker_loading
            && self.store.picker.is_some()
            && (key.code == KeyCode::Enter
                || (key.code == KeyCode::Char('e') && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            self.show_message("Loading CD-ROM details…".into());
            return false;
        }
        let previous_view = self.view;
        let had_picker = self.store.picker.is_some();
        let previous_wizard = self.wizard.as_ref().map(|wizard| wizard.content_snapshot());
        let previous_input = self
            .ex_line
            .as_ref()
            .map(|input| (input.value().to_string(), input.cursor(), input.filter));
        let had_confirm = self.confirm.is_some();
        let snap_modal = |app: &Self| {
            app.current_detail()
                .map(|detail| detail.snap_modal)
                .unwrap_or(ui::views::detail::SnapModal::None)
        };
        let modal_before = snap_modal(self);
        let quit = self.on_key_inner(key);
        if modal_before != snap_modal(self)
            && let Some(detail) = &self.detail
        {
            detail.snaps.review_scroll.set(0);
        }
        self.enter_view_if_changed();
        // Settings › Confirmations › snapshot revert off: run the revert with
        // the modal's defaults instead of asking.
        if modal_before != ui::views::detail::SnapModal::Revert
            && snap_modal(self) == ui::views::detail::SnapModal::Revert
            && !crate::config::confirmations().revert
        {
            self.on_key_inner(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        }
        if had_picker && self.store.picker.is_none() && self.picker_loading {
            self.picker_loading = false;
            self.cancel_read(ReadKind::Picker);
        }
        self.cancel_detail_load_if_left(previous_view);
        let current_wizard = self.wizard.as_ref().map(|wizard| wizard.content_snapshot());
        if previous_wizard != current_wizard {
            self.wizard_revision = self.wizard_revision.wrapping_add(1);
        }
        let current_input = self
            .ex_line
            .as_ref()
            .map(|input| (input.value().to_string(), input.cursor(), input.filter));
        if previous_input != current_input {
            self.input_revision = self.input_revision.wrapping_add(1);
        }
        if !had_confirm && self.confirm.is_some() {
            self.confirm_scroll.set(0);
            self.apply_confirm_settings();
        }
        for (message, target, success) in std::mem::take(&mut self.messages.actions) {
            self.record_action_event(&message, &target, success);
        }
        quit
    }

    fn record_action_event(&mut self, message: &str, target: &str, success: bool) {
        let (head, _) = message.split_once("  ── ").unwrap_or((message, ""));
        let event = crate::model::LibvirtEvent {
            timestamp: jiff::Zoned::now().strftime("%H:%M:%S").to_string(),
            object: target.to_string(),
            event: String::from(if success { "done" } else { "failed" }),
            detail: crate::model::sanitize(head.trim_start_matches(['✓', '✗', ' '])),
            kind: String::from("action"),
            scope: String::from("app"),
        };
        self.dash.recent_events.push(event.clone());
        if self.dash.recent_events.len() > 50 {
            self.dash.recent_events.remove(0);
        }
        self.events_state.push(event);
    }

    /// Key handling proper (see `on_key`).
    fn on_key_inner(&mut self, key: KeyEvent) -> bool {
        // Ignore key release/repeat reports (kitty protocol, Windows).
        if key.kind != crossterm::event::KeyEventKind::Press {
            return false;
        }
        self.last_key = std::time::Instant::now();
        if self.viewport.get().is_some_and(|area| {
            ui::layout::breakpoint(area.width, area.height) == ui::layout::Breakpoint::TooSmall
        }) {
            // No action may run while its UI is hidden. Escape still cancels.
            if key.code == KeyCode::Esc {
                self.confirm = None;
                if let Some(detail) = &mut self.detail {
                    detail.snap_modal = ui::views::detail::SnapModal::None;
                }
            }
            return key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        }
        let scroll_delta = match key.code {
            KeyCode::Down => Some(1isize),
            KeyCode::Up => Some(-1),
            KeyCode::PageDown => Some(5),
            KeyCode::PageUp => Some(-5),
            KeyCode::Home => Some(isize::MIN),
            KeyCode::End => Some(isize::MAX),
            _ => None,
        };
        if self.confirm.is_some()
            && let Some(delta) = scroll_delta
        {
            self.confirm_scroll
                .set(self.confirm_scroll.get().saturating_add_signed(delta));
            return false;
        }
        if let Some(detail) = self
            .detail
            .as_ref()
            .filter(|d| self.view == View::Detail && d.snap_modal != ui::views::detail::SnapModal::None)
            && let Some(delta) = scroll_delta
        {
            detail
                .snaps
                .review_scroll
                .set(detail.snaps.review_scroll.get().saturating_add_signed(delta));
            return false;
        }

        // C-c quits; `q`/`ZZ` are checked only after the overlays that
        // take text input (ex line, palette, wizard, confirm, forms) had their turn.
        // A C-c meant to cancel a job that just finished must not quit: within
        // a short grace it only warns (a second C-c quits).
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self
                .job_finished_at
                .take()
                .is_some_and(|at| at.elapsed() < Duration::from_millis(1500))
            {
                self.show_message(String::from("job already finished; press C-c again to quit"));
                return false;
            }
            return true;
        }
        // Confirm modal captures y/n/Esc.
        if let Some((c, _, _)) = self.confirm.as_mut()
            && c.type_name.is_some()
        {
            // Type-the-name mode: letters are input, only Enter confirms.
            match key.code {
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    c.typed.push(ch);
                    return false;
                }
                KeyCode::Backspace => {
                    c.typed.pop();
                    return false;
                }
                KeyCode::Enter if !c.satisfied() => return false,
                _ => {}
            }
        }
        if self.confirm.is_some() {
            let typed_mode = self
                .confirm
                .as_ref()
                .is_some_and(|(c, _, _)| c.type_name.is_some());
            let cancel = key.code == KeyCode::Esc
                || (!typed_mode && key.code == KeyCode::Char('n') && key.modifiers.is_empty());
            let ok = ((!typed_mode && key.code == KeyCode::Char('y')) || key.code == KeyCode::Enter)
                && key.modifiers.is_empty()
                && self.confirm.as_ref().is_some_and(|(c, _, _)| c.satisfied());
            if cancel {
                self.confirm = None;
            } else if ok {
                self.run_confirmed();
            }
            return false;
        }
        // Ex line editing.
        if let Some(ex) = self.ex_line.as_mut() {
            let typing = !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
            if ex.filter {
                // `/` filter: applies live while typing; ⏎ keeps it, ⎋ clears it.
                match key.code {
                    KeyCode::Esc => {
                        self.ex_line = None;
                        self.apply_filter(String::new());
                    }
                    KeyCode::Enter => self.ex_line = None,
                    KeyCode::Backspace => ex.backspace(),
                    KeyCode::Left => ex.move_left(),
                    KeyCode::Right => ex.move_right(),
                    KeyCode::Char(c) if typing => ex.insert(c),
                    _ => {}
                }
                if let Some(ex) = &self.ex_line {
                    let text = ex.value().to_string();
                    self.apply_filter(text);
                }
                return false;
            }
            // Tab / Shift-Tab: complete the token at the end of the line.
            if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                self.tab_complete(key.code == KeyCode::BackTab);
                return false;
            }
            self.completion = None;
            let Some(ex) = self.ex_line.as_mut() else {
                return false;
            };
            match key.code {
                KeyCode::Esc => self.ex_line = None,
                KeyCode::Enter => {
                    let line = ex.value().to_string();
                    self.ex_line = None;
                    self.ex_history_pos = None;
                    if !line.trim().is_empty() && self.ex_history.last() != Some(&line) {
                        self.ex_history.push(line.clone());
                        if self.ex_history.len() > 200 {
                            self.ex_history.remove(0);
                        }
                    }
                    if matches!(
                        crate::input::ex::parse(&line),
                        Ok(crate::input::ex::ExCommand::Messages)
                    ) {
                        self.messages_view = Some(0);
                        return false;
                    }
                    if let Ok(crate::input::ex::ExCommand::Connect(uri)) = crate::input::ex::parse(&line) {
                        if self.uri == uri {
                            self.request_refresh();
                            return false;
                        }
                        if self
                            .detail
                            .as_ref()
                            .is_some_and(|detail| detail.hw.pending_count() > 0)
                        {
                            self.show_message(
                                "⚠ Unsaved hardware changes: use :w or :q! before connecting".into(),
                            );
                            return false;
                        }
                        if self.view == View::Detail {
                            self.view = View::Dashboard;
                        }
                        self.detail = None;
                        if let Some(draft) = self.nets.new_form.take() {
                            self.network_retries.push((self.uri.clone(), draft));
                        }
                        self.uri = uri;
                        self.dash.uri.clone_from(&self.uri);
                        self.dash.connected = false;
                        self.dash.rows.clear();
                        self.dash.marks.clear();
                        self.dash.configs.clear();
                        self.dash.ips.clear();
                        self.dash.selection = 0;
                        self.dash.shutdowns.clear();
                        self.dash.visual_anchor = None;
                        self.dash.recent_events.clear();
                        // Nothing of the previous host may survive the switch:
                        // gauges, title, pools, metrics, the Events top bar.
                        self.dash.host_gauges.clear();
                        self.dash.host_pools.clear();
                        self.dash.host_title.clear();
                        self.dash.metrics = Default::default();
                        self.dash.node = None;
                        self.dash.hostname = connection_hostname(&self.uri);
                        if !self.demo {
                            crate::ui::chrome::set_hostname(&self.dash.hostname);
                        }
                        self.events_state.events.clear();
                        self.events_state.selected = 0;
                        self.events_state.uri.clone_from(&self.uri);
                        self.events_state.connected = true;
                        self.host = ui::views::host::HostState {
                            uri: self.uri.clone(),
                            connected: true,
                            ..Default::default()
                        };
                        self.nets.uri.clone_from(&self.uri);
                        self.store.uri.clone_from(&self.uri);
                        self.nets.networks.clear();
                        self.nets.leases.clear();
                        self.nets.attached.clear();
                        self.nets.selected = 0;
                        self.store.pools.clear();
                        self.store.volumes.clear();
                        self.store.isos.clear();
                        self.store.pool_selected = 0;
                        self.store.vol_selected = 0;
                        self.dash.message = "loading…".into();
                        self.sync_poller();
                        if !self.demo {
                            self.request_view_load(self.view);
                        }
                        return false;
                    }
                    if self.view == View::Detail
                        && !self.demo
                        && !self.dry_run
                        && matches!(
                            crate::input::ex::parse(&line),
                            Ok(crate::input::ex::ExCommand::Write)
                        )
                    {
                        self.submit_hardware();
                        return false;
                    }
                    if !self.demo
                        && !self.dry_run
                        && let Ok(crate::input::ex::ExCommand::AttachHostdev(domain, node)) =
                            crate::input::ex::parse(&line)
                    {
                        self.submit_hostdev(domain, node);
                        return false;
                    }
                    if !self.demo
                        && self.dry_run
                        && let Ok(crate::input::ex::ExCommand::Media(domain, target, iso)) =
                            crate::input::ex::parse(&line)
                        && target.is_empty()
                    {
                        self.request_media_preview(domain, iso);
                        return false;
                    }
                    if !self.demo
                        && !self.dry_run
                        && let Ok(crate::input::ex::ExCommand::Shutdown(domain)) =
                            crate::input::ex::parse(&line)
                    {
                        match self.dash.rows.iter().find(|row| row.name == domain) {
                            Some(row) if row.state == crate::model::DomainState::Running => {
                                let plan = crate::command::builders::lifecycle::shutdown(&domain);
                                self.submit_plans(vec![(domain, plan)]);
                            }
                            Some(_) => self.show_message(format!("✗ {domain} is not running")),
                            None => self.show_message(format!("✗ no domain named {domain}")),
                        }
                        return false;
                    }
                    if !self.demo
                        && !self.dry_run
                        && let Ok(crate::input::ex::ExCommand::Desc(domain, description)) =
                            crate::input::ex::parse(&line)
                    {
                        self.submit_description(domain, description);
                        return false;
                    }
                    if !self.demo
                        && !self.dry_run
                        && let Ok(crate::input::ex::ExCommand::Media(domain, target, iso)) =
                            crate::input::ex::parse(&line)
                        && target.is_empty()
                    {
                        let active = self.domain_is_active(&domain);
                        self.submit_media_change(domain, iso, active);
                        return false;
                    }
                    if !self.demo
                        && !self.dry_run
                        && let Ok(crate::input::ex::ExCommand::Clone(domain, name)) =
                            crate::input::ex::parse(&line)
                    {
                        match clone_problem(&self.dash, &domain, &name) {
                            Some(problem) => self.show_message(problem),
                            None => {
                                let plan =
                                    crate::command::builders::lifecycle::clone_on(&self.uri, &domain, &name);
                                self.submit_plans(vec![(domain, plan)]);
                            }
                        }
                        return false;
                    }
                    if !self.demo
                        && !self.dry_run
                        && let Some(plans) = ex_job_plans(&line, |d| self.domain_is_active(d))
                    {
                        match plans {
                            Ok(plans) if is_block_resize(&line) => {
                                self.confirm_plans("⚠ Resize block device", "resize", plans)
                            }
                            Ok(plans) => {
                                self.submit_plans(plans);
                            }
                            Err(message) => self.show_message(message),
                        }
                        return false;
                    }
                    if !self.demo
                        && !self.dry_run
                        && let Some(raw) = raw_job_step(&line)
                    {
                        match raw {
                            Ok(step) => {
                                self.submit_raw(step);
                            }
                            Err(message) => self.show_message(message),
                        }
                        return false;
                    }
                    let interval_before = self.config.general.refresh_interval_secs;
                    let quit = if self.view == View::Detail {
                        run_detail_ex(
                            &line,
                            &mut self.detail,
                            &mut self.view,
                            &mut self.uri,
                            &mut self.dash,
                            &mut self.history,
                            &mut self.messages,
                            self.dry_run,
                            self.demo,
                            &mut self.config,
                            &mut self.theme,
                            &mut self.confirm,
                        )
                    } else {
                        run_ex_line(
                            &line,
                            &mut self.uri,
                            &mut self.dash,
                            &mut self.history,
                            &mut self.messages,
                            self.dry_run,
                            self.demo,
                            &mut self.help_filter,
                            &mut self.config,
                            &mut self.theme,
                            &mut self.confirm,
                        )
                    };
                    if quit {
                        return true;
                    }
                    if self.config.general.refresh_interval_secs != interval_before {
                        // Restart the poller with the new interval.
                        self.poller_uri.clear();
                    }
                    // Results land in dash.message: show them in the current view too.
                    if self.view != View::Detail
                        || !matches!(
                            crate::input::ex::parse(&line),
                            Ok(crate::input::ex::ExCommand::Write
                                | crate::input::ex::ExCommand::Discard
                                | crate::input::ex::ExCommand::Quit)
                        )
                    {
                        let msg = self.dash.message.clone();
                        self.show_message(msg);
                    }
                }
                KeyCode::Backspace => ex.backspace(),
                KeyCode::Delete => ex.delete(),
                KeyCode::Home => ex.home(),
                KeyCode::End => ex.end(),
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => ex.home(),
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => ex.end(),
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => ex.kill_to_start(),
                KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => ex.kill_word(),
                KeyCode::Left => ex.move_left(),
                KeyCode::Right => ex.move_right(),
                KeyCode::Char(c) if typing => {
                    ex.insert(c);
                    self.ex_history_pos = None;
                }
                // History recall (newest first), like vim's command line.
                KeyCode::Up | KeyCode::Down => {
                    let lines = &self.ex_history;
                    if !lines.is_empty() {
                        let next = match (self.ex_history_pos, key.code) {
                            (None, KeyCode::Up) => Some(lines.len() - 1),
                            (Some(p), KeyCode::Up) => Some(p.saturating_sub(1)),
                            (Some(p), KeyCode::Down) if p + 1 < lines.len() => Some(p + 1),
                            _ => None,
                        };
                        self.ex_history_pos = next;
                        let text = next.map_or_else(String::new, |p| lines[p].clone());
                        if let Some(ex) = self.ex_line.as_mut() {
                            ex.set(&text);
                        }
                    }
                }
                _ => {}
            }
            return false;
        }
        // Wizard captures all keys while open.
        if self.wizard.as_ref().is_some_and(|w| !w.command_review)
            && !self.demo
            && !self.dry_run
            && key.code == KeyCode::Char('e')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.request_wizard_editor();
            return false;
        }
        if !self.demo
            && !self.dry_run
            && key.code == KeyCode::Enter
            && key.modifiers.is_empty()
            && let Some(wizard) = self.wizard.as_ref()
            && !wizard.command_review
            && wizard.step >= 5
            && wizard.problems().is_empty()
        {
            self.submit_wizard(wizard.clone());
            return false;
        }
        if self.wizard.is_some()
            && handle_wizard_key(
                key,
                &mut self.wizard,
                &self.uri,
                &self.config.wizard.iso_dirs,
                self.dry_run,
                self.demo,
                &mut self.history,
                &mut self.messages,
                &mut self.dash,
            )
        {
            return false;
        }
        // Palette and help overlays capture keys while open.
        if key.code == KeyCode::Enter
            && key.modifiers.is_empty()
            && let Some(command) = self
                .palette
                .as_ref()
                .and_then(|palette| palette.selected_item())
                .and_then(|item| match item.virsh_cmd.as_str() {
                    "snapshot-create-as" => Some(ui::views::detail::SnapModal::Create),
                    "snapshot-revert" => Some(ui::views::detail::SnapModal::Revert),
                    "snapshot-delete" => Some(ui::views::detail::SnapModal::Delete),
                    "snapshot-list" => Some(ui::views::detail::SnapModal::None),
                    _ => None,
                })
        {
            self.palette = None;
            self.request_palette_snapshot(command);
            return false;
        }
        // Modal overlays own the keyboard: an unhandled key is dropped, never
        // passed to the view underneath (only C-c still quits).
        let ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        if let Some(scroll) = self.messages_view.as_mut() {
            let max = self.messages.len().saturating_sub(1);
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => self.messages_view = None,
                KeyCode::Char('k') | KeyCode::Up if !ctrl => *scroll = (*scroll + 1).min(max),
                KeyCode::Char('j') | KeyCode::Down if !ctrl => *scroll = scroll.saturating_sub(1),
                KeyCode::Char('u') if ctrl => *scroll = (*scroll + 10).min(max),
                KeyCode::Char('d') if ctrl => *scroll = scroll.saturating_sub(10),
                KeyCode::Char('g') => *scroll = max,
                KeyCode::Char('G') => *scroll = 0,
                _ => {}
            }
            if !ctrl_c {
                return false;
            }
        }
        if self.palette.is_some() {
            if handle_palette_key(key, &mut self.palette, &mut self.ex_line) && !self.demo {
                self.schedule_palette_doc();
            }
            if !ctrl_c {
                return false;
            }
        }
        if self.help_filter.is_some() {
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => {
                    self.help_page = 1 - self.help_page.min(1);
                    self.help_scroll = 0;
                }
                KeyCode::Down | KeyCode::PageDown => self.help_scroll = self.help_scroll.saturating_add(1),
                KeyCode::Up | KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(1),
                _ => {
                    handle_help_key(key, &mut self.help_filter);
                }
            }
            if self.help_filter.is_none() {
                self.help_page = 0;
                self.help_scroll = 0;
            }
            if !ctrl_c {
                return false;
            }
        }
        // Settings captures navigation while open.
        if self.settings.is_some() {
            let before = self.config.clone();
            if handle_settings_key(key, &mut self.settings, &mut self.config, &mut self.theme)
                && (self.config.general.refresh_interval_secs != before.general.refresh_interval_secs
                    || self.config.monitoring != before.monitoring)
            {
                // Restart the poller with the new interval / memory-stats option.
                self.poller_uri.clear();
            }
            if !ctrl_c {
                return false;
            }
        }
        // An unfinished key sequence takes precedence over view-specific raw keys.
        let sequence_in_progress = self.engine.pending_prefix().is_some() || self.engine.has_pending_digits();
        // Storage raw keys (picker, forms, pool/vol actions) bypass the engine.
        if self.store.picker.is_some() && !self.demo && !self.dry_run && self.submit_picker(key) {
            return false;
        }
        if self.view == View::Storage
            && self.store.picker.is_none()
            && !sequence_in_progress
            && key.code == KeyCode::Enter
            && key.modifiers.is_empty()
            && self.request_storage_picker()
        {
            return false;
        }
        if self.view == View::Storage
            && self.store.picker.is_none()
            && !sequence_in_progress
            && !self.demo
            && !self.dry_run
            && key.modifiers.is_empty()
            && let KeyCode::Char(ch) = key.code
            && let Some(plans) = self.storage_pool_plans(ch)
        {
            if !plans.is_empty() {
                self.submit_plans(plans);
            }
            return false;
        }
        if ((self.view == View::Storage && !sequence_in_progress) || self.store.picker.is_some())
            && handle_storage_key(
                key,
                &mut self.store,
                self.dry_run,
                self.demo,
                &mut self.history,
                &mut self.messages,
                &mut self.confirm,
                &mut self.ex_line,
            )
        {
            return false;
        }
        // Snapshot modals capture keys (y/n/Esc + toggles).
        if self.view == View::Detail && !self.demo && !self.dry_run && self.submit_snapshot(key) {
            return false;
        }
        if self.view == View::Detail
            && self.detail.as_ref().is_some_and(|d| {
                d.tab == crate::ui::views::detail::DetailTab::Snapshots
                    && d.snap_modal != crate::ui::views::detail::SnapModal::None
            })
            && handle_snap_modal(
                key,
                &mut self.detail,
                self.dry_run,
                self.demo,
                &mut self.history,
                &mut self.messages,
            )
        {
            return false;
        }
        // Hardware tab keys bypass the engine (INSERT raw editing, J/K, Tab…),
        // except that in NORMAL mode an unfinished sequence (`␣m…`) keeps its
        // final key: `i` of `␣mi` is not the local INSERT shortcut.
        let hw_insert = self.detail.as_ref().is_some_and(|d| d.hw_insert);
        if self.view == View::Detail
            && (hw_insert || !sequence_in_progress)
            && handle_hardware_key(
                key,
                &mut self.detail,
                &mut self.confirm,
                &mut self.ex_line,
                &mut self.engine,
            )
        {
            return false;
        }
        if key.code == KeyCode::Esc && self.dash.visual_anchor.is_some() {
            self.dash.visual_anchor = None;
            self.dash.message = format!("{} marked", self.dash.marks.len());
            return false;
        }
        let in_detail = self.view == View::Detail;
        let text_input = (self.view == View::Networks && self.nets.new_form.is_some())
            || self.store.picker.is_some()
            || self.detail.as_ref().is_some_and(|d| d.hw_insert);
        if !text_input && handle_key(key, in_detail) {
            return true;
        }
        // `ZZ` quits. The first `Z` never reaches the engine: it waits for a
        // second `Z` and only resolves as a lone `Z` (managed-save) after
        // [`Z_TIMEOUT`]; any other key cancels it (vim behaviour).
        let is_z = key.code == KeyCode::Char('Z')
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if !text_input && is_z && !self.replaying_z && self.engine.pending_prefix().is_none() {
            if std::mem::take(&mut self.pending_quit_z) {
                return true;
            }
            self.pending_quit_z = true;
            return false;
        }
        if std::mem::take(&mut self.pending_quit_z) {
            // `Z` followed by another key: drop both, like vim.
            return false;
        }
        // Normal dispatch through the key engine.
        let seq = crate::input::key::KeySeq::from_event(&key);
        if seq.token() == "Ignored" {
            return false;
        }
        // Networks raw keys (new form, e edit, form editing) bypass the engine.
        if self.view == View::Networks
            && self.nets.new_form.is_none()
            && !sequence_in_progress
            && key.code == KeyCode::Char('n')
            && key.modifiers.is_empty()
            && let Some(index) = self.network_retries.iter().position(|(uri, _)| uri == &self.uri)
        {
            let (_, draft) = self.network_retries.remove(index);
            self.nets.new_form = Some(draft);
            return false;
        }
        if self.view == View::Networks
            && self.nets.new_form.is_some()
            && !self.demo
            && !self.dry_run
            && key.code == KeyCode::Enter
            && key.modifiers.is_empty()
        {
            match prepare_network_create(&self.nets) {
                Ok((name, xml)) => {
                    self.submit_network_create(name, xml);
                }
                Err(message) => self.nets.message = message,
            }
            return false;
        }
        if self.view == View::Networks
            && self.nets.new_form.is_none()
            && !sequence_in_progress
            && !self.demo
            && !self.dry_run
            && key.code == KeyCode::Char('e')
            && key.modifiers.is_empty()
        {
            self.request_network_editor();
            return false;
        }
        if self.view == View::Networks
            && (self.nets.new_form.is_some() || !sequence_in_progress)
            && handle_networks_key(
                key,
                &mut self.nets,
                self.dry_run,
                self.demo,
                &mut self.history,
                &mut self.messages,
            )
        {
            return false;
        }
        if let Some(id) = self.active_job.as_ref().map(|job| job.id) {
            use crate::input::engine::KeyAction as A;
            let mut probe = self.engine.clone();
            let blocked = probe.feed(&seq).into_iter().find(|resolved| {
                !matches!(
                    resolved.action,
                    A::MoveUp
                        | A::MoveDown
                        | A::GoTop
                        | A::GoBottom
                        | A::HalfDown
                        | A::HalfUp
                        | A::Open
                        | A::Back
                        | A::Filter
                        | A::NextMatch
                        | A::PrevMatch
                        | A::NewDomain
                        | A::Ex
                        | A::Palette
                        | A::Help
                        | A::Settings
                        | A::Refresh
                        | A::Redraw
                        | A::PrevTab
                        | A::NextTab
                        | A::GotoXml
                        | A::ViewDomains
                        | A::ViewHost
                        | A::ViewNetworks
                        | A::ViewStorage
                        | A::ViewEvents
                        | A::Noop
                )
            });
            if let Some(blocked) = blocked {
                let escape = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
                self.engine.feed(&crate::input::key::KeySeq::from_event(&escape));
                let message = if matches!(blocked.action, A::Quit | A::QuitAll) {
                    format!("⚠ Job #{id} is running; press C-c to cancel before quitting")
                } else {
                    format!("⚠ Job #{id} is running; wait or press C-c to cancel")
                };
                self.show_message(message);
                self.busy_feedback = Some(std::time::Instant::now());
                return false;
            }
        }
        // ":" opens the ex line directly (also in the map).
        for resolved in self.engine.feed(&seq) {
            if is_view_switch(resolved.action) && self.blocks_leaving_detail() {
                continue;
            }
            if switch_view(resolved.action, &mut self.view) {
                continue;
            }
            use crate::input::engine::KeyAction as A;
            if resolved.action == A::Refresh {
                self.request_refresh();
                continue;
            }
            if self.detail_loading
                && self.view == View::Detail
                && !matches!(
                    resolved.action,
                    A::MoveUp
                        | A::MoveDown
                        | A::GoTop
                        | A::GoBottom
                        | A::HalfDown
                        | A::HalfUp
                        | A::Back
                        | A::PrevTab
                        | A::NextTab
                        | A::GotoXml
                        | A::Help
                        | A::Ex
                        | A::Redraw
                )
            {
                self.show_message("Loading domain details…".into());
                continue;
            }
            // Host table: move / open; Events: navigation, filter chips and search.
            if self.view == View::Host {
                match resolved.action {
                    A::MoveDown | A::MoveUp => {
                        let d = if resolved.action == A::MoveDown { 1 } else { -1 };
                        for _ in 0..resolved.count {
                            self.host.move_selection(d);
                        }
                        continue;
                    }
                    A::GoTop => {
                        self.host.selected = 0;
                        continue;
                    }
                    A::GoBottom => {
                        self.host.selected = self.host.domains.len().saturating_sub(1);
                        continue;
                    }
                    A::Open => {
                        if let Some(name) = self.host.selected_name().map(str::to_string) {
                            self.dash.select_name(&name);
                            self.view = View::Dashboard;
                            self.request_detail();
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            if self.view == View::Events {
                let ev = &mut self.events_state;
                let n = ev.visible().len();
                let handled = match resolved.action {
                    A::MoveDown => {
                        ev.selected = (ev.selected + resolved.count as usize).min(n.saturating_sub(1));
                        true
                    }
                    A::MoveUp => {
                        ev.selected = ev.selected.saturating_sub(resolved.count as usize);
                        true
                    }
                    A::GoTop => {
                        ev.selected = 0;
                        true
                    }
                    A::GoBottom => {
                        ev.selected = n.saturating_sub(1);
                        true
                    }
                    A::HalfDown => {
                        ev.selected = (ev.selected + 10).min(n.saturating_sub(1));
                        true
                    }
                    A::HalfUp => {
                        ev.selected = ev.selected.saturating_sub(10);
                        true
                    }
                    A::ChipCycle | A::ChipCycleBack => {
                        let all = ui::views::events::EventFilter::ALL;
                        let i = all.iter().position(|f| *f == ev.filter).unwrap_or(0);
                        let len = all.len();
                        let next = if resolved.action == A::ChipCycle {
                            (i + 1) % len
                        } else {
                            (i + len - 1) % len
                        };
                        ev.filter = all[next];
                        ev.selected = 0;
                        true
                    }
                    A::Filter => {
                        self.ex_line = Some(crate::input::textinput::TextInput::filter(&ev.query));
                        true
                    }
                    A::Open => {
                        let obj = ev.visible().get(ev.selected).map(|e| e.object.clone());
                        if let Some(obj) = obj {
                            self.dash.select_name(&obj);
                            if self.dash.selected().name == obj {
                                self.view = View::Dashboard;
                                self.request_detail();
                            }
                        }
                        true
                    }
                    _ => false,
                };
                if handled {
                    continue;
                }
            }
            if matches!(self.view, View::Host | View::Events | View::Gallery)
                && !matches!(
                    resolved.action,
                    A::Ex | A::Palette | A::Help | A::Settings | A::NewDomain | A::InsertMedia
                )
            {
                continue;
            }
            if self.view == View::Networks && !self.demo && matches!(resolved.action, A::MoveDown | A::MoveUp)
            {
                let before = self.nets.selected;
                self.nets
                    .move_selection(if resolved.action == A::MoveDown { 1 } else { -1 });
                if self.nets.focus == ui::views::networks::NetFocus::Networks && self.nets.selected != before
                {
                    self.request_selection();
                }
                continue;
            }
            if self.view == View::Storage
                && !self.demo
                && matches!(resolved.action, A::MoveDown | A::MoveUp)
                && self.store.focus_pools
            {
                let before = self.store.pool_selected;
                self.store.pool_selected = if resolved.action == A::MoveDown {
                    (before + 1).min(self.store.pools.len().saturating_sub(1))
                } else {
                    before.saturating_sub(1)
                };
                if self.store.pool_selected != before {
                    // Volumes are cached per view load: no read, no reload restart.
                    self.store.show_pool_volumes(None);
                }
                continue;
            }
            if self.view == View::Networks
                && !self.demo
                && !self.dry_run
                && let Some(plans) = self.network_plans(resolved.action)
            {
                if !plans.is_empty() {
                    self.submit_plans(plans);
                }
                continue;
            }
            if self.view == View::Networks
                && !matches!(
                    resolved.action,
                    A::Ex
                        | A::Redraw
                        | A::Palette
                        | A::Help
                        | A::Settings
                        | A::NewDomain
                        | A::InsertMedia
                        | A::EjectMedia
                )
            {
                if dispatch_networks(
                    resolved.action,
                    &mut self.nets,
                    &mut self.detail,
                    &mut self.view,
                    self.dry_run,
                    self.demo,
                    &mut self.history,
                    &mut self.messages,
                    &mut self.confirm,
                ) {
                    return true;
                }
                // ⏎ on a lease: open the owning domain's detail view.
                if let Some(dom) = self.nets.pending_open.take() {
                    self.dash.select_name(&dom);
                    if self.dash.selected().name == dom {
                        self.view = View::Dashboard;
                        self.request_detail();
                    }
                }
                continue;
            }
            match (self.view, resolved.action) {
                (_, A::Ex) => {
                    self.ex_line = Some(crate::input::textinput::TextInput::new());
                    continue;
                }
                (View::Detail, A::EditXml)
                    if !self.demo
                        && !self.dry_run
                        && self
                            .detail
                            .as_ref()
                            .is_some_and(|detail| detail.tab == ui::views::detail::DetailTab::Xml) =>
                {
                    self.begin_domain_editor();
                    continue;
                }
                (View::Dashboard, A::ExportXml) if !self.demo && !self.dry_run => {
                    self.submit_xml_export();
                    continue;
                }
                (View::Dashboard | View::Detail, A::Viewer) => {
                    let domain = if self.view == View::Detail {
                        self.detail
                            .as_ref()
                            .map(|detail| detail.domain.clone())
                            .unwrap_or_default()
                    } else {
                        self.dash.selected().name.clone()
                    };
                    if (self.demo || self.dry_run) && !domain.is_empty() {
                        self.show_message(format!("[dry-run] Open viewer for {domain}"));
                    } else if !domain.is_empty() {
                        self.request_viewer(self.uri.clone(), domain);
                    }
                    continue;
                }
                (View::Dashboard | View::Detail, A::Console) if self.demo || self.dry_run => {
                    self.show_message("[dry-run] Open console".into());
                    continue;
                }
                (
                    View::Dashboard,
                    action @ (A::Start | A::Shutdown | A::Reboot | A::Pause | A::Autostart | A::SendKey),
                ) if !self.demo && !self.dry_run => {
                    if let Some(plans) = self.lifecycle_plans(action)
                        && !plans.is_empty()
                    {
                        if plans.len() > 1 {
                            // Bulk actions always show what will run first.
                            let verb = action.name().replace('_', " ");
                            self.confirm_plans(
                                &format!("{} {} domains", capitalize(&verb), plans.len()),
                                &verb,
                                plans,
                            );
                        } else {
                            self.submit_plans(plans);
                        }
                    }
                    continue;
                }
                (_, A::InsertMedia) => {
                    // The picker lives in Storage: leaving Detail must not drop edits.
                    if self.view == View::Detail && self.blocks_leaving_detail() {
                        continue;
                    }
                    self.request_picker();
                    continue;
                }
                (_, A::NewDomain) => {
                    let remote = !self.demo && !is_local_uri(&self.uri);
                    // Remote capacity comes from the poller's node sample
                    // (`nodeinfo` + `nodememstats`), refreshed every tick.
                    let node = self
                        .dash
                        .node
                        .clone()
                        .filter(|n| n.threads > 0 && n.mem_total_kib > 0);
                    if remote && node.is_none() {
                        self.show_message(String::from(
                            "reading the remote host's capacity… press ␣n again in a moment",
                        ));
                        continue;
                    }
                    let mut facts = wizard_host_facts(
                        &self.dash,
                        &self.nets,
                        &self.store,
                        &self.uri,
                        &self.config.wizard.iso_dirs,
                        self.demo,
                    );
                    if let (true, Some(node)) = (remote, node) {
                        facts.threads = node.threads;
                        facts.free_gib = node.mem_available_kib as f64 / 1024.0 / 1024.0;
                        // /dev/kvm describes this machine, not the remote host.
                        facts.no_kvm = false;
                    }
                    open_wizard(&mut self.wizard, facts);
                    continue;
                }
                (_, A::DiskAttach | A::DiskResize | A::NicAttach | A::NicLink | A::Migrate) => {
                    let (domain, running) = match &self.detail {
                        Some(d) if self.view == View::Detail => {
                            (d.domain.clone(), d.state == crate::model::DomainState::Running)
                        }
                        _ => {
                            let r = self.dash.selected();
                            (r.name.clone(), r.state == crate::model::DomainState::Running)
                        }
                    };
                    let cfg = match &self.detail {
                        Some(d) if self.view == View::Detail => Some(d.config.clone()),
                        _ => self.dash.configs.get(&domain).cloned(),
                    };
                    match prefill_command(resolved.action, &domain, cfg.as_ref(), running) {
                        Some(cmd) => self.ex_line = Some(crate::input::textinput::TextInput::with(&cmd)),
                        None => self.show_message(format!("✗ {domain} has no device for that action")),
                    }
                    continue;
                }
                (_, A::BootOrder) => {
                    self.request_detail_tab(crate::ui::views::detail::DetailTab::Hardware, Some("boot"));
                    continue;
                }
                (View::Dashboard, A::PrevTab) => {
                    self.dash.detail_tab = (self.dash.detail_tab + 4) % 5;
                    continue;
                }
                (View::Dashboard, A::NextTab) => {
                    self.dash.detail_tab = (self.dash.detail_tab + 1) % 5;
                    continue;
                }
                (_, A::Palette) => {
                    if self.demo {
                        self.palette = Some(ui::overlays::palette::PaletteState::demo());
                    } else {
                        self.open_async_palette();
                    }
                    continue;
                }
                (_, A::Help) => {
                    self.help_filter = Some(String::new());
                    continue;
                }
                (_, A::Settings) => {
                    self.settings = Some(ui::views::settings::SettingsState::open(&self.config));
                    continue;
                }
                (_, A::EjectMedia) => {
                    if !self.demo && !self.dry_run {
                        self.submit_media_eject();
                        continue;
                    }
                    if !self.demo && self.dry_run {
                        let domain = if self.view == View::Detail {
                            self.detail
                                .as_ref()
                                .map(|detail| detail.domain.clone())
                                .unwrap_or_default()
                        } else {
                            self.dash.selected().name.clone()
                        };
                        if !domain.is_empty() {
                            self.request_media_preview(domain, None);
                        }
                        continue;
                    }
                    if self.demo && self.store.picker.is_none() {
                        self.request_picker();
                    }
                    eject_media(
                        &mut self.store,
                        self.dry_run,
                        self.demo,
                        &mut self.history,
                        &mut self.messages,
                    );
                    continue;
                }
                (View::Storage, A::Open) if self.request_storage_picker() => {
                    continue;
                }
                (View::Storage, _) => {
                    if dispatch_storage(resolved.action, &mut self.store) {
                        return true;
                    }
                    continue;
                }
                (View::Dashboard | View::Host, A::Open) => {
                    self.request_detail();
                    continue;
                }
                (View::Dashboard | View::Host, A::SnapshotNew | A::SnapshotRevert | A::SnapshotDelete) => {
                    let modal = match resolved.action {
                        A::SnapshotNew => crate::ui::views::detail::SnapModal::Create,
                        A::SnapshotRevert => crate::ui::views::detail::SnapModal::Revert,
                        _ => crate::ui::views::detail::SnapModal::Delete,
                    };
                    self.request_snapshot_detail(modal);
                    continue;
                }
                (_, A::Back) if self.view == View::Detail => {
                    if self.blocks_leaving_detail() {
                        continue;
                    }
                    // The domain may have been edited: re-read its config.
                    if let Some(d) = &self.detail {
                        self.dash.configs.remove(&d.domain);
                        if let Some(p) = &self.poller {
                            let _ = p.tx.send(poller::Request::RefetchConfig(d.domain.clone()));
                        }
                    }
                    self.sent_selection = None;
                    self.detail = None;
                    self.view = View::Dashboard;
                    continue;
                }
                (_, A::EditXml) if self.view != View::Detail => {
                    self.request_detail_tab(crate::ui::views::detail::DetailTab::Xml, None);
                    continue;
                }
                _ => {}
            }
            if self.view == View::Detail {
                if dispatch_detail(
                    resolved.action,
                    &mut self.detail,
                    &mut self.view,
                    &self.uri,
                    self.dry_run,
                    self.demo,
                ) {
                    return true;
                }
                continue;
            }
            // Motions move by `delta × count` in one step (no per-row loop).
            let motion: Option<i32> = match resolved.action {
                A::MoveDown => Some(1),
                A::MoveUp => Some(-1),
                A::HalfDown => Some(10),
                A::HalfUp => Some(-10),
                _ => None,
            };
            if let Some(delta) = motion {
                self.dash
                    .move_selection(delta.saturating_mul(resolved.count as i32));
                continue;
            }
            for _ in 0..resolved.count {
                if dispatch_action(
                    resolved.action,
                    &mut self.dash,
                    &mut self.uri,
                    &mut self.history,
                    &mut self.messages,
                    &mut self.confirm,
                    &mut self.ex_line,
                    self.dry_run,
                    self.demo,
                ) {
                    // Quit requested (handled above for q/ZZ; :qa returns true here).
                    return true;
                }
            }
            if self.demo {}
        }
        // The which-key popup counts from the key that opened the prefix
        // (a lone `Space` must show it too, not only `Space s`).
        if self.engine.pending_prefix().is_none() {
            self.which_key_since = None;
        } else if self.which_key_since.is_none() {
            self.which_key_since = Some(std::time::Instant::now());
        }
        false
    }
}

/// App view (tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum View {
    Dashboard,
    Host,
    Networks,
    Storage,
    Events,
    Gallery,
    Detail,
}

/// `:block-resize …` (shrinking destroys data: always confirmed).
fn is_block_resize(line: &str) -> bool {
    matches!(
        crate::input::ex::parse(line),
        Ok(crate::input::ex::ExCommand::Resize(..))
    )
}

/// Hostname to show for `uri` until libvirt reports one: this machine for
/// local URIs, the URI's host part for remote ones.
fn connection_hostname(uri: &str) -> String {
    if is_local_uri(uri) {
        crate::backend::virsh::hostname()
    } else {
        remote_host_label(uri)
    }
}

/// Host part of a remote URI (`qemu+ssh://user@srv/system` → `srv`).
fn remote_host_label(uri: &str) -> String {
    uri.split_once("://")
        .map(|(_, rest)| rest.split('/').next().unwrap_or(""))
        .map(|authority| authority.rsplit('@').next().unwrap_or(authority))
        .map(|host| host.split(':').next().unwrap_or(host).to_string())
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| uri.to_string())
}

/// True for the `1`-`5` view switches.
fn is_view_switch(action: crate::input::engine::KeyAction) -> bool {
    use crate::input::engine::KeyAction as A;
    matches!(
        action,
        A::ViewDomains | A::ViewHost | A::ViewNetworks | A::ViewStorage | A::ViewEvents
    )
}

/// Switch views for View* actions. Returns true when handled.
fn switch_view(action: crate::input::engine::KeyAction, view: &mut View) -> bool {
    use crate::input::engine::KeyAction as A;
    match action {
        A::ViewDomains => *view = View::Dashboard,
        A::ViewHost => *view = View::Host,
        A::ViewNetworks => *view = View::Networks,
        A::ViewStorage => *view = View::Storage,
        A::ViewEvents => *view = View::Events,
        _ => return false,
    }
    true
}

use poller::{LiveMsg, is_local_uri};

/// How long a lone `Z` waits for the second `Z` of `ZZ`.
const Z_TIMEOUT: Duration = Duration::from_millis(600);
/// Idle time after a prefix key (`Space`, `g`, …) before the which-key popup.
const WHICH_KEY_DELAY: Duration = Duration::from_millis(300);

fn handle_key(key: KeyEvent, in_detail: bool) -> bool {
    // q quits, C-c quits (ZZ is handled in `handle_key_event`).
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }
    matches!(key.code, KeyCode::Char('q') if key.modifiers.is_empty()) && !in_detail
}

use external::{detached_run, suspend_run};

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    #[test]
    fn which_key_timer_starts_with_the_first_prefix_key() {
        let mut app = fixture_app();
        app.handle_key_event(key(KeyCode::Char(' ')));
        assert_eq!(app.engine.pending_prefix().as_deref(), Some("Space"));
        assert!(
            app.which_key_since.is_some(),
            "a lone Space must start the which-key timer"
        );
        app.handle_key_event(key(KeyCode::Esc));
        assert!(app.which_key_since.is_none());
    }

    #[test]
    fn which_key_popup_hides_what_is_behind_it() {
        let app = fixture_app();
        let map = crate::input::keymap::default_map();
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(60, 30)).unwrap();
        let frame = terminal
            .draw(|f| {
                let area = f.area();
                f.render_widget(
                    ratatui::widgets::Paragraph::new("X".repeat(60 * 30))
                        .wrap(ratatui::widgets::Wrap { trim: false }),
                    area,
                );
                render_which_key(f, &app.theme, "Space", &map);
            })
            .unwrap();
        let buffer = frame.buffer;
        // The popup sits bottom-right; its rows end in blank padding before the border.
        let leaks: Vec<String> = (0..30)
            .filter_map(|y| {
                let row: String = (0..60).map(|x| buffer[(x, y)].symbol().to_string()).collect();
                let inner = row
                    .rsplit_once('│')
                    .and_then(|(left, _)| left.rsplit_once('│'))
                    .map(|(_, inner)| inner.to_string())?;
                inner.contains('X').then_some(row)
            })
            .collect();
        assert!(leaks.is_empty(), "background shows through the popup: {leaks:#?}");
    }

    #[test]
    fn q_quits() {
        assert!(handle_key(key(KeyCode::Char('q')), false));
        assert!(!handle_key(key(KeyCode::Char('q')), true));
    }

    pub(super) fn fixture_app() -> App {
        let config = Config::default();
        let (loads_tx, _) = tokio::sync::mpsc::unbounded_channel();
        App {
            theme: current_theme(&config),
            dash: ui::views::dashboard::DashboardState::demo(),
            host: ui::views::host::HostState::demo(),
            nets: ui::views::networks::NetworksState::demo(),
            store: ui::views::storage::StorageState::demo(),
            events_state: ui::views::events::EventsState::demo(),
            wizard: None,
            settings: None,
            palette: None,
            help_filter: None,
            messages_view: None,
            job_finished_at: None,
            help_page: 0,
            help_scroll: 0,
            confirm_scroll: std::cell::Cell::new(0),
            viewport: std::cell::Cell::new(None),
            ex_history: Vec::new(),
            ex_history_pos: None,
            events_rx: None,
            view: View::Dashboard,
            pending_quit_z: false,
            replaying_z: false,
            engine: crate::input::engine::KeyEngine::new(),
            keymap_file: crate::input::keymap_file::KeymapFile::in_memory(),
            history: crate::command::history::History::new(200),
            uri: "test:///default".to_string(),
            dry_run: false,
            demo: false,
            confirm: None,
            ex_line: None,
            messages: MessageLog::default(),
            last_key: std::time::Instant::now(),
            which_key_since: None,
            detail: None,
            poller: None,
            poller_uri: String::new(),
            sent_selection: None,
            prev_view: View::Dashboard,
            entered: std::collections::HashSet::new(),
            loads_tx,
            config,
            completion: None,
            virsh_index: None,
            virsh_flags: std::collections::HashMap::new(),
            active_job: None,
            next_job_id: 1,
            busy_feedback: None,
            read_serial: 0,
            selection_task: std::collections::HashMap::new(),
            read_serials: std::collections::HashMap::new(),
            connection_epoch: 0,
            epoch_uri: "test:///default".into(),
            view_load_serial: std::collections::HashMap::new(),
            view_load_tasks: std::collections::HashMap::new(),
            view_load_pending: std::collections::HashSet::new(),
            view_load_refreshing: std::collections::HashSet::new(),
            help_task: None,
            help_docs: std::collections::HashMap::new(),
            input_revision: 0,
            wizard_revision: 0,
            viewer_task: None,
            detail_loading: false,
            picker_loading: false,
            pending_snapshot_modal: None,
            hardware_reload_required: None,
            editor_retries: Vec::new(),
            editor_generation: 0,
            network_retries: Vec::new(),
        }
    }

    #[tokio::test]
    async fn slow_command_does_not_hold_input() {
        let dir = tempfile::tempdir().unwrap();
        let started = dir.path().join("started");
        let release = dir.path().join("release");
        let started_arg = started.to_string_lossy().into_owned();
        let release_arg = release.to_string_lossy().into_owned();
        let mut app = fixture_app();
        let before = app.dash.selection;
        let builder = std::sync::Arc::new(move |_: &str| crate::command::plan::CommandPlan {
            steps: vec![crate::command::plan::CommandStep {
                program: "python3".into(),
                argv: vec![
                    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
                    "--started".into(),
                    started_arg.clone(),
                    "--release".into(),
                    release_arg.clone(),
                ],
                stdin: None,
            }],
            summary: "Finished test".into(),
        });
        app.confirm = Some((
            crate::ui::overlays::confirm::Confirm {
                kind: crate::ui::overlays::confirm::ConfirmKind::Always,
                title: "Test".into(),
                question: "Test".into(),
                commands: vec![],
                confirm_label: "y test".into(),
                type_name: None,
                typed: String::new(),
            },
            builder,
            vec!["test-target".into()],
        ));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let (observed_tx, observed_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut observed_tx = Some(observed_tx);
            while let Some(key) = rx.recv().await {
                app.on_key(key).await;
                if app.dash.selection != before
                    && let Some(sender) = observed_tx.take()
                {
                    let _ = sender.send(app.dash.selection);
                }
            }
        });
        tx.send(key(KeyCode::Char('y'))).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while !started.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        tx.send(key(KeyCode::Char('j'))).unwrap();
        let observed = tokio::time::timeout(Duration::from_secs(1), observed_rx).await;
        std::fs::write(&release, "release").unwrap();
        drop(tx);
        task.abort();
        assert!(observed.is_ok(), "input was held until the command exited");
    }

    #[tokio::test]
    async fn busy_job_rejects_second_submission() {
        let mut app = fixture_app();
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("second");
        let first = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "first");
        let second =
            crate::command::plan::CommandPlan::single("touch", vec![marker.to_str().unwrap()], "second");
        assert!(app.submit_plans(vec![("A".into(), first)]));
        assert!(!app.submit_plans(vec![("B".into(), second)]));
        assert!(!marker.exists());
        app.cancel_job();
    }

    #[tokio::test]
    async fn active_job_renders_progress_in_message_line() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest-a".into(), plan)]));
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("Job #1"));
        assert!(text.contains("guest-a"));
        assert!(text.contains("C-c cancel"));
        app.cancel_job();
    }

    #[tokio::test]
    async fn job_progress_uses_ascii_spinner_in_a_narrow_terminal() {
        let mut app = fixture_app();
        app.config.appearance.icons = "ascii".into();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("vm".into(), plan)]));
        let backend = ratatui::backend::TestBackend::new(50, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let row: String = (0..50).map(|x| buffer[(x, 11)].symbol()).collect();
        assert!(row.contains("Job #1"));
        assert!(
            row.chars()
                .next()
                .is_some_and(|ch| matches!(ch, '|' | '/' | '-' | '\\'))
        );
        app.cancel_job();
    }

    #[tokio::test]
    async fn job_message_states_snapshot() {
        use crate::command::job::{BatchOutcome, BatchResult, CommandEvent};

        fn row(app: &mut App, width: u16, active: bool) -> String {
            let backend = ratatui::backend::TestBackend::new(width, 30);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|frame| app.draw(frame)).unwrap();
            let buffer = terminal.backend().buffer();
            let text: String = (0..width).map(|x| buffer[(x, 29)].symbol()).collect();
            let text = text.trim_end();
            if active {
                let spinner = text.chars().next().unwrap();
                if app.config.appearance.icons == "ascii" {
                    assert!(matches!(spinner, '|' | '/' | '-' | '\\'));
                } else {
                    assert!("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏".contains(spinner));
                }
                let mut parts: Vec<_> = text.split(" · ").collect();
                if parts.len() >= 4 && parts[3].ends_with('s') {
                    parts[3] = "<elapsed>s";
                }
                let normalized = parts.join(" · ");
                format!(
                    "<spinner>{}",
                    &normalized[normalized.chars().next().unwrap().len_utf8()..]
                )
            } else {
                text.to_string()
            }
        }

        fn outcome(result: BatchResult, completed_targets: Vec<String>) -> BatchOutcome {
            BatchOutcome {
                failures: Vec::new(),
                started_steps: vec![],
                completed_steps: vec![],
                outputs: vec![],
                completed_targets,
                result,
            }
        }

        fn start(app: &mut App) {
            let plan =
                crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 0"], "Finished vt-test-vm");
            assert!(app.submit_plans(vec![("vt-test-vm".into(), plan)]));
            app.active_job.as_ref().unwrap().task.abort();
        }

        let mut app = fixture_app();
        start(&mut app);
        let preparing = row(&mut app, 100, true);
        app.active_job.as_mut().unwrap().progress = Some(CommandEvent {
            target: "vt-test-vm".into(),
            step: 1,
            total: 2,
            command: "test".into(),
            program: "sh".into(),
            argv: vec![],
        });
        let running = row(&mut app, 100, true);
        app.config.appearance.icons = "ascii".into();
        let narrow_ascii = row(&mut app, 32, true);
        app.cancel_job();
        let cancelling = row(&mut app, 100, true);
        app.finish_job(Ok(outcome(BatchResult::Success, vec!["vt-test-vm".into()])));
        let success = row(&mut app, 100, false);
        start(&mut app);
        app.finish_job(Ok(outcome(
            BatchResult::Failed {
                target: "vt-test-vm".into(),
                step: 1,
                message: "test failure".into(),
            },
            vec![],
        )));
        let failure = row(&mut app, 100, false);
        insta::assert_snapshot!(format!(
            "preparing: {preparing}\nrunning: {running}\nnarrow ASCII: {narrow_ascii}\ncancelling: {cancelling}\nsuccess: {success}\nfailure: {failure}"
        ));
    }

    #[tokio::test]
    async fn busy_quit_shows_warning_and_ctrl_c_does_not_quit() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest".into(), plan)]));
        assert!(!app.on_key(key(KeyCode::Char('q'))).await);
        assert!(app.dash.message.contains("cancel before quitting"));
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("cancel before quitting"));
        let mut ctrl_c = key(KeyCode::Char('c'));
        ctrl_c.modifiers = KeyModifiers::CONTROL;
        assert!(!app.on_key(ctrl_c).await);
    }

    #[tokio::test]
    async fn busy_quit_reconnect_and_terminal_handoffs_are_refused() {
        for route in ["q", "ZZ", ":q", ":qa", ":connect", "console", "editor"] {
            let mut app = fixture_app();
            let uri = app.uri.clone();
            let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
            assert!(app.submit_plans(vec![("guest".into(), plan)]));
            let id = app.active_job.as_ref().unwrap().id;
            match route {
                "q" => assert!(!app.on_key(key(KeyCode::Char('q'))).await),
                "ZZ" => {
                    let mut z = key(KeyCode::Char('Z'));
                    z.modifiers = KeyModifiers::SHIFT;
                    assert!(!app.on_key(z).await);
                    assert!(!app.on_key(z).await);
                }
                ":q" | ":qa" | ":connect" => {
                    let line = match route {
                        ":q" => "q",
                        ":qa" => "qa",
                        _ => "connect test:///other",
                    };
                    app.ex_line = Some(crate::input::textinput::TextInput::with(line));
                    assert!(!app.on_key(key(KeyCode::Enter)).await);
                }
                "console" => assert!(!app.on_key(key(KeyCode::Char('c'))).await),
                "editor" => assert!(!app.on_key(key(KeyCode::Char('e'))).await),
                _ => unreachable!(),
            }
            assert_eq!(app.uri, uri, "{route}");
            assert_eq!(app.active_job.as_ref().unwrap().id, id, "{route}");
            assert!(
                app.dash.message.contains("is running"),
                "{route}: {}",
                app.dash.message
            );
            if matches!(route, "q" | "ZZ" | ":q" | ":qa") {
                assert!(app.dash.message.contains("cancel before quitting"), "{route}");
            }
            app.active_job.take().unwrap().task.abort();
        }
    }

    #[tokio::test]
    async fn ctrl_c_cancels_idempotently_when_busy_and_quits_when_idle() {
        let mut app = fixture_app();
        let mut ctrl_c = key(KeyCode::Char('c'));
        ctrl_c.modifiers = KeyModifiers::CONTROL;
        assert!(app.on_key(ctrl_c).await);
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest".into(), plan)]));
        assert!(!app.on_key(ctrl_c).await);
        assert!(!app.on_key(ctrl_c).await);
        assert!(*app.active_job.as_ref().unwrap().cancel.borrow());
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn busy_q_remains_text_in_an_ex_line() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest".into(), plan)]));
        app.ex_line = Some(crate::input::textinput::TextInput::new());
        assert!(!app.on_key(key(KeyCode::Char('q'))).await);
        assert_eq!(app.ex_line.as_ref().unwrap().value(), "q");
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn busy_job_allows_detail_tab_and_field_navigation() {
        let mut app = fixture_app();
        app.view = View::Detail;
        app.detail = Some(ui::views::detail::DetailState::demo("vt-test-vm"));
        let before = app.detail.as_ref().unwrap().tab;
        assert!(app.submit_plans(vec![]));
        assert!(!app.on_key(key(KeyCode::Char('L'))).await);
        assert_ne!(app.detail.as_ref().unwrap().tab, before);
        app.detail.as_mut().unwrap().tab = ui::views::detail::DetailTab::Hardware;
        assert!(!app.on_key(key(KeyCode::Tab)).await);
        assert!(app.busy_feedback.is_none());
        app.cancel_job();
    }

    #[tokio::test]
    async fn help_and_redraw_remain_available_while_job_runs() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        app.on_key(key(KeyCode::Char('?'))).await;
        assert!(app.help_filter.is_some());
        app.on_key(key(KeyCode::Esc)).await;
        assert!(app.help_filter.is_none());
        let mut redraw = key(KeyCode::Char('l'));
        redraw.modifiers = KeyModifiers::CONTROL;
        app.on_key(redraw).await;
        assert!(app.dash.redraw_requested);
    }

    #[tokio::test]
    async fn network_view_opens_global_overlays() {
        for overlay in ["help", "palette", "settings", "command", "redraw"] {
            let mut app = fixture_app();
            app.view = View::Networks;
            let event = match overlay {
                "help" => key(KeyCode::Char('?')),
                "settings" => key(KeyCode::Char(',')),
                "command" => key(KeyCode::Char(':')),
                "redraw" => {
                    let mut event = key(KeyCode::Char('l'));
                    event.modifiers = KeyModifiers::CONTROL;
                    event
                }
                "palette" => {
                    let mut event = key(KeyCode::Char('p'));
                    event.modifiers = KeyModifiers::CONTROL;
                    event
                }
                _ => unreachable!(),
            };
            app.on_key(event).await;
            assert!(
                match overlay {
                    "help" => app.help_filter.is_some(),
                    "palette" => app.palette.is_some(),
                    "settings" => app.settings.is_some(),
                    "command" => app.ex_line.is_some(),
                    "redraw" => app.dash.redraw_requested,
                    _ => false,
                },
                "{overlay} did not open in the network view"
            );
        }
    }

    #[tokio::test]
    async fn leader_shortcuts_are_not_consumed_by_area_keys() {
        for view in [View::Networks, View::Storage] {
            let mut app = fixture_app();
            app.demo = true;
            app.view = view;
            app.on_key(key(KeyCode::Char(' '))).await;
            app.on_key(key(KeyCode::Char('n'))).await;
            assert!(
                app.wizard.is_some(),
                "new-domain shortcut was consumed in {view:?}"
            );
            assert!(app.nets.new_form.is_none());
            assert!(app.ex_line.is_none());
        }
    }

    #[tokio::test]
    async fn numeric_prefix_is_consumed_before_view_specific_shortcuts() {
        for view in [View::Networks, View::Storage] {
            let mut app = fixture_app();
            app.demo = true;
            app.view = view;
            app.on_key(key(KeyCode::Char('1'))).await;
            assert!(app.engine.has_pending_digits());
            app.on_key(key(KeyCode::Char('n'))).await;
            assert!(
                !app.engine.has_pending_digits(),
                "count was left pending in {view:?}"
            );
            assert!(app.nets.new_form.is_none());
            assert!(app.ex_line.is_none());
            app.last_key = std::time::Instant::now() - Duration::from_millis(200);
            app.on_digit_timeout();
            assert_eq!(app.view, view, "stale count switched the view");
        }
    }

    #[tokio::test]
    async fn area_shortcuts_remain_available_without_a_pending_sequence() {
        let mut networks = fixture_app();
        networks.demo = true;
        networks.view = View::Networks;
        networks.on_key(key(KeyCode::Char('n'))).await;
        assert!(networks.nets.new_form.is_some());

        let mut storage = fixture_app();
        storage.demo = true;
        storage.view = View::Storage;
        storage.on_key(key(KeyCode::Char('n'))).await;
        assert!(storage.ex_line.is_some());
    }

    #[tokio::test]
    async fn media_leader_shortcut_is_not_consumed_by_network_editor_key() {
        let mut app = fixture_app();
        app.demo = true;
        app.view = View::Networks;
        for ch in [' ', 'm', 'e'] {
            app.on_key(key(KeyCode::Char(ch))).await;
        }
        assert!(
            app.messages
                .iter()
                .any(|message| message.contains("change-media"))
        );
        assert!(!app.nets.message.contains("Edited network"));
    }

    #[tokio::test]
    async fn completed_job_records_history_and_one_synthetic_event() {
        let mut app = fixture_app();
        let before = app.events_state.events.len();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 0"], "Finished A");
        app.submit_plans(vec![("A".into(), plan)]);
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert!(app.active_job.is_none());
        assert_eq!(app.history.all().len(), 1);
        assert_eq!(app.events_state.events.len(), before + 1);
    }

    #[test]
    fn action_event_uses_explicit_outcome_instead_of_message_prefix() {
        let mut app = fixture_app();
        app.record_action_event("✓ misleading message", "vt-test-a", false);
        let event = app.events_state.events.last().unwrap();
        assert_eq!(event.event, "failed");
        assert_eq!(event.object, "vt-test-a");
    }

    #[tokio::test]
    async fn preview_events_are_explicit_and_preserve_the_target() {
        let mut app = fixture_app();
        let before = app.events_state.events.len();
        app.messages.push("✓ ordinary diagnostic".into());
        app.messages
            .push_action("preview without a prefix".into(), "vt-test-a", false);
        app.on_key(key(KeyCode::Esc)).await;
        assert_eq!(app.events_state.events.len(), before + 1);
        let event = app.events_state.events.last().unwrap();
        assert_eq!(event.event, "failed");
        assert_eq!(event.object, "vt-test-a");
    }

    #[tokio::test]
    async fn lifecycle_previews_record_one_event_for_the_captured_target() {
        for demo in [false, true] {
            for route in ["keyboard", "command", "confirmation"] {
                let mut app = fixture_app();
                app.demo = demo;
                app.dry_run = !demo;
                app.dash.marks.clear();
                let target = app.dash.selected().name.clone();
                let before = app.events_state.events.len();
                match route {
                    "keyboard" => {
                        app.on_key(key(KeyCode::Char('s'))).await;
                    }
                    "command" => {
                        app.ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                            "start {target}"
                        )));
                        app.on_key(key(KeyCode::Enter)).await;
                    }
                    "confirmation" => {
                        app.on_key(key(KeyCode::Char('D'))).await;
                        app.on_key(key(KeyCode::Char('y'))).await;
                    }
                    _ => unreachable!(),
                }
                assert_eq!(app.events_state.events.len(), before + 1, "{route}, demo={demo}");
                let event = app.events_state.events.last().unwrap();
                assert_eq!(event.event, "done");
                assert_eq!(event.object, target);
            }
        }
    }

    #[tokio::test]
    async fn network_and_pool_previews_keep_their_own_targets() {
        for view in [View::Networks, View::Storage] {
            let mut app = fixture_app();
            app.demo = true;
            app.view = view;
            let target = match view {
                View::Networks => app.nets.selected_name().to_string(),
                View::Storage => app.store.selected_pool().to_string(),
                _ => unreachable!(),
            };
            let before = app.events_state.events.len();
            app.on_key(key(KeyCode::Char('s'))).await;
            assert_eq!(app.events_state.events.len(), before + 1, "{view:?}");
            let event = app.events_state.events.last().unwrap();
            assert_eq!(event.event, "done");
            assert_eq!(event.object, target);
        }
    }

    #[tokio::test]
    async fn partial_batch_records_success_before_failure() {
        let mut app = fixture_app();
        let dir = tempfile::tempdir().unwrap();
        let forbidden = dir.path().join("c-started");
        let (poller, mut requests) = poller::test_poller();
        app.poller = Some(poller);
        let before = app.events_state.events.len();
        let ok = crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 0"], "Finished A");
        let fail = crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 5"], "Finished B");
        let later = crate::command::plan::CommandPlan::single(
            "touch",
            vec![forbidden.to_str().unwrap()],
            "Finished C",
        );
        app.submit_plans(vec![("A".into(), ok), ("B".into(), fail), ("C".into(), later)]);
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        // Review P1-1: one failing target does not stop the others.
        assert!(forbidden.exists(), "the batch must still run C after B failed");
        let tail: Vec<(String, String)> = app.events_state.events[before..]
            .iter()
            .map(|e| (e.object.clone(), e.event.clone()))
            .collect();
        assert!(tail.contains(&("A".into(), "done".into())), "{tail:?}");
        assert!(tail.contains(&("C".into(), "done".into())), "{tail:?}");
        assert!(tail.contains(&("B".into(), "failed".into())), "{tail:?}");
        assert!(
            app.dash.message.contains("2 ok / 1 failed: B"),
            "{}",
            app.dash.message
        );
        assert_eq!(app.history.all().len(), 3);
        assert!(app.history.all()[0].contains("exit 0"));
        assert!(app.history.all()[1].contains("exit 5"));
        assert!(matches!(requests.try_recv(), Ok(poller::Request::Now)));
        assert!(app.messages.iter().any(|m| m.contains("Finished A")));
        assert!(
            app.messages
                .iter()
                .any(|m| m.starts_with("✗ B: step 1/1 failed:"))
        );
    }

    #[tokio::test]
    async fn timed_out_job_reports_uncertain_state_and_refreshes() {
        use crate::command::job::{BatchOutcome, BatchResult, CommandEvent};
        let mut app = fixture_app();
        let (poller, mut requests) = poller::test_poller();
        app.poller = Some(poller);
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 0"], "Finished A");
        assert!(app.submit_plans(vec![("vt-test-a".into(), plan)]));
        app.active_job.as_ref().unwrap().task.abort();
        app.finish_job(Ok(BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![CommandEvent {
                target: "vt-test-a".into(),
                step: 1,
                total: 1,
                command: "sh -c 'exit 0'".into(),
                program: "sh".into(),
                argv: vec!["-c".into(), "exit 0".into()],
            }],
            completed_steps: vec![],
            outputs: vec![],
            completed_targets: vec![],
            result: BatchResult::Timeout,
        }));
        assert!(app.active_job.is_none());
        assert!(app.dash.message.contains("timed out; verify the object state"));
        assert_eq!(app.history.all(), &["sh -c 'exit 0'"]);
        assert_eq!(app.events_state.events.last().unwrap().event, "failed");
        assert_eq!(app.events_state.events.last().unwrap().object, "vt-test-a");
        assert!(matches!(requests.try_recv(), Ok(poller::Request::Now)));
    }

    #[tokio::test]
    async fn event_burst_leaves_work_for_a_later_tick() {
        let mut app = fixture_app();
        let (tx, rx) = tokio::sync::mpsc::channel(600);
        for index in 0..513 {
            tx.try_send(crate::model::LibvirtEvent {
                timestamp: String::new(),
                object: format!("vt-test-{index}"),
                event: "test".into(),
                detail: String::new(),
                kind: "test".into(),
                scope: "domain".into(),
            })
            .unwrap();
        }
        app.events_rx = Some(rx);
        app.on_tick();
        assert_eq!(app.events_rx.as_ref().unwrap().len(), 1);
        app.on_tick();
        assert_eq!(app.events_state.events.last().unwrap().object, "vt-test-512");
    }

    #[tokio::test]
    async fn live_burst_leaves_work_for_the_next_loop() {
        let mut app = fixture_app();
        let (poller, tx) = poller::test_poller_with_messages();
        app.poller = Some(poller);
        for index in 0..65 {
            tx.try_send(LiveMsg::Failed(format!("failure-{index}"))).unwrap();
        }
        app.on_live(LiveMsg::Failed("first".into()));
        assert!(matches!(
            app.poller.as_mut().unwrap().rx.try_recv(),
            Ok(LiveMsg::Failed(message)) if message == "failure-64"
        ));
    }

    #[tokio::test]
    async fn cancellation_event_identifies_only_an_unfinished_started_target() {
        use crate::command::job::{BatchOutcome, BatchResult, CommandEvent};
        for second_started in [false, true] {
            let mut app = fixture_app();
            let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 0"], "Finished");
            assert!(app.submit_plans(vec![("A".into(), plan.clone()), ("B".into(), plan)]));
            app.active_job.as_ref().unwrap().task.abort();
            let event = |target: &str| CommandEvent {
                target: target.into(),
                step: 1,
                total: 1,
                command: format!("command {target}"),
                program: "sh".into(),
                argv: vec![],
            };
            let mut started_steps = vec![event("A")];
            if second_started {
                started_steps.push(event("B"));
            }
            app.finish_job(Ok(BatchOutcome {
                failures: Vec::new(),
                started_steps,
                completed_steps: vec![event("A")],
                outputs: vec![],
                completed_targets: vec!["A".into()],
                result: BatchResult::Cancelled,
            }));
            let result = app.events_state.events.last().unwrap();
            assert_eq!(result.event, "failed");
            assert_eq!(result.object, if second_started { "B" } else { "" });
        }
    }

    #[tokio::test]
    async fn q_remains_back_in_detail_while_job_runs() {
        let mut app = fixture_app();
        app.view = View::Detail;
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        assert!(!app.on_key(key(KeyCode::Char('q'))).await);
        assert_eq!(app.view, View::Dashboard);
    }

    #[tokio::test]
    async fn panicked_job_releases_busy_state_and_reports_failure() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        let job = app.active_job.as_mut().unwrap();
        job.task.abort();
        job.task = tokio::spawn(async { panic!("worker failed") });
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert!(app.active_job.is_none());
        assert!(app.dash.message.contains("failed"));
        assert!(app.submit_plans(vec![]));
    }

    #[tokio::test]
    async fn panicked_worker_retains_started_step_history() {
        let mut app = fixture_app();
        assert!(app.submit_plans(vec![]));
        let mut plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 0"], "test");
        plan.steps = vec![plan.steps[0].clone(); 3];
        let command = crate::command::display::display(&plan.steps[0]);
        let batch = crate::command::job::OwnedBatch {
            uri: "test:///default".into(),
            demo: false,
            dry_run: false,
            plans: vec![("vt-test-vm".into(), plan)],
            temporary_files: vec![],
        };
        let (_cancel, control) = tokio::sync::watch::channel(false);
        let (events, _receiver) = tokio::sync::mpsc::channel(1);
        let (task, ledger) = crate::command::job::spawn_tracked(async move {
            let _ = crate::command::job::run_batch(batch, control, events).await;
            panic!("worker failed after starting a process");
        });
        let job = app.active_job.as_mut().unwrap();
        job.task.abort();
        job.task = task;
        job.started_ledger = ledger;
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert_eq!(
            app.history
                .all()
                .iter()
                .filter(|entry| *entry == &command)
                .count(),
            3
        );
    }

    #[tokio::test]
    async fn cancelled_job_renders_cancelling_state() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        app.cancel_job();
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("cancelling"));
    }

    #[tokio::test]
    async fn lifecycle_route_captures_selected_target_and_arguments() {
        use crate::input::engine::KeyAction;
        let mut app = fixture_app();
        app.dash.marks.clear();
        let target = app.dash.selected().name.clone();
        let result = app.lifecycle_plans(KeyAction::Reboot).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, target);
        assert_eq!(result[0].1.steps[0].argv, vec!["reboot", target.as_str()]);
        app.dash.move_selection(1);
        assert_ne!(result[0].0, app.dash.selected().name);
    }

    #[tokio::test]
    async fn partial_shutdown_tracks_successful_targets_and_records_failure() {
        let mut app = fixture_app();
        let before = app.events_state.events.len();
        let a = crate::command::builders::lifecycle::shutdown("vt-test-a");
        let b = crate::command::builders::lifecycle::shutdown("vt-test-b");
        app.submit_plans(vec![("vt-test-a".into(), a), ("vt-test-b".into(), b)]);
        app.active_job.as_ref().unwrap().task.abort();
        let outcome = crate::command::job::BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![],
            completed_steps: vec![],
            outputs: vec![],
            completed_targets: vec!["vt-test-a".into()],
            result: crate::command::job::BatchResult::Failed {
                target: "vt-test-b".into(),
                step: 1,
                message: "failed".into(),
            },
        };
        app.finish_job(Ok(outcome));
        assert!(
            app.dash
                .shutdowns
                .iter()
                .any(|pending| pending.name == "vt-test-a")
        );
        assert!(
            !app.dash
                .shutdowns
                .iter()
                .any(|pending| pending.name == "vt-test-b")
        );
        assert_eq!(app.events_state.events.len(), before + 2);
    }

    #[tokio::test]
    async fn lifecycle_actions_capture_expected_commands_for_sorted_bulk_targets() {
        use crate::input::engine::KeyAction as A;
        let mut app = fixture_app();
        for name in ["vt-test-a", "vt-test-b"] {
            let mut row = app.dash.rows[0].clone();
            row.name = name.into();
            app.dash.rows.push(row);
        }
        app.dash.marks.clear();
        app.dash.marks.insert("vt-test-b".into());
        app.dash.marks.insert("vt-test-a".into());
        for (action, verb) in [
            (A::Start, "start"),
            (A::Reboot, "reboot"),
            (A::ManagedSave, "managedsave"),
            (A::SendKey, "send-key"),
        ] {
            let plans = app.lifecycle_plans(action).unwrap();
            assert_eq!(
                plans
                    .iter()
                    .map(|(target, _)| target.as_str())
                    .collect::<Vec<_>>(),
                vec!["vt-test-a", "vt-test-b"]
            );
            assert!(plans.iter().all(|(_, plan)| plan.steps[0].argv[0] == verb));
        }
    }

    #[tokio::test]
    async fn raw_job_preserves_sanitized_multiline_output() {
        let mut app = fixture_app();
        let step =
            crate::command::plan::CommandStep::new("sh", vec!["-c", "printf 'first\\nsecond\\033[31m\\n'"]);
        app.submit_raw(step);
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert!(app.messages.iter().any(|message| message == "first"));
        assert!(
            app.messages
                .iter()
                .any(|message| message.starts_with("second") && !message.chars().any(char::is_control))
        );
        assert!(app.dash.message.contains("more lines in :messages"));
    }

    #[test]
    fn raw_job_route_rejects_explicit_connection_and_keeps_destructive_confirmation() {
        assert!(matches!(
            raw_job_step("virsh -c qemu:///system list"),
            Some(Err(_))
        ));
        assert!(raw_job_step("destroy vt-test-a --graceful").is_none());
        let step = raw_job_step("dominfo 'vt-test guest'").unwrap().unwrap();
        assert_eq!(step.argv, vec!["dominfo", "vt-test guest"]);
    }

    #[tokio::test]
    async fn destructive_raw_command_requires_confirmation_before_job_admission() {
        let mut app = fixture_app();
        let line = "!virsh destroy vt-test-a";
        assert!(!run_ex_line(
            line,
            &mut app.uri,
            &mut app.dash,
            &mut app.history,
            &mut app.messages,
            false,
            false,
            &mut app.help_filter,
            &mut app.config,
            &mut app.theme,
            &mut app.confirm,
        ));
        assert!(app.active_job.is_none());
        let (confirmation, _, targets) = app
            .confirm
            .as_ref()
            .expect("destructive command needs confirmation");
        assert_eq!(targets, &["vt-test-a"]);
        assert!(
            confirmation
                .commands
                .iter()
                .any(|line| line.contains("destroy vt-test-a"))
        );
        app.run_confirmed();
        let job = app
            .active_job
            .take()
            .expect("confirmed command was not submitted");
        assert_eq!(job.plans[0].1.steps[0].argv, ["destroy", "vt-test-a"]);
        job.task.abort();
    }

    #[test]
    fn ex_job_plans_capture_explicit_target_and_arguments() {
        let plans = ex_job_plans("rename vt-test-a vt-test-b", |_| true)
            .unwrap()
            .unwrap();
        assert_eq!(plans[0].0, "vt-test-a");
        assert_eq!(
            plans[0].1.steps[0].argv,
            vec!["domrename", "vt-test-a", "vt-test-b"]
        );
        assert!(ex_job_plans("destroy vt-test-a", |_| true).is_none());
        assert!(ex_job_plans("attach-iso vt-test-a /tmp/test.iso", |_| true).is_none());
    }

    #[test]
    fn typed_ex_routes_capture_their_explicit_target() {
        let cases = [
            ("start vt-test-a", "start"),
            ("rename vt-test-a vt-test-b", "domrename"),
            ("attach-disk vt-test-a /tmp/disk.qcow2 vdb", "attach-disk"),
            ("detach-disk vt-test-a vdb", "detach-disk"),
            ("attach-nic vt-test-a default virtio", "attach-interface"),
            ("block-resize vt-test-a vda 2G", "blockresize"),
            ("media vt-test-a hdb /tmp/test.iso", "change-media"),
        ];
        for (line, verb) in cases {
            let plans = ex_job_plans(line, |_| true)
                .unwrap_or_else(|| panic!("missing route: {line}"))
                .unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_eq!(plans[0].0, "vt-test-a", "{line}");
            assert_eq!(plans[0].1.steps[0].argv[0], verb, "{line}");
        }
    }

    #[tokio::test]
    async fn reboot_shortcut_submits_captured_plan_without_blocking() {
        let mut app = fixture_app();
        app.dash.marks.clear();
        let target = app.dash.selected().name.clone();
        assert!(!app.on_key(key(KeyCode::Char('r'))).await);
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, target);
        assert_eq!(job.plans[0].1.steps[0].argv, vec!["reboot", target.as_str()]);
        app.cancel_job();
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
    }

    #[tokio::test]
    async fn lifecycle_entry_routes_submit_owned_jobs_and_reject_a_second_action() {
        for route in ["keyboard", "command", "palette", "repeat"] {
            let mut app = fixture_app();
            app.dash.marks.clear();
            let target = app.dash.selected().name.clone();
            match route {
                "keyboard" => {
                    app.on_key(key(KeyCode::Char('r'))).await;
                }
                "command" => {
                    app.ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                        "start {target}"
                    )));
                    app.on_key(key(KeyCode::Enter)).await;
                }
                "palette" => {
                    let mut palette = ui::overlays::palette::PaletteState::open_live(&target, &[]);
                    palette.query = "start".into();
                    app.palette = Some(palette);
                    app.on_key(key(KeyCode::Enter)).await;
                    app.on_key(key(KeyCode::Enter)).await;
                }
                "repeat" => {
                    app.on_key(key(KeyCode::Char('r'))).await;
                    let first = app.active_job.take().unwrap();
                    first.task.abort();
                    app.on_key(key(KeyCode::Char('.'))).await;
                }
                _ => unreachable!(),
            }
            let job = app
                .active_job
                .as_ref()
                .unwrap_or_else(|| panic!("{route}: no job"));
            let id = job.id;
            assert_eq!(job.plans[0].0, target, "{route}");
            let previous = app.dash.selection;
            app.on_key(key(KeyCode::Char('j'))).await;
            assert_ne!(app.dash.selection, previous, "{route}: navigation blocked");
            app.on_key(key(KeyCode::Char('r'))).await;
            assert_eq!(app.active_job.as_ref().unwrap().id, id, "{route}: replaced job");
            let job = app.active_job.take().unwrap();
            job.task.abort();
        }
    }

    #[tokio::test]
    async fn destructive_action_obeys_confirmation_setting() {
        const CHILD: &str = "VT_TEST_CONFIRMATION_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "app::tests::destructive_action_obeys_confirmation_setting",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated confirmation test failed: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        for (shortcut, verb) in [('D', "destroy"), ('R', "reset"), ('X', "undefine")] {
            for ask in [true, false] {
                let mut config = Config::default();
                config.confirmations.destroy = ask;
                config.confirmations.reset = ask;
                config.confirmations.undefine = ask;
                config.confirmations.type_name_for_undefine = false;
                crate::config::set_runtime(&config);
                let mut app = fixture_app();
                app.dash.marks.clear();
                let target = app.dash.selected().name.clone();
                app.on_key(key(KeyCode::Char(shortcut))).await;
                if ask {
                    assert!(app.confirm.is_some(), "{verb}: missing confirmation");
                    assert!(app.active_job.is_none());
                    app.on_key(key(KeyCode::Char('y'))).await;
                } else {
                    assert!(app.confirm.is_none(), "{verb}: unexpected confirmation");
                }
                let job = app.active_job.take().unwrap_or_else(|| panic!("{verb}: no job"));
                assert_eq!(job.plans[0].0, target);
                assert_eq!(job.plans[0].1.steps[0].argv[0], verb);
                job.task.abort();
            }
        }
    }

    #[tokio::test]
    async fn raw_ex_command_submits_job_without_spawning_on_ui_loop() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with("dominfo vt-test-a"));
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        let job = app.active_job.as_ref().unwrap();
        assert!(job.raw);
        assert_eq!(job.plans[0].1.steps[0].argv, vec!["dominfo", "vt-test-a"]);
        app.cancel_job();
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
    }

    #[tokio::test]
    async fn refresh_while_job_runs_notifies_poller_without_waiting() {
        let mut app = fixture_app();
        let (poller, mut requests) = poller::test_poller();
        app.poller = Some(poller);
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        let mut refresh = key(KeyCode::Char('r'));
        refresh.modifiers = KeyModifiers::CONTROL;
        assert!(!app.on_key(refresh).await);
        assert!(matches!(requests.try_recv(), Ok(poller::Request::Now)));
    }

    #[tokio::test]
    async fn coordinated_shutdown_cancels_and_reaps_active_job() {
        let mut app = fixture_app();
        let dir = tempfile::tempdir().unwrap();
        let started = dir.path().join("started");
        let release = dir.path().join("release");
        let step = crate::command::plan::CommandStep {
            program: "python3".into(),
            argv: vec![
                concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
                "--started".into(),
                started.to_string_lossy().into_owned(),
                "--release".into(),
                release.to_string_lossy().into_owned(),
            ],
            stdin: None,
        };
        app.submit_raw(step);
        tokio::time::timeout(Duration::from_secs(3), async {
            while !started.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        #[cfg(target_os = "linux")]
        let pid: u32 = std::fs::read_to_string(started).unwrap().parse().unwrap();
        app.shutdown_active_job().await;
        assert!(app.active_job.is_none());
        #[cfg(target_os = "linux")]
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }

    #[tokio::test]
    async fn raw_ex_commands_from_network_view_use_background_job() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.ex_line = Some(crate::input::textinput::TextInput::with("dominfo vt-test-a"));
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        assert!(app.active_job.as_ref().is_some_and(|job| job.raw));
        app.cancel_job();
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
    }

    #[tokio::test]
    async fn raw_ex_commands_from_detail_use_background_job() {
        let mut app = fixture_app();
        app.view = View::Detail;
        app.ex_line = Some(crate::input::textinput::TextInput::with("dominfo vt-test-a"));
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        assert!(app.active_job.as_ref().is_some_and(|job| job.raw));
        app.cancel_job();
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
    }

    #[tokio::test]
    async fn palette_can_be_browsed_but_not_executed_while_job_runs() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        let mut palette_key = key(KeyCode::Char('p'));
        palette_key.modifiers = KeyModifiers::CONTROL;
        app.on_key(palette_key).await;
        assert!(app.palette.is_some());
        app.on_key(key(KeyCode::Char('x'))).await;
        assert_eq!(app.palette.as_ref().unwrap().query, "x");
        app.on_key(key(KeyCode::Enter)).await;
        assert!(app.palette.is_some());
        assert!(app.active_job.is_some());
        app.on_key(key(KeyCode::Esc)).await;
        assert!(app.palette.is_none());
    }

    #[tokio::test]
    async fn settings_can_be_viewed_but_not_changed_while_job_runs() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        app.on_key(key(KeyCode::Char(','))).await;
        assert!(app.settings.is_some());
        let before = app.config.clone();
        app.on_key(key(KeyCode::Enter)).await;
        assert_eq!(app.config, before);
        app.on_key(key(KeyCode::Esc)).await;
        assert!(app.settings.is_none());
    }

    #[tokio::test]
    async fn connection_setting_cannot_change_during_a_job() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        let mut settings = ui::views::settings::SettingsState::open(&app.config);
        settings.cat = 3;
        settings.pane = ui::views::settings::SettingsPane::Options;
        settings.opt_idx = 1;
        app.settings = Some(settings);
        let old_config = app.config.clone();
        let old_uri = app.uri.clone();
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        assert_eq!(app.config, old_config);
        assert_eq!(app.uri, old_uri);
        assert!(app.dash.message.contains("is running"));
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn messages_can_be_opened_without_allowing_new_commands_while_busy() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        app.messages.push("previous result".into());
        app.on_key(key(KeyCode::Char(':'))).await;
        for ch in "messages".chars() {
            app.on_key(key(KeyCode::Char(ch))).await;
        }
        app.on_key(key(KeyCode::Enter)).await;
        // Review P1-27: the full history opens in the :messages overlay.
        assert_eq!(app.messages_view, Some(0));
        assert_eq!(app.messages.last().map(String::as_str), Some("previous result"));
        app.on_key(key(KeyCode::Esc)).await;
        assert!(app.messages_view.is_none());
        assert!(app.active_job.is_some());
        app.on_key(key(KeyCode::Char(':'))).await;
        for ch in "start vt-test-a".chars() {
            app.on_key(key(KeyCode::Char(ch))).await;
        }
        app.on_key(key(KeyCode::Enter)).await;
        assert!(app.dash.message.contains("is running"));
        assert_eq!(app.active_job.as_ref().unwrap().id, 1);
    }

    #[test]
    fn stale_selection_load_does_not_replace_current_network_leases() {
        let mut app = fixture_app();
        app.read_serial = 2;
        let before = app.nets.leases.len();
        let mut stale = app.nets.clone();
        stale.leases.clear();
        let result = requests::SelectionResult::Networks {
            uri: app.uri.clone(),
            serial: 1,
            selected: app.nets.selected_name().to_string(),
            state: Box::new(stale),
        };
        app.apply_selection_result(result);
        assert_eq!(app.nets.leases.len(), before);
    }

    #[tokio::test]
    async fn job_completion_invalidates_older_selection_loads() {
        let mut app = fixture_app();
        app.read_serial = 7;
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 0"], "test");
        app.submit_plans(vec![("vt-test-a".into(), plan)]);
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert_ne!(app.read_serial, 7);
    }

    #[test]
    fn stale_view_load_is_rejected_after_connection_reuse() {
        let mut app = fixture_app();
        app.connection_epoch = 2;
        app.view_load_serial.insert(View::Networks, 3);
        let original = app.nets.networks.len();
        let mut stale = app.nets.clone();
        stale.networks.clear();
        app.on_loaded(Loaded::Nets(app.uri.clone(), 0, 3, Box::new(stale)));
        assert_eq!(app.nets.networks.len(), original);
    }

    #[tokio::test]
    async fn old_selection_read_is_rejected_after_a_b_a_connection_switch() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.read_serial = 11;
        let original_uri = app.uri.clone();
        let original_state = app.nets.clone();
        let target = original_state.selected_name().to_string();
        let mut stale_state = original_state.clone();
        stale_state.leases.clear();
        let old_result = requests::SelectionResult::Networks {
            uri: original_uri.clone(),
            serial: app.read_serial,
            selected: target,
            state: Box::new(stale_state),
        };
        for uri in ["test:///vt-test-other", original_uri.as_str()] {
            app.ex_line = Some(crate::input::textinput::TextInput::with(&format!(
                "connect {uri}"
            )));
            assert!(!app.on_key(key(KeyCode::Enter)).await);
        }
        assert_eq!(app.uri, original_uri);
        assert_eq!(app.connection_epoch, 2);
        app.nets = original_state;
        let lease_count = app.nets.leases.len();
        assert!(lease_count > 0);
        app.apply_selection_result(old_result);
        assert_eq!(app.nets.leases.len(), lease_count);
    }

    #[tokio::test]
    async fn repeated_view_refresh_coalesces_to_one_followup() {
        let mut app = fixture_app();
        app.request_view_load(View::Host);
        let serial = app.view_load_serial[&View::Host];
        let task_id = app.view_load_tasks[&View::Host].0.id();
        app.request_view_load(View::Host);
        app.request_view_load(View::Host);
        assert_eq!(app.view_load_serial[&View::Host], serial);
        assert_eq!(app.view_load_tasks[&View::Host].0.id(), task_id);
        assert!(app.view_load_pending.contains(&View::Host));
        app.on_loaded(Loaded::Host(
            app.uri.clone(),
            app.connection_epoch,
            serial,
            Box::new(app.host.clone()),
        ));
        assert!(!app.view_load_pending.contains(&View::Host));
        assert_eq!(app.view_load_serial[&View::Host], serial + 1);
        assert_ne!(app.view_load_tasks[&View::Host].0.id(), task_id);
    }

    #[tokio::test]
    async fn failed_view_load_starts_a_pending_refresh() {
        let mut app = fixture_app();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        app.loads_tx = tx.clone();
        let serial = 1;
        app.view_load_serial.insert(View::Host, serial);
        let task = supervise_view_load(
            View::Host,
            app.uri.clone(),
            app.connection_epoch,
            serial,
            tx,
            async { panic!("injected loader failure") },
        );
        app.view_load_tasks.insert(View::Host, ViewLoadTask(task));
        app.request_view_load(View::Host);
        let loaded = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        app.on_loaded(loaded);
        assert_eq!(app.view_load_serial[&View::Host], serial + 1);
        assert!(app.view_load_tasks.contains_key(&View::Host));
        assert!(!app.view_load_pending.contains(&View::Host));
    }

    #[tokio::test]
    async fn selection_change_preserves_pending_view_refresh() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.request_view_load(View::Networks);
        let serial = app.view_load_serial[&View::Networks];
        app.request_view_load(View::Networks);
        app.request_selection();
        assert!(app.view_load_tasks.contains_key(&View::Networks));
        assert!(app.view_load_serial[&View::Networks] > serial);
        assert!(!app.view_load_pending.contains(&View::Networks));
        let restarted_serial = app.view_load_serial[&View::Networks];
        app.request_selection();
        assert!(app.view_load_tasks.contains_key(&View::Networks));
        assert!(app.view_load_serial[&View::Networks] > restarted_serial);
        assert!(app.view_load_refreshing.contains(&View::Networks));
    }

    #[tokio::test]
    async fn refreshed_nonfirst_selection_does_not_restart_full_view_load() {
        for view in [View::Networks, View::Storage] {
            let mut app = fixture_app();
            app.view = view;
            app.request_view_load(view);
            let serial = app.view_load_serial[&view];
            app.view_load_refreshing.insert(view);
            match view {
                View::Networks => {
                    assert!(app.nets.networks.len() > 1);
                    app.nets.selected = 1;
                    let mut fresh = app.nets.clone();
                    fresh.selected = 0;
                    app.on_loaded(Loaded::Nets(
                        app.uri.clone(),
                        app.connection_epoch,
                        serial,
                        Box::new(fresh),
                    ));
                }
                View::Storage => {
                    assert!(app.store.pools.len() > 1);
                    app.store.pool_selected = 1;
                    let mut fresh = app.store.clone();
                    fresh.pool_selected = 0;
                    app.on_loaded(Loaded::Store(
                        app.uri.clone(),
                        app.connection_epoch,
                        serial,
                        Box::new(fresh),
                    ));
                }
                _ => unreachable!(),
            }
            assert!(!app.view_load_tasks.contains_key(&view));
            assert!(!app.view_load_refreshing.contains(&view));
            // Networks read the leases of the kept selection; Storage shows the
            // volumes cached by the load (no read).
            assert_eq!(!app.selection_task.is_empty(), view == View::Networks);
        }
    }

    #[tokio::test]
    async fn job_completion_invalidates_older_full_view_load() {
        let mut app = fixture_app();
        app.view_load_serial.insert(View::Networks, 4);
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "exit 0"], "test");
        app.submit_plans(vec![("vt-test-a".into(), plan)]);
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        let original = app.nets.networks.len();
        let mut stale = app.nets.clone();
        stale.networks.clear();
        app.on_loaded(Loaded::Nets(
            app.uri.clone(),
            app.connection_epoch,
            4,
            Box::new(stale),
        ));
        assert_eq!(app.nets.networks.len(), original);
    }

    #[test]
    fn full_view_load_preserves_selected_network_and_pool() {
        let mut app = fixture_app();
        app.nets.selected = 1;
        app.store.pool_selected = 1;
        let network = app.nets.selected_name().to_string();
        let pool = app.store.selected_pool().to_string();
        let mut fresh_nets = app.nets.clone();
        fresh_nets.selected = 0;
        let mut fresh_store = app.store.clone();
        fresh_store.pool_selected = 0;
        app.view_load_serial.insert(View::Networks, 1);
        app.view_load_serial.insert(View::Storage, 1);
        app.on_loaded(Loaded::Nets(
            app.uri.clone(),
            app.connection_epoch,
            1,
            Box::new(fresh_nets),
        ));
        app.on_loaded(Loaded::Store(
            app.uri.clone(),
            app.connection_epoch,
            1,
            Box::new(fresh_store),
        ));
        assert_eq!(app.nets.selected_name(), network);
        assert_eq!(app.store.selected_pool(), pool);
    }

    #[tokio::test]
    async fn network_selection_is_scheduled_while_command_is_running() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.nets.selected = 0;
        let command = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), command)]);
        assert!(!app.on_key(key(KeyCode::Char('j'))).await);
        assert_eq!(app.nets.selected, 1);
        assert!(!app.selection_task.is_empty());
        app.selection_task.clear();
        app.cancel_job();
    }

    #[test]
    fn pool_switch_shows_cached_volumes_without_a_read() {
        let mut app = fixture_app();
        let pool = app.store.selected_pool().to_string();
        let volumes = app.store.volumes.clone();
        app.store.pool_volumes.insert(pool.clone(), volumes.clone());
        app.store.volumes.clear();
        app.store.show_pool_volumes(None);
        assert_eq!(app.store.volumes.len(), volumes.len());
        assert!(app.selection_task.is_empty());
    }

    #[tokio::test]
    async fn completion_cache_miss_schedules_help_without_changing_input() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with("dom"));
        app.tab_complete(false);
        assert!(app.help_task.is_some());
        assert_eq!(app.ex_line.as_ref().unwrap().value(), "dom");
        app.help_task.take();
    }

    #[tokio::test]
    async fn repeated_completion_requests_coalesce() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with("dom"));
        app.tab_complete(false);
        let first = app.help_task.as_ref().unwrap().task.id();
        app.tab_complete(false);
        assert_eq!(app.help_task.as_ref().unwrap().task.id(), first);
        app.help_task.take();
    }

    #[tokio::test]
    async fn delayed_completion_does_not_replace_edited_input() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with("dom"));
        app.tab_complete(false);
        app.ex_line.as_mut().unwrap().set("changed");
        app.apply_help_result(Ok(requests::HelpData::Index(Some(vec![
            crate::backend::virsh::parse_help::HelpEntry {
                name: "dominfo".into(),
                summary: "info".into(),
            },
        ]))));
        assert_eq!(app.ex_line.as_ref().unwrap().value(), "changed");
    }

    #[tokio::test]
    async fn completion_remains_available_during_job_without_blocking() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        app.ex_line = Some(crate::input::textinput::TextInput::with("dom"));
        assert!(!app.on_key(key(KeyCode::Tab)).await);
        assert!(app.help_task.is_some());
        assert!(app.active_job.is_some());
        app.help_task.take();
        app.cancel_job();
    }

    #[tokio::test]
    async fn palette_opens_immediately_and_requests_help_in_background() {
        let mut app = fixture_app();
        let mut keypress = key(KeyCode::Char('p'));
        keypress.modifiers = KeyModifiers::CONTROL;
        assert!(!app.on_key(keypress).await);
        assert!(app.palette.is_some());
        assert!(app.help_task.is_some());
        app.help_task.take();
    }

    #[tokio::test]
    async fn missing_palette_help_is_not_retried_forever() {
        let mut app = fixture_app();
        app.virsh_index = Some(vec![]);
        app.open_async_palette();
        let command = app.palette.as_ref().unwrap().needs_doc().unwrap();
        assert!(app.help_task.is_some());
        app.apply_help_result(Ok(requests::HelpData::Doc(command, None)));
        assert!(app.help_task.is_none());
    }

    #[tokio::test]
    async fn confirmed_network_plan_returns_without_waiting_for_command() {
        let mut app = fixture_app();
        app.view = View::Networks;
        let dir = tempfile::tempdir().unwrap();
        let started = dir.path().join("started").to_string_lossy().into_owned();
        let release = dir.path().join("release").to_string_lossy().into_owned();
        let build = std::sync::Arc::new(move |_: &str| crate::command::plan::CommandPlan {
            steps: vec![crate::command::plan::CommandStep {
                program: "python3".into(),
                stdin: None,
                argv: vec![
                    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/slow_command.py").into(),
                    "--started".into(),
                    started.clone(),
                    "--release".into(),
                    release.clone(),
                ],
            }],
            summary: "network".into(),
        });
        app.confirm = Some((
            crate::ui::overlays::confirm::Confirm {
                kind: crate::ui::overlays::confirm::ConfirmKind::Always,
                title: "Test".into(),
                question: "Test".into(),
                commands: vec![],
                confirm_label: "y test".into(),
                type_name: None,
                typed: String::new(),
            },
            build,
            vec!["vt-test-net".into()],
        ));
        let result =
            tokio::time::timeout(Duration::from_millis(500), app.on_key(key(KeyCode::Char('y')))).await;
        assert!(result.is_ok(), "confirmation blocked the UI");
        assert!(app.active_job.is_some());
        app.cancel_job();
    }

    #[test]
    fn network_actions_capture_selected_network() {
        use crate::input::engine::KeyAction as A;
        let mut app = fixture_app();
        app.view = View::Networks;
        let target = app.nets.selected_name().to_string();
        let plans = app.network_plans(A::Start).unwrap();
        assert_eq!(plans[0].0, target);
        assert_eq!(plans[0].1.steps[0].argv, vec!["net-start", target.as_str()]);
        assert!(app.network_plans(A::Destroy).is_none());
    }

    #[test]
    fn storage_pool_action_captures_selected_pool() {
        let mut app = fixture_app();
        app.view = View::Storage;
        let target = app.store.selected_pool().to_string();
        let plans = app.storage_pool_plans('s').unwrap();
        assert_eq!(plans[0].0, target);
        assert_eq!(plans[0].1.steps[0].argv, vec!["pool-start", target.as_str()]);
        assert!(app.storage_pool_plans('D').is_none());
    }

    #[test]
    fn network_reload_preserves_an_edited_form() {
        let mut app = fixture_app();
        let mut fresh = app.nets.clone();
        fresh.new_form = None;
        app.nets.open_new_form();
        app.nets.new_form.as_mut().unwrap().fields[0].current = "vt-test-new".into();
        app.view_load_serial.insert(View::Networks, 1);
        app.on_loaded(Loaded::Nets(
            app.uri.clone(),
            app.connection_epoch,
            1,
            Box::new(fresh),
        ));
        assert_eq!(
            app.nets.new_form.as_ref().unwrap().fields[0].current,
            "vt-test-new"
        );
    }

    #[test]
    fn network_create_preparation_validates_and_captures_xml() {
        let mut app = fixture_app();
        app.nets.open_new_form();
        app.nets.new_form.as_mut().unwrap().fields[0].current = "vt-test-network".into();
        let (name, xml) = prepare_network_create(&app.nets).unwrap();
        assert_eq!(name, "vt-test-network");
        assert!(xml.contains("<name>vt-test-network</name>"));
        assert!(app.nets.new_form.is_some());
    }

    #[tokio::test]
    async fn network_create_submission_keeps_newer_draft() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.nets.open_new_form();
        app.nets.new_form.as_mut().unwrap().fields[0].current = "vt-test-first".into();
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        assert!(app.active_job.is_some());
        assert!(app.nets.new_form.is_none());
        app.nets.open_new_form();
        app.nets.new_form.as_mut().unwrap().fields[0].current = "vt-test-newer".into();
        app.cancel_job();
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert_eq!(
            app.nets.new_form.as_ref().unwrap().fields[0].current,
            "vt-test-newer"
        );
        assert_eq!(app.network_retries.len(), 1);
        app.on_key(key(KeyCode::Esc)).await;
        app.on_key(key(KeyCode::Char('n'))).await;
        assert_eq!(
            app.nets.new_form.as_ref().unwrap().fields[0].current,
            "vt-test-first"
        );
    }

    #[tokio::test]
    async fn failed_network_submission_restores_draft_after_leaving_view() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.nets.open_new_form();
        app.nets.new_form.as_mut().unwrap().fields[0].current = "vt-test-first".into();
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        app.view = View::Host;
        app.cancel_job();
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert_eq!(app.view, View::Host);
        assert_eq!(
            app.nets.new_form.as_ref().unwrap().fields[0].current,
            "vt-test-first"
        );
    }

    #[tokio::test]
    async fn network_draft_can_be_edited_while_job_runs() {
        let mut app = fixture_app();
        app.view = View::Networks;
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        app.submit_plans(vec![("guest".into(), plan)]);
        app.on_key(key(KeyCode::Char('n'))).await;
        assert!(app.nets.new_form.is_some());
        app.on_key(key(KeyCode::Char('x'))).await;
        assert_eq!(app.nets.new_form.as_ref().unwrap().fields[0].current, "x");
        app.on_key(key(KeyCode::Enter)).await;
        assert!(app.nets.new_form.is_some());
        assert!(app.active_job.is_some());
    }

    #[tokio::test]
    async fn partial_network_create_reports_successful_earlier_step() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.nets.open_new_form();
        app.submit_network_create("vt-test-net".into(), "<network/>".into());
        app.active_job.as_ref().unwrap().task.abort();
        let completed = crate::command::job::CommandEvent {
            target: "vt-test-net".into(),
            step: 1,
            total: 3,
            command: "virsh net-define /tmp/test.xml".into(),
            program: "virsh".into(),
            argv: vec![
                "-c".into(),
                "test:///default".into(),
                "net-define".into(),
                "/tmp/test.xml".into(),
            ],
        };
        let outcome = crate::command::job::BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![completed.clone()],
            completed_steps: vec![completed],
            outputs: vec![],
            completed_targets: vec![],
            result: crate::command::job::BatchResult::Failed {
                target: "vt-test-net".into(),
                step: 2,
                message: "failed".into(),
            },
        };
        app.finish_job(Ok(outcome));
        assert!(
            app.messages
                .iter()
                .any(|m| m.contains("net-define") && m.starts_with('✓'))
        );
        assert!(app.messages.iter().any(|m| m.starts_with("✗ step 2/3")));
    }

    #[tokio::test]
    async fn delayed_completion_does_not_move_changed_cursor() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with("dom"));
        app.tab_complete(false);
        app.ex_line.as_mut().unwrap().move_left();
        let cursor = app.ex_line.as_ref().unwrap().cursor();
        app.apply_help_result(Ok(requests::HelpData::Index(Some(vec![
            crate::backend::virsh::parse_help::HelpEntry {
                name: "dominfo".into(),
                summary: "info".into(),
            },
        ]))));
        assert_eq!(app.ex_line.as_ref().unwrap().value(), "dom");
        assert_eq!(app.ex_line.as_ref().unwrap().cursor(), cursor);
    }

    #[tokio::test]
    async fn delayed_completion_does_not_change_reopened_matching_line() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with("dom"));
        app.tab_complete(false);
        app.on_key(key(KeyCode::Esc)).await;
        app.on_key(key(KeyCode::Char(':'))).await;
        for ch in "dom".chars() {
            app.on_key(key(KeyCode::Char(ch))).await;
        }
        app.apply_help_result(Ok(requests::HelpData::Index(Some(vec![
            crate::backend::virsh::parse_help::HelpEntry {
                name: "dominfo".into(),
                summary: "info".into(),
            },
        ]))));
        assert_eq!(app.ex_line.as_ref().unwrap().value(), "dom");
    }

    #[tokio::test]
    async fn wizard_submission_captures_uri_and_keeps_newer_draft() {
        let mut app = fixture_app();
        let draft = ui::overlays::wizard::WizardState {
            step: 5,
            name: "vt-test-a".into(),
            iso_path: "/tmp/test.iso".into(),
            ..Default::default()
        };
        app.wizard = Some(draft.clone());
        assert!(app.submit_wizard(draft));
        let plan = &app.active_job.as_ref().unwrap().plans[0].1;
        assert_eq!(&plan.steps[0].argv[..2], &["--connect", "test:///default"]);
        app.wizard.as_mut().unwrap().name = "vt-test-newer".into();
        app.cancel_job();
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert_eq!(app.wizard.as_ref().unwrap().name, "vt-test-newer");
    }

    #[tokio::test]
    async fn closed_wizard_can_be_reopened_while_earlier_submission_runs() {
        let mut app = fixture_app();
        let draft = ui::overlays::wizard::WizardState {
            name: "vt-test-first".into(),
            ..Default::default()
        };
        app.wizard = Some(draft.clone());
        assert!(app.submit_wizard(draft));
        // Closing the original draft changes its identity, as the normal key path does.
        app.wizard = None;
        app.wizard_revision += 1;
        app.on_key(key(KeyCode::Char(' '))).await;
        app.on_key(key(KeyCode::Char('n'))).await;
        let newer = app
            .wizard
            .as_mut()
            .expect("new wizard could not be opened while the job ran");
        newer.name = "vt-test-newer".into();
        app.active_job.as_ref().unwrap().task.abort();
        app.finish_job(Ok(crate::command::job::BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![],
            completed_steps: vec![],
            outputs: vec![],
            completed_targets: vec!["vt-test-first".into()],
            result: crate::command::job::BatchResult::Success,
        }));
        assert_eq!(app.wizard.as_ref().unwrap().name, "vt-test-newer");
    }

    #[tokio::test]
    async fn busy_wizard_prefix_does_not_allow_other_leader_actions() {
        let mut app = fixture_app();
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest".into(), plan)]));
        let id = app.active_job.as_ref().unwrap().id;
        app.on_key(key(KeyCode::Char(' '))).await;
        assert!(app.engine.pending_prefix().is_some());
        app.on_key(key(KeyCode::Char('x'))).await;
        assert!(app.engine.pending_prefix().is_none());
        assert!(app.wizard.is_none());
        assert_eq!(app.active_job.as_ref().unwrap().id, id);
        assert!(app.dash.message.contains("is running"));
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn busy_wizard_prefix_cannot_bypass_a_conflicting_full_binding() {
        use crate::input::engine::KeyAction;
        let mut app = fixture_app();
        let mut map = crate::input::keymap::default_map();
        map.insert(vec!["Space".into()], KeyAction::Help);
        app.engine = crate::input::engine::KeyEngine::with_map(map);
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest".into(), plan)]));
        app.on_key(key(KeyCode::Char(' '))).await;
        assert!(
            app.help_filter.is_none(),
            "conflicting complete binding was admitted"
        );
        assert!(app.dash.message.contains("is running"));
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn busy_navigation_key_cannot_run_a_custom_unsafe_action() {
        use crate::input::engine::KeyAction;
        let mut app = fixture_app();
        app.demo = true;
        let mut map = crate::input::keymap::default_map();
        map.insert(vec!["j".into()], KeyAction::Console);
        app.engine = crate::input::engine::KeyEngine::with_map(map);
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest".into(), plan)]));
        assert!(!app.on_key(key(KeyCode::Char('j'))).await);
        assert!(app.dash.message.contains("is running"));
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn refused_custom_action_does_not_replace_repeat_history() {
        use crate::input::{engine::KeyAction, key::KeySeq};
        let mut app = fixture_app();
        app.demo = true;
        let mut map = crate::input::keymap::default_map();
        map.insert(vec!["j".into()], KeyAction::Reboot);
        app.engine = crate::input::engine::KeyEngine::with_map(map);
        app.engine.feed(&KeySeq::from_event(&key(KeyCode::Char('s'))));
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest".into(), plan)]));
        app.on_key(key(KeyCode::Char('j'))).await;
        app.active_job.take().unwrap().task.abort();
        app.on_key(key(KeyCode::Char('.'))).await;
        assert!(app.dash.message.contains("Started"), "{}", app.dash.message);
    }

    #[tokio::test]
    async fn busy_wizard_can_advance_and_validate_without_submitting() {
        const CHILD: &str = "VT_TEST_BUSY_WIZARD_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let dir = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "app::tests::busy_wizard_can_advance_and_validate_without_submitting",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("XDG_STATE_HOME", dir.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        let mut app = fixture_app();
        app.wizard = Some(ui::overlays::wizard::WizardState::default());
        let plan = crate::command::plan::CommandPlan::single("sh", vec!["-c", "sleep 30"], "test");
        assert!(app.submit_plans(vec![("guest".into(), plan)]));
        let id = app.active_job.as_ref().unwrap().id;
        app.on_key(key(KeyCode::Char('x'))).await;
        assert_eq!(app.wizard.as_ref().unwrap().name, "x");
        app.on_key(key(KeyCode::Enter)).await;
        assert_eq!(app.wizard.as_ref().unwrap().step, 1);
        let wizard = app.wizard.as_mut().unwrap();
        wizard.step = 5;
        wizard.name.clear();
        assert!(!wizard.problems().is_empty());
        app.on_key(key(KeyCode::Enter)).await;
        assert_ne!(app.wizard.as_ref().unwrap().step, 5);
        assert_eq!(app.active_job.as_ref().unwrap().id, id);
        app.wizard = Some(ui::overlays::wizard::WizardState {
            step: 5,
            name: "vt-test-newer".into(),
            iso_path: "/tmp/test.iso".into(),
            ..Default::default()
        });
        assert!(app.wizard.as_ref().unwrap().problems().is_empty());
        app.on_key(key(KeyCode::Enter)).await;
        assert_eq!(app.active_job.as_ref().unwrap().id, id);
        assert_eq!(app.wizard.as_ref().unwrap().name, "vt-test-newer");
        assert!(app.dash.message.contains("is running"));
        app.active_job.take().unwrap().task.abort();
    }

    #[tokio::test]
    async fn wizard_define_submission_prints_one_phase_then_defines() {
        let mut app = fixture_app();
        let draft = ui::overlays::wizard::WizardState {
            step: 5,
            name: "vt-test-a".into(),
            start_after: false,
            ..Default::default()
        };
        app.wizard = Some(draft.clone());
        assert!(app.submit_wizard(draft));
        let plan = &app.active_job.as_ref().unwrap().plans[0].1;
        assert_eq!(plan.steps.len(), 2);
        // One persistent phase (issue #5), not the install phase alone.
        let argv = &plan.steps[0].argv;
        assert_eq!(argv.last().map(String::as_str), Some("--print-xml"));
        assert!(argv.contains(&"--install=no_install=yes".to_string()));
        assert!(
            argv.iter()
                .any(|arg| arg.starts_with("--boot=") && arg.ends_with("hd,cdrom"))
        );
        assert_eq!(plan.steps[1].argv[0], "define");
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn wizard_submission_captures_viewer_request() {
        let mut app = fixture_app();
        let draft = ui::overlays::wizard::WizardState {
            name: "vt-test-a".into(),
            start_after: true,
            open_console: true,
            ..Default::default()
        };
        assert!(app.submit_wizard(draft));
        assert_eq!(
            app.active_job.as_ref().unwrap().viewer_after.as_deref(),
            Some("vt-test-a")
        );
        app.cancel_job();
        let result = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(result);
        assert!(app.viewer_task.is_none());
    }

    #[tokio::test]
    async fn wizard_success_schedules_viewer_after_job() {
        let mut app = fixture_app();
        let draft = ui::overlays::wizard::WizardState {
            name: "vt-test-a".into(),
            start_after: true,
            open_console: true,
            ..Default::default()
        };
        assert!(app.submit_wizard(draft));
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(Ok(crate::command::job::BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![],
            completed_steps: vec![],
            outputs: vec![],
            completed_targets: vec!["vt-test-a".into()],
            result: crate::command::job::BatchResult::Success,
        }));
        assert!(app.viewer_task.is_some());
        // Do not launch an external viewer in the test.
        app.viewer_task.take();
    }

    #[tokio::test]
    async fn viewer_lookup_is_owned_by_background_task() {
        let mut app = fixture_app();
        app.request_viewer("test:///default".into(), "vt-test-a".into());
        assert!(app.viewer_task.is_some());
        app.viewer_task.take();
    }

    #[tokio::test]
    async fn connection_change_cancels_an_old_viewer_lookup() {
        let mut app = fixture_app();
        app.request_viewer(app.uri.clone(), "vt-test-a".into());
        assert!(app.viewer_task.is_some());
        app.ex_line = Some(crate::input::textinput::TextInput::with(
            "connect test:///vt-test-other",
        ));
        app.on_key(key(KeyCode::Enter)).await;
        assert!(
            app.viewer_task.is_none(),
            "old viewer lookup survived connection change"
        );
    }

    #[tokio::test]
    async fn viewer_shortcut_schedules_background_lookup() {
        let mut app = fixture_app();
        app.on_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE))
            .await;
        assert!(app.viewer_task.is_some());
        app.viewer_task.take();
    }

    #[tokio::test]
    async fn explicit_network_refresh_is_scheduled_in_background() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.request_refresh();
        assert!(app.view_load_tasks.contains_key(&View::Networks));
        app.view_load_tasks.clear();
    }

    #[tokio::test]
    async fn refresh_key_schedules_network_reload() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL))
            .await;
        assert!(app.view_load_tasks.contains_key(&View::Networks));
        app.view_load_tasks.clear();
    }

    #[tokio::test]
    async fn lone_view_digit_resolves_within_responsiveness_budget() {
        let mut app = fixture_app();
        app.engine.feed(&crate::input::key::KeySeq::parse("2"));
        app.last_key = std::time::Instant::now() - Duration::from_millis(200);
        app.on_digit_timeout();
        assert_eq!(app.view, View::Host);
        // Review PF-9: the view load starts at once, not on the next tick.
        assert!(app.view_load_tasks.contains_key(&View::Host));
        app.view_load_tasks.clear();
    }

    #[tokio::test]
    async fn digit_view_switch_cancels_loading_detail() {
        let mut app = fixture_app();
        app.request_detail();
        app.engine.feed(&crate::input::key::KeySeq::parse("2"));
        app.last_key = std::time::Instant::now() - Duration::from_millis(200);
        app.on_digit_timeout();
        assert_eq!(app.view, View::Host);
        assert!(!app.detail_loading);
        assert!(app.selection_task.is_empty());
    }

    #[tokio::test]
    async fn refresh_invalidates_an_older_selection_read() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.selection_task.insert(
            ReadKind::Networks,
            SelectionTask(tokio::spawn(std::future::pending())),
        );
        let serial = app.read_serial;
        app.request_refresh();
        assert!(app.selection_task.is_empty());
        assert_ne!(app.read_serial, serial);
        app.view_load_tasks.clear();
    }

    #[tokio::test]
    async fn detail_open_is_immediate_and_ignores_stale_results() {
        let mut app = fixture_app();
        let name = app.dash.selected().name.clone();
        app.request_detail();
        assert_eq!(app.view, View::Detail);
        assert_eq!(app.detail.as_ref().unwrap().domain, name);
        assert!(!app.selection_task.is_empty());
        let serial = app.read_serial;
        let mut old = app.detail.as_ref().unwrap().clone();
        old.xml = "stale".into();
        app.begin_read(ReadKind::Detail);
        app.apply_selection_result(SelectionResult::Detail {
            uri: app.uri.clone(),
            serial,
            epoch: app.connection_epoch,
            selected: name,
            state: Box::new(old),
        });
        assert_ne!(app.detail.as_ref().unwrap().xml, "stale");
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn detail_result_preserves_tab_and_loading_blocks_edits() {
        let mut app = fixture_app();
        app.request_detail();
        let serial = app.read_serial;
        let mut loaded = app.detail.as_ref().unwrap().clone();
        loaded.xml = "<domain/>".into();
        app.detail.as_mut().unwrap().tab = ui::views::detail::DetailTab::Hardware;
        app.on_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
            .await;
        assert!(!app.detail.as_ref().unwrap().hw_insert);
        app.apply_selection_result(SelectionResult::Detail {
            uri: app.uri.clone(),
            serial,
            epoch: app.connection_epoch,
            selected: loaded.domain.clone(),
            state: Box::new(loaded),
        });
        let detail = app.detail.as_ref().unwrap();
        assert_eq!(detail.tab, ui::views::detail::DetailTab::Hardware);
        assert_eq!(detail.xml, "<domain/>");
        assert!(!app.detail_loading);
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn leaving_loading_detail_cancels_the_pending_read() {
        let mut app = fixture_app();
        app.request_detail();
        let serial = app.read_serial;
        app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
            .await;
        assert_eq!(app.view, View::Dashboard);
        assert!(app.selection_task.is_empty());
        assert!(!app.detail_loading);
        assert_ne!(app.read_serial, serial);
    }

    #[tokio::test]
    async fn picker_read_preserves_edited_query_and_rejects_stale_results() {
        let mut app = fixture_app();
        app.request_picker();
        assert!(app.picker_loading);
        let picker = app.store.picker.as_mut().unwrap();
        let domain = picker.domain.clone();
        picker.query = "edited".into();
        app.apply_selection_result(SelectionResult::Picker {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            domain: domain.clone(),
            cdrom: Some(("hdb".into(), "ide".into(), "old.iso".into())),
        });
        assert_eq!(app.store.picker.as_ref().unwrap().query, "edited");
        assert_eq!(app.store.picker.as_ref().unwrap().target, "hdb");
        assert!(!app.picker_loading);
        app.apply_selection_result(SelectionResult::Picker {
            uri: app.uri.clone(),
            serial: app.read_serial.wrapping_sub(1),
            epoch: app.connection_epoch,
            domain,
            cdrom: Some(("stale".into(), "ide".into(), String::new())),
        });
        assert_eq!(app.store.picker.as_ref().unwrap().target, "hdb");
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn storage_iso_picker_uses_selected_domain_and_returns_immediately() {
        let mut app = fixture_app();
        assert!(!app.request_storage_picker());
        app.store.volumes[0].name = "vt-test.iso".into();
        let domain = app.dash.selected().name.clone();
        assert!(app.request_storage_picker());
        assert_eq!(app.store.picker.as_ref().unwrap().domain, domain);
        assert!(app.picker_loading);
        assert!(!app.selection_task.is_empty());
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn storage_enter_opens_iso_picker_through_input_dispatch() {
        let mut app = fixture_app();
        app.view = View::Storage;
        app.store.volumes[0].name = "vt-test.iso".into();
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
        assert!(app.store.picker.is_some());
        assert!(app.picker_loading);
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn picker_opened_from_dashboard_captures_input() {
        let mut app = fixture_app();
        app.request_picker();
        app.store.picker.as_mut().unwrap().query = "abc".into();
        app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
            .await;
        assert_eq!(app.store.picker.as_ref().unwrap().query, "ab");
        assert_eq!(app.view, View::Storage);
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn picker_opened_from_dashboard_is_visible() {
        let mut app = fixture_app();
        app.request_picker();
        let backend = ratatui::backend::TestBackend::new(174, 43);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let screen: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(screen.contains("Insert media"));
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn completed_job_restarts_picker_read_without_losing_input() {
        let mut app = fixture_app();
        assert!(app.submit_plans(vec![]));
        app.cancel_job();
        let finished = (&mut app.active_job.as_mut().unwrap().task).await;
        app.request_picker();
        app.store.picker.as_mut().unwrap().query = "edited".into();
        let serial = app.read_serial;
        app.finish_job(finished);
        assert!(app.picker_loading);
        assert!(!app.selection_task.is_empty());
        assert_ne!(app.read_serial, serial);
        assert_eq!(app.store.picker.as_ref().unwrap().query, "edited");
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn detail_shortcut_keeps_boot_selection_after_loading() {
        let mut app = fixture_app();
        app.request_detail_tab(ui::views::detail::DetailTab::Hardware, Some("boot"));
        let mut loaded = app.detail.as_ref().unwrap().clone();
        loaded.hw.selected = 0;
        assert_eq!(app.detail.as_ref().unwrap().hw.selected_device().id, "boot");
        app.apply_selection_result(SelectionResult::Detail {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            selected: loaded.domain.clone(),
            state: Box::new(loaded),
        });
        let detail = app.detail.as_ref().unwrap();
        assert_eq!(detail.tab, ui::views::detail::DetailTab::Hardware);
        assert_eq!(detail.hw.selected_device().id, "boot");
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn snapshot_shortcut_opens_modal_only_after_matching_load() {
        use ui::views::detail::{DetailTab, SnapModal};
        let mut app = fixture_app();
        app.request_snapshot_detail(SnapModal::Create);
        let loaded = app.detail.as_ref().unwrap().clone();
        assert_eq!(loaded.tab, DetailTab::Snapshots);
        assert_eq!(loaded.snap_modal, SnapModal::None);
        app.apply_selection_result(SelectionResult::Detail {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            selected: loaded.domain.clone(),
            state: Box::new(loaded),
        });
        assert_eq!(app.detail.as_ref().unwrap().snap_modal, SnapModal::Create);
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn snapshot_load_does_not_open_modal_over_another_tab() {
        use ui::views::detail::{DetailTab, SnapModal};
        let mut app = fixture_app();
        app.request_snapshot_detail(SnapModal::Create);
        let loaded = app.detail.as_ref().unwrap().clone();
        app.detail.as_mut().unwrap().tab = DetailTab::Overview;
        app.apply_selection_result(SelectionResult::Detail {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            selected: loaded.domain.clone(),
            state: Box::new(loaded),
        });
        assert_eq!(app.detail.as_ref().unwrap().snap_modal, SnapModal::None);
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn palette_snapshot_opens_real_detail_in_background() {
        let mut app = fixture_app();
        app.palette = Some(ui::overlays::palette::PaletteState::demo());
        assert_eq!(
            app.palette.as_ref().unwrap().selected_item().unwrap().virsh_cmd,
            "snapshot-create-as"
        );
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
        assert!(app.palette.is_none());
        assert!(app.detail_loading);
        assert!(app.detail.as_ref().unwrap().xml.is_empty());
        assert!(!app.selection_task.is_empty());
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn palette_revert_does_not_open_for_an_empty_snapshot_list() {
        use ui::views::detail::SnapModal;
        let mut app = fixture_app();
        app.request_palette_snapshot(SnapModal::Revert);
        let loaded = app.detail.as_ref().unwrap().clone();
        assert!(loaded.snaps.rows.is_empty());
        app.apply_selection_result(SelectionResult::Detail {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            selected: loaded.domain.clone(),
            state: Box::new(loaded),
        });
        assert_eq!(app.detail.as_ref().unwrap().snap_modal, SnapModal::None);
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn snapshot_revert_submits_owned_safety_and_revert_steps() {
        use ui::views::detail::{DetailState, DetailTab, SnapModal};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Snapshots;
        detail.snap_modal = SnapModal::Revert;
        detail.snaps.safety = true;
        app.view = View::Detail;
        app.detail = Some(detail);
        assert!(app.submit_snapshot(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)));
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert_eq!(job.plans[0].1.steps.len(), 2);
        assert_eq!(job.plans[0].1.steps[0].argv[0], "snapshot-create-as");
        assert_eq!(job.plans[0].1.steps[1].argv[0], "snapshot-revert");
        assert_eq!(app.detail.as_ref().unwrap().snap_modal, SnapModal::None);
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn snapshot_confirmation_key_submits_job() {
        use ui::views::detail::{DetailState, DetailTab, SnapModal};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Snapshots;
        detail.snap_modal = SnapModal::Delete;
        app.view = View::Detail;
        app.detail = Some(detail);
        app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            .await;
        assert_eq!(
            app.active_job.as_ref().unwrap().plans[0].1.steps[0].argv[0],
            "snapshot-delete"
        );
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn snapshot_creation_validates_name_and_captures_options() {
        use ui::views::detail::{DetailState, DetailTab, SnapModal};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Snapshots;
        detail.snap_modal = SnapModal::Create;
        detail.snaps.create_name.clear();
        detail.snaps.create_atomic = false;
        detail.snaps.create_quiesce = true;
        app.view = View::Detail;
        app.detail = Some(detail);
        app.on_key(key(KeyCode::Enter)).await;
        assert!(app.active_job.is_none());
        assert_eq!(app.detail.as_ref().unwrap().snap_modal, SnapModal::Create);
        app.detail.as_mut().unwrap().snaps.create_name = "vt-test-snapshot".into();
        app.on_key(key(KeyCode::Enter)).await;
        let argv = &app.active_job.as_ref().unwrap().plans[0].1.steps[0].argv;
        assert!(argv.contains(&"--quiesce".into()));
        assert!(!argv.contains(&"--atomic".into()));
        app.cancel_job();
    }

    #[tokio::test]
    async fn snapshot_refresh_preserves_new_form_input() {
        let mut app = fixture_app();
        app.view = View::Detail;
        app.detail = Some(ui::views::detail::DetailState::demo("vt-test-a"));
        let snapshots = app.detail.as_ref().unwrap().snaps.clone();
        app.detail.as_mut().unwrap().snaps.create_name = "new draft".into();
        app.apply_selection_result(SelectionResult::Snapshots {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            domain: "vt-test-a".into(),
            state: Box::new(snapshots),
        });
        assert_eq!(app.detail.as_ref().unwrap().snaps.create_name, "new draft");
    }

    #[tokio::test]
    async fn failed_snapshot_job_schedules_read_back() {
        use ui::views::detail::{DetailState, DetailTab, SnapModal};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Snapshots;
        detail.snap_modal = SnapModal::Revert;
        app.view = View::Detail;
        app.detail = Some(detail);
        assert!(app.submit_snapshot(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)));
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(Ok(crate::command::job::BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![],
            completed_steps: vec![],
            outputs: vec![],
            completed_targets: vec![],
            result: crate::command::job::BatchResult::Failed {
                target: "vt-test-a".into(),
                step: 1,
                message: "failed".into(),
            },
        }));
        assert!(!app.selection_task.is_empty());
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn picker_insertion_submits_captured_media_job() {
        let mut app = fixture_app();
        app.store = ui::views::storage::StorageState::demo_picker();
        app.store.picker.as_mut().unwrap().domain = "vt-test-a".into();
        assert!(app.submit_picker(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert_eq!(job.plans[0].1.steps[0].argv[0], "change-media");
        assert!(
            job.plans[0].1.steps[0]
                .argv
                .iter()
                .any(|arg| arg.ends_with(".iso"))
        );
        assert!(app.store.picker.is_none());
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn picker_eject_key_submits_job_from_dashboard() {
        let mut app = fixture_app();
        app.store = ui::views::storage::StorageState::demo_picker();
        app.store.picker.as_mut().unwrap().domain = "vt-test-a".into();
        app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL))
            .await;
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert!(job.plans[0].1.steps[0].argv.contains(&"--eject".into()));
        assert!(job.refresh_storage);
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn picker_enter_key_submits_job_from_dashboard() {
        let mut app = fixture_app();
        app.store = ui::views::storage::StorageState::demo_picker();
        app.store.picker.as_mut().unwrap().domain = "vt-test-a".into();
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert_eq!(job.plans[0].1.steps[0].argv[0], "change-media");
        assert!(job.refresh_storage);
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn picker_cannot_eject_before_drive_detection() {
        let mut app = fixture_app();
        app.request_picker();
        app.on_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL))
            .await;
        assert!(app.store.picker.is_some());
        assert!(app.active_job.is_none());
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn standalone_eject_job_captures_selected_domain() {
        let mut app = fixture_app();
        let domain = app.dash.selected().name.clone();
        assert!(app.submit_media_eject());
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.uri, "test:///default");
        assert_eq!(job.plans[0].0, domain);
        assert_eq!(job.plans[0].1.steps[0].argv, vec!["dumpxml", domain.as_str()]);
        assert!(job.refresh_storage);
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn standalone_eject_shortcut_schedules_job() {
        let mut app = fixture_app();
        for ch in [' ', 'm', 'e'] {
            app.on_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
                .await;
        }
        assert!(app.active_job.is_some());
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn media_change_preparation_captures_explicit_inputs() {
        let mut app = fixture_app();
        assert!(app.submit_media_change("vt-test-a".into(), Some("/tmp/example.iso".into()), true));
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert_eq!(job.plans[0].1.steps[0].argv, vec!["dumpxml", "vt-test-a"]);
        assert!(job.plans[0].1.steps[1].argv.contains(&"/tmp/example.iso".into()));
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn attach_iso_command_without_target_submits_preparation_job() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with(
            "attach-iso vt-test-a /tmp/example.iso",
        ));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert!(job.plans[0].1.steps[1].argv.contains(&"/tmp/example.iso".into()));
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn description_submission_captures_domain_and_connection() {
        let mut app = fixture_app();
        assert!(app.submit_description("vt-test-a".into(), "new description".into()));
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.uri, "test:///default");
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert_eq!(job.plans[0].1.steps.len(), 2);
        assert_eq!(job.plans[0].1.steps[0].argv[0], "dumpxml");
        assert_eq!(job.plans[0].1.steps[1].argv[0], "define");
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn description_command_submits_background_preparation() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with(
            "desc vt-test-a new description",
        ));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
        assert_eq!(app.active_job.as_ref().unwrap().plans[0].0, "vt-test-a");
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn hostdev_submission_captures_domain_and_node() {
        let mut app = fixture_app();
        assert!(app.submit_hostdev("vt-test-a".into(), "pci_test".into()));
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.uri, "test:///default");
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert_eq!(job.plans[0].1.steps[0].argv, vec!["nodedev-dumpxml", "pci_test"]);
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn hostdev_command_submits_background_preparation() {
        let mut app = fixture_app();
        app.ex_line = Some(crate::input::textinput::TextInput::with(
            "attach-hostdev vt-test-a pci_test",
        ));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
        assert_eq!(app.active_job.as_ref().unwrap().plans[0].0, "vt-test-a");
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn hardware_apply_captures_pending_changes_in_job() {
        use ui::views::detail::{DetailState, DetailTab};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Hardware;
        detail.hw.set_field("cpus", "max", "16");
        app.view = View::Detail;
        app.detail = Some(detail);
        assert!(app.submit_hardware());
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, "vt-test-a");
        assert!(job.hardware_snapshot.is_some());
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn hardware_readback_does_not_clear_a_newer_draft() {
        use ui::views::detail::{DetailState, DetailTab};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Hardware;
        detail.hw.set_field("cpus", "max", "16");
        let fingerprint = detail.hw.draft_fingerprint();
        let config = detail.config.clone();
        let xml = detail.xml.clone();
        detail.hw.set_field("memory", "max", "20G");
        app.view = View::Detail;
        app.detail = Some(detail);
        app.apply_selection_result(SelectionResult::Hardware {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            domain: "vt-test-a".into(),
            fingerprint,
            success: true,
            clear_submitted: true,
            state: Some(Box::new((config, xml))),
        });
        assert!(app.detail.as_ref().unwrap().hw.pending_count() > 0);
    }

    #[tokio::test]
    async fn successful_hardware_readback_clears_only_submitted_draft() {
        use ui::views::detail::{DetailState, DetailTab};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Hardware;
        detail.hw.set_field("cpus", "max", "16");
        let fingerprint = detail.hw.draft_fingerprint();
        let config = detail.config.clone();
        let xml = detail.xml.clone();
        app.view = View::Detail;
        app.detail = Some(detail);
        app.apply_selection_result(SelectionResult::Hardware {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            domain: "vt-test-a".into(),
            fingerprint,
            success: true,
            clear_submitted: true,
            state: Some(Box::new((config, xml))),
        });
        assert_eq!(app.detail.as_ref().unwrap().hw.pending_count(), 0);
    }

    #[tokio::test]
    async fn failed_hardware_readback_retains_submitted_edits() {
        use ui::views::detail::{DetailState, DetailTab};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Hardware;
        detail.hw.set_field("cpus", "max", "16");
        let fingerprint = detail.hw.draft_fingerprint();
        let config = detail.config.clone();
        let xml = detail.xml.clone();
        app.view = View::Detail;
        app.detail = Some(detail);
        app.apply_selection_result(SelectionResult::Hardware {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            domain: "vt-test-a".into(),
            fingerprint,
            success: false,
            clear_submitted: true,
            state: Some(Box::new((config, xml))),
        });
        assert!(app.detail.as_ref().unwrap().hw.pending_count() > 0);
    }

    #[tokio::test]
    async fn xml_editor_readback_preserves_unsubmitted_hardware_edits() {
        use ui::views::detail::{DetailState, DetailTab};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Xml;
        detail.hw.set_field("memory", "max", "20G");
        let fingerprint = detail.hw.draft_fingerprint();
        let config = detail.config.clone();
        let xml = detail.xml.clone();
        app.view = View::Detail;
        app.detail = Some(detail);
        app.apply_selection_result(SelectionResult::Hardware {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            domain: "vt-test-a".into(),
            fingerprint,
            success: true,
            clear_submitted: false,
            state: Some(Box::new((config, xml))),
        });
        assert!(app.detail.as_ref().unwrap().hw.pending_count() > 0);
    }

    #[tokio::test]
    async fn hardware_write_command_submits_job() {
        use ui::views::detail::{DetailState, DetailTab};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Hardware;
        detail.hw.set_field("cpus", "max", "16");
        app.view = View::Detail;
        app.detail = Some(detail);
        app.ex_line = Some(crate::input::textinput::TextInput::with("w"));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await;
        assert_eq!(app.active_job.as_ref().unwrap().plans[0].0, "vt-test-a");
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn partial_hardware_failure_requires_reload_before_retry() {
        use ui::views::detail::{DetailState, DetailTab};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Hardware;
        detail.hw.set_field("cpus", "max", "16");
        app.view = View::Detail;
        app.detail = Some(detail);
        assert!(app.submit_hardware());
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
        let step = crate::command::job::CommandEvent {
            target: "vt-test-a".into(),
            step: 1,
            total: 2,
            command: "virsh define".into(),
            program: "virsh".into(),
            argv: vec!["define".into()],
        };
        app.finish_job(Ok(crate::command::job::BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![step.clone()],
            completed_steps: vec![step],
            outputs: vec![],
            completed_targets: vec![],
            result: crate::command::job::BatchResult::Failed {
                target: "vt-test-a".into(),
                step: 2,
                message: "failed".into(),
            },
        }));
        assert_eq!(app.hardware_reload_required.as_deref(), Some("vt-test-a"));
        assert!(app.submit_hardware());
        assert!(app.active_job.is_none());
        assert!(app.detail.as_ref().unwrap().hw.pending_count() > 0);
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn xml_export_job_captures_selected_domain_and_path() {
        let mut app = fixture_app();
        let domain = app.dash.selected().name.clone();
        assert!(app.submit_xml_export());
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, domain);
        assert_eq!(job.plans[0].1.steps[0].argv[0], "dumpxml");
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn xml_export_shortcut_submits_job() {
        let mut app = fixture_app();
        for ch in [' ', 'x'] {
            app.on_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE))
                .await;
        }
        assert!(app.active_job.is_some());
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn successful_hardware_job_waits_for_readback_before_retry() {
        use ui::views::detail::{DetailState, DetailTab};
        let mut app = fixture_app();
        let mut detail = DetailState::demo("vt-test-a");
        detail.tab = DetailTab::Hardware;
        detail.hw.set_field("cpus", "max", "16");
        let fingerprint = detail.hw.draft_fingerprint();
        let config = detail.config.clone();
        let xml = detail.xml.clone();
        app.view = View::Detail;
        app.detail = Some(detail);
        assert!(app.submit_hardware());
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
        app.finish_job(Ok(crate::command::job::BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![],
            completed_steps: vec![],
            outputs: vec![],
            completed_targets: vec!["vt-test-a".into()],
            result: crate::command::job::BatchResult::Success,
        }));
        assert_eq!(app.hardware_reload_required.as_deref(), Some("vt-test-a"));
        assert!(app.submit_hardware());
        assert!(app.active_job.is_none());
        app.apply_selection_result(SelectionResult::Hardware {
            uri: app.uri.clone(),
            serial: app.read_serial,
            epoch: app.connection_epoch,
            domain: "vt-test-a".into(),
            fingerprint,
            success: true,
            clear_submitted: true,
            state: Some(Box::new((config, xml))),
        });
        assert!(app.hardware_reload_required.is_none());
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn failed_xml_validation_retains_draft_without_stealing_another_view() {
        let mut app = fixture_app();
        let draft = EditorDraft {
            uri: app.uri.clone(),
            epoch: app.connection_epoch,
            target: "vt-test-a".into(),
            xml: "<domain><name>vt-test-a</name></domain>".into(),
            network: false,
            wizard: None,
            generation: 1,
            base: String::new(),
        };
        assert!(app.submit_xml_editor(draft));
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
        app.view = View::Host;
        app.finish_job(Ok(crate::command::job::BatchOutcome {
            failures: Vec::new(),
            started_steps: vec![],
            completed_steps: vec![],
            outputs: vec![],
            completed_targets: vec![],
            result: crate::command::job::BatchResult::Failed {
                target: "vt-test-a".into(),
                step: 1,
                message: "validation failed".into(),
            },
        }));
        assert_eq!(app.view, View::Host);
        let retry = app.editor_retries.first().unwrap();
        assert!(retry.xml.contains("validation failed"));
        assert!(retry.xml.contains("<name>vt-test-a</name>"));
    }

    #[tokio::test]
    async fn network_editor_source_ignores_an_obsolete_selection() {
        let mut app = fixture_app();
        app.view = View::Networks;
        app.request_network_editor();
        let target = app.nets.selected_name().to_string();
        let serial = app.read_serial;
        assert!(!app.selection_task.is_empty());
        app.nets.selected = (app.nets.selected + 1).min(app.nets.networks.len() - 1);
        app.apply_selection_result(SelectionResult::EditorSource {
            uri: app.uri.clone(),
            epoch: app.connection_epoch,
            serial,
            target,
            generation: app.editor_generation,
            result: Ok("<network/>".into()),
        });
        assert!(app.editor_retries.is_empty());
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn domain_editor_refuses_unavailable_secure_xml() {
        let mut app = fixture_app();
        app.view = View::Detail;
        app.detail = Some(ui::views::detail::DetailState::demo("vt-test-vm"));
        app.request_domain_editor("vt-test-vm".into());
        app.selection_task.clear();
        app.apply_selection_result(SelectionResult::DomainEditorSource {
            uri: app.uri.clone(),
            epoch: app.connection_epoch,
            serial: app.read_serial,
            target: "vt-test-vm".into(),
            generation: app.editor_generation,
            result: Err("permission denied".into()),
        });
        assert!(
            app.detail
                .as_ref()
                .unwrap()
                .message
                .contains("secure XML unavailable")
        );
        assert!(app.editor_retries.is_empty());
        assert!(app.active_job.is_none());
    }

    #[tokio::test]
    async fn network_editor_source_waits_when_an_overlay_is_open() {
        let mut app = fixture_app();
        app.view = View::Networks;
        let target = app.nets.selected_name().to_string();
        app.request_network_editor();
        app.ex_line = Some(crate::input::textinput::TextInput::with("help"));
        app.apply_selection_result(SelectionResult::EditorSource {
            uri: app.uri.clone(),
            epoch: app.connection_epoch,
            serial: app.read_serial,
            target,
            generation: app.editor_generation,
            result: Ok("<network/>".into()),
        });
        assert_eq!(app.editor_retries.len(), 1);
        assert!(app.ex_line.is_some());
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn wizard_editor_refuses_url_installs() {
        let mut app = fixture_app();
        app.wizard = Some(ui::overlays::wizard::WizardState {
            media: ui::overlays::wizard::MediaKind::Url,
            url: "https://example.org/os/".into(),
            ..Default::default()
        });
        app.request_wizard_editor();
        assert!(!app.selection_task.contains_key(&ReadKind::Editor));
        assert!(app.wizard.as_ref().unwrap().error.contains("URL install"));
    }

    #[tokio::test]
    async fn wizard_editor_source_does_not_open_for_a_changed_draft() {
        let mut app = fixture_app();
        app.wizard = Some(ui::overlays::wizard::WizardState::default());
        let snapshot = app.wizard.as_ref().unwrap().content_snapshot();
        app.request_wizard_editor();
        let serial = app.read_serial;
        let revision = app.wizard_revision;
        app.wizard_revision += 1;
        app.apply_selection_result(SelectionResult::WizardEditorSource {
            uri: app.uri.clone(),
            epoch: app.connection_epoch,
            serial,
            target: "vt-test-new".into(),
            generation: app.editor_generation,
            revision,
            snapshot,
            result: Ok("<domain/>".into()),
        });
        assert!(app.editor_retries.is_empty());
        app.selection_task.clear();
    }

    #[tokio::test]
    async fn connect_switches_immediately_and_discards_old_dashboard_targets() {
        let mut app = fixture_app();
        app.dash.marks.insert("vt-test-old".into());
        app.ex_line = Some(crate::input::textinput::TextInput::with(
            "connect test:///vt-test-next",
        ));
        let result = tokio::time::timeout(Duration::from_millis(250), app.on_key(key(KeyCode::Enter))).await;
        assert!(result.is_ok());
        assert_eq!(app.uri, "test:///vt-test-next");
        assert_eq!(app.connection_epoch, 1);
        assert!(app.dash.rows.is_empty());
        assert!(app.dash.marks.is_empty());
        assert!(app.nets.networks.is_empty());
        assert!(app.store.pools.is_empty());
        assert_eq!(app.dash.message, "loading…");
    }

    #[tokio::test]
    async fn connect_preserves_unsaved_hardware_draft() {
        let mut app = fixture_app();
        app.view = View::Detail;
        let mut detail = ui::views::detail::DetailState::demo("vt-test-vm");
        detail.hw.set_field("cpus", "max", "16");
        let fingerprint = detail.hw.draft_fingerprint();
        app.detail = Some(detail);
        app.ex_line = Some(crate::input::textinput::TextInput::with(
            "connect test:///vt-test-next",
        ));
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        assert_eq!(app.uri, "test:///default");
        assert_eq!(app.detail.as_ref().unwrap().hw.draft_fingerprint(), fingerprint);
        assert!(
            app.detail
                .as_ref()
                .unwrap()
                .message
                .contains("Unsaved hardware changes")
        );
    }

    #[tokio::test]
    async fn connect_retains_network_draft_for_its_original_uri() {
        let mut app = fixture_app();
        app.view = View::Host;
        app.nets.open_new_form();
        app.nets.new_form.as_mut().unwrap().fields[0].current = "vt-test-old".into();
        app.ex_line = Some(crate::input::textinput::TextInput::with(
            "connect test:///vt-test-next",
        ));
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        assert!(app.nets.new_form.is_none());
        assert_eq!(app.network_retries[0].0, "test:///default");
        assert_eq!(app.network_retries[0].1.fields[0].current, "vt-test-old");
    }

    #[test]
    fn empty_network_and_pool_selections_cannot_start_commands() {
        let mut app = fixture_app();
        app.nets.networks.clear();
        app.store.pools.clear();
        assert!(
            app.network_plans(crate::input::engine::KeyAction::Start)
                .unwrap()
                .is_empty()
        );
        assert!(app.storage_pool_plans('s').unwrap().is_empty());
    }

    #[test]
    fn dry_run_media_preview_requires_cached_cdrom_details() {
        let mut app = fixture_app();
        app.dry_run = true;
        app.request_media_preview("vt-test-vm".into(), Some("/tmp/test.iso".into()));
        assert!(app.dash.message.contains("are not loaded"));
        assert!(app.active_job.is_none());
        assert!(app.selection_task.is_empty());
    }

    #[test]
    fn dry_run_media_preview_uses_cached_cdrom_target() {
        let mut app = fixture_app();
        app.dry_run = true;
        let mut detail = ui::views::detail::DetailState::demo("vt-test-vm");
        detail.uri = app.uri.clone();
        detail.config.disks = vec![crate::model::DiskInfo {
            target: "hdb".into(),
            source: String::new(),
            device: "cdrom".into(),
            bus: "ide".into(),
            ..Default::default()
        }];
        app.detail = Some(detail);
        app.request_media_preview("vt-test-vm".into(), Some("/tmp/test.iso".into()));
        assert!(app.dash.message.contains("[dry-run]"));
        assert!(app.dash.message.contains("hdb"));
        assert!(app.selection_task.is_empty());
        assert!(app.active_job.is_none());
    }

    #[tokio::test]
    async fn dry_run_hardware_apply_preserves_pending_edits() {
        let mut app = fixture_app();
        app.dry_run = true;
        app.view = View::Detail;
        let mut detail = ui::views::detail::DetailState::demo("vt-test-vm");
        detail.tab = ui::views::detail::DetailTab::Hardware;
        detail.hw.set_field("cpus", "max", "16");
        let fingerprint = detail.hw.draft_fingerprint();
        app.detail = Some(detail);
        app.ex_line = Some(crate::input::textinput::TextInput::with("w"));
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        let detail = app.detail.as_ref().unwrap();
        assert_eq!(detail.hw.draft_fingerprint(), fingerprint);
        assert!(detail.message.contains("[dry-run]"));
        assert!(app.active_job.is_none());
    }

    #[tokio::test]
    async fn preview_modes_do_not_schedule_ex_command_workflows() {
        for demo in [false, true] {
            for command in [
                "start vt-test-a",
                "desc vt-test-a new description",
                "attach-hostdev vt-test-a pci_test",
                "attach-iso vt-test-a /tmp/example.iso",
                "virsh dominfo vt-test-a",
            ] {
                let mut app = fixture_app();
                app.demo = demo;
                app.dry_run = !demo;
                app.ex_line = Some(crate::input::textinput::TextInput::with(command));
                app.on_key(key(KeyCode::Enter)).await;
                assert!(
                    app.active_job.is_none(),
                    "{command} scheduled a job in preview mode"
                );
                assert!(
                    app.selection_task.is_empty(),
                    "{command} scheduled preparation in preview mode"
                );
            }
        }
    }

    #[tokio::test]
    async fn ex_shutdown_submits_the_explicit_running_domain_as_a_job() {
        let mut app = fixture_app();
        app.dash.rows[0].name = "vt-test-vm".into();
        app.dash.rows[0].state = crate::model::DomainState::Running;
        app.ex_line = Some(crate::input::textinput::TextInput::with("shutdown vt-test-vm"));
        assert!(!app.on_key(key(KeyCode::Enter)).await);
        let job = app.active_job.as_ref().unwrap();
        assert_eq!(job.plans[0].0, "vt-test-vm");
        assert_eq!(job.plans[0].1.steps[0].argv[0], "shutdown");
        app.cancel_job();
        let _ = (&mut app.active_job.as_mut().unwrap().task).await;
    }

    #[tokio::test]
    async fn demo_viewer_and_console_do_not_start_external_processes() {
        let mut app = fixture_app();
        app.demo = true;
        assert!(!app.on_key(key(KeyCode::Char('v'))).await);
        assert!(app.viewer_task.is_none());
        assert!(app.dash.message.contains("[dry-run]"));
        assert!(!app.on_key(key(KeyCode::Char('c'))).await);
        assert!(app.dash.message.contains("[dry-run]"));
    }

    #[test]
    fn demo_snapshot_edit_is_a_preview() {
        let mut detail = Some(ui::views::detail::DetailState::demo("vt-test-vm"));
        detail.as_mut().unwrap().tab = ui::views::detail::DetailTab::Snapshots;
        let mut view = View::Detail;
        assert!(!dispatch_detail(
            crate::input::engine::KeyAction::EditXml,
            &mut detail,
            &mut view,
            "test:///default",
            false,
            true,
        ));
        assert!(detail.unwrap().message.contains("[dry-run]"));
    }
}

#[cfg(test)]
mod review_fixes {
    use super::*;
    fn app(view: View) -> App {
        let mut app = super::tests::fixture_app();
        app.view = view;
        app.dry_run = true;
        app.demo = true;
        app
    }
    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[tokio::test]
    async fn network_view_handles_global_shortcuts() {
        let mut a = app(View::Networks);
        a.on_key(key(':')).await;
        assert!(a.ex_line.is_some());
        a.ex_line = None;
        a.on_key(key('?')).await;
        assert!(a.help_filter.is_some());
        a.help_filter = None;
        a.on_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL))
            .await;
        assert!(a.palette.is_some());
    }
    fn hardware_app() -> App {
        let mut a = app(View::Dashboard);
        let mut detail = crate::ui::views::detail::DetailState::demo("arch-dev");
        detail.tab = crate::ui::views::detail::DetailTab::Hardware;
        a.detail = Some(detail);
        a.view = View::Detail;
        a
    }
    #[tokio::test]
    async fn insert_media_sequence_from_hardware_opens_picker() {
        let mut a = hardware_app();
        for c in [' ', 'm', 'i'] {
            a.on_key(key(c)).await;
        }
        assert!(a.store.picker.is_some(), "Space m i must open the media picker");
        assert_eq!(a.view, View::Storage);
    }
    #[tokio::test]
    async fn insert_media_from_hardware_keeps_unsaved_edits() {
        let mut a = hardware_app();
        a.detail.as_mut().unwrap().hw.set_field("memory", "current", "8G");
        for c in [' ', 'm', 'i'] {
            a.on_key(key(c)).await;
        }
        assert_eq!(a.view, View::Detail);
        assert!(a.store.picker.is_none());
        assert!(a.detail.as_ref().unwrap().hw.pending_count() > 0);
        assert!(!a.detail.as_ref().unwrap().hw_insert);
    }
    #[tokio::test]
    async fn lone_i_in_hardware_still_enters_insert() {
        let mut a = hardware_app();
        a.on_key(key('i')).await;
        assert!(a.detail.as_ref().unwrap().hw_insert);
        assert!(a.store.picker.is_none());
    }
    #[tokio::test]
    async fn insert_media_opens_visible_picker() {
        let mut a = app(View::Dashboard);
        for c in [' ', 'm', 'i'] {
            a.on_key(key(c)).await;
        }
        assert!(a.store.picker.is_some());
        assert_eq!(a.view, View::Storage);
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(174, 43)).unwrap();
        terminal.draw(|f| a.draw(f)).unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Insert media"));
    }
    #[tokio::test]
    async fn eject_media_targets_selected_domain() {
        let mut a = app(View::Dashboard);
        a.dash.select_name("win11-gaming");
        for c in [' ', 'm', 'e'] {
            a.on_key(key(c)).await;
        }
        let cmd = a.history.last().unwrap();
        assert!(!cmd.contains("arch-dev"), "{cmd}");
        assert!(cmd.contains("win11-gaming"));
    }
    #[test]
    fn nic_detach_uses_selected_mac() {
        let xml = "<domain><name>vt-test-review</name><memory>1048576</memory><vcpu>1</vcpu><devices><interface type='network'><mac address='52:54:00:aa:bb:cc'/><source network='default'/><model type='virtio'/><target dev='vnet77'/></interface></devices></domain>";
        let cfg = crate::backend::virsh::parse_xml::parse_domain_xml(xml).unwrap();
        let mut d = ui::views::detail::DetailState::demo("vt-test-review");
        d.hw = ui::views::detail::hardware::HardwareState::from_config(&cfg, xml, true);
        d.hw.selected =
            d.hw.devices
                .iter()
                .position(|d| d.id.starts_with("nic-"))
                .unwrap();
        let mut confirm = None;
        stage_detach(&mut d, &mut confirm);
        let (c, _, _) = confirm.unwrap();
        assert!(!c.commands[0].contains("52:54:00:a3:1f:7c"));
        assert!(c.commands[0].contains("52:54:00:aa:bb:cc"));
    }

    #[tokio::test]
    async fn detail_write_error_remains_visible() {
        let mut a = app(View::Detail);
        let mut d = ui::views::detail::DetailState::demo("vt-test-review");
        d.hw.set_field("boot", "firmware", "bogus");
        a.detail = Some(d);
        a.dash.message = "previous dashboard message".into();
        a.ex_line = Some(crate::input::textinput::TextInput::with("w"));
        a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
        assert!(a.messages.last().unwrap().contains("cannot apply"));
        assert!(a.detail.as_ref().unwrap().message.contains("cannot apply"));
    }
    #[tokio::test]
    async fn storage_handles_colon_and_iso_enter() {
        let mut a = app(View::Storage);
        a.on_key(key(':')).await;
        assert!(a.ex_line.is_some());
        a.ex_line = None;
        assert!(!a.store.isos.is_empty());
        a.store.volumes[0].name = "vt-test-review.iso".into();
        a.store.vol_selected = 0;
        a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
        assert!(a.store.picker.is_some());
    }
    #[tokio::test]
    async fn detail_colon_opens_ex_line() {
        let mut a = app(View::Detail);
        let mut d = ui::views::detail::DetailState::demo("vt-test-review");
        d.hw.set_field("boot", "firmware", "bogus");
        a.detail = Some(d);
        a.on_key(key(':')).await;
        assert!(
            a.ex_line.is_some(),
            "':' must open the ex line in the detail view"
        );
        // `:w` belongs to the detail handler; the dashboard handler would say
        // "not available here" instead of reporting the invalid field.
        a.ex_line = Some(crate::input::textinput::TextInput::with("w"));
        a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
        assert!(
            a.detail.as_ref().unwrap().message.contains("cannot apply"),
            "expected :w to reach the detail handler, got {:?}",
            a.detail.as_ref().unwrap().message
        );
    }
    #[tokio::test]
    async fn snapshot_create_name_is_editable() {
        let mut a = app(View::Detail);
        let mut d = ui::views::detail::DetailState::demo("vt-test-review");
        d.tab = ui::views::detail::DetailTab::Snapshots;
        d.snap_modal = ui::views::detail::SnapModal::Create;
        let name = d.snaps.create_name.clone();
        a.detail = Some(d);
        a.on_key(key('a')).await;
        assert_eq!(a.detail.as_ref().unwrap().snaps.create_name, format!("{name}a"));
        a.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
            .await;
        assert_eq!(a.detail.as_ref().unwrap().snaps.create_name, name);
    }

    #[tokio::test]
    async fn connect_clears_old_detail() {
        let mut a = app(View::Detail);
        let mut d = ui::views::detail::DetailState::demo("vt-test-old-host-vm");
        d.uri = a.uri.clone();
        a.detail = Some(d);
        a.ex_line = Some(crate::input::textinput::TextInput::with(
            "connect qemu:///review-new",
        ));
        a.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).await;
        assert_eq!(a.uri, "qemu:///review-new");
        assert!(a.detail.is_none());
        assert_eq!(a.view, View::Dashboard);
    }

    /// Every cell must carry the theme background: cells left at the
    /// terminal default show the host terminal's own colour (herdr-app
    /// paints them grey, Alacritty happens to match tokyo-night).
    #[test]
    fn every_view_paints_every_cell() {
        for view in [
            View::Dashboard,
            View::Host,
            View::Networks,
            View::Storage,
            View::Events,
            View::Detail,
        ] {
            for (w, h) in [(174, 43), (160, 37), (120, 36), (80, 24)] {
                let mut a = app(view);
                a.detail = Some(ui::views::detail::DetailState::demo("arch-dev"));
                let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| a.draw(f)).unwrap();
                let buf = terminal.backend().buffer();
                let unpainted: Vec<(u16, u16)> = (0..h)
                    .flat_map(|y| (0..w).map(move |x| (x, y)))
                    .filter(|&p| buf[p].bg == ratatui::style::Color::Reset)
                    .collect();
                assert!(
                    unpainted.is_empty(),
                    "{view:?} {w}x{h}: {} unpainted cells, first {:?}",
                    unpainted.len(),
                    &unpainted[..unpainted.len().min(5)]
                );
            }
        }
    }
}
