//! App: main loop, tick, and input.

pub mod backoff;
pub mod external;
pub mod mode;
pub mod poller;

mod detail;
mod domains;
mod ex;
mod live;
mod networks;
mod palette;
mod render;
mod settings;
mod storage;
mod wizard;

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
use live::*;
use networks::*;
use palette::*;
use ratatui::layout::Rect;
use ratatui::style::Style;
use render::*;
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
    let mut config = config.clone();
    if let Some(theme) = &cli.theme {
        config.appearance.theme = theme.clone();
    }
    crate::command::exec::set_demo(cli.demo);
    if !cli.demo {
        crate::ui::chrome::set_live(crate::ui::chrome::LiveChrome {
            hostname: crate::backend::virsh::hostname(),
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
        // Live mode: read-only fetch at startup.
        use crate::backend::{Backend, virsh::VirshBackend};
        let backend = VirshBackend::new(&cli.connect);
        match backend.list_domains().await {
            Ok(summaries) => ui::views::dashboard::DashboardState::from_summaries(&summaries),
            Err(e) => {
                let mut d = ui::views::dashboard::DashboardState::demo();
                d.rows.clear();
                d.message = format!(
                    "✗ virsh list failed: {}",
                    e.to_string().lines().next().unwrap_or("")
                );
                d
            }
        }
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
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut ticker = interval(Duration::from_secs(1));
    let mut events = external::spawn_input_thread();
    let pending_quit_z = false;
    let (keymap, keymap_warnings) = crate::input::keymap::load();
    let engine = crate::input::engine::KeyEngine::with_map(keymap);
    let history = crate::command::history::History::new(200);
    let uri = cli.connect.clone();
    let dry_run = cli.dry_run;
    let demo = cli.demo;
    let confirm: Option<crate::command::plan::PendingConfirm> = None;
    let ex_line: Option<crate::input::textinput::TextInput> = None;
    let mut messages: Vec<String> = vec![dash.message.clone()];
    if let Some(w) = keymap_warnings.first() {
        dash.message = format!("✗ {w}");
        messages.extend(keymap_warnings.iter().cloned());
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
    if !cli.demo {
        for v in [View::Host, View::Networks, View::Storage] {
            spawn_load(&cli.connect, v, &loads_tx);
        }
    }

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
        events_rx,
        view,
        pending_quit_z,
        engine,
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
    };
    loop {
        // After $EDITOR / console: repaint every cell, not just the diff.
        if external::take_full_redraw() {
            terminal.clear()?;
        }
        terminal.draw(|f| app.draw(f))?;
        app.sync_poller();
        tokio::select! {
            Some(msg) = async {
                match app.poller.as_mut() {
                    Some(p) => p.rx.recv().await,
                    None => std::future::pending().await,
                }
            } => app.on_live(msg),
            Some(loaded) = loads_rx.recv() => app.on_loaded(loaded),
            _ = ticker.tick() => app.on_tick().await,
            _ = tokio::time::sleep(Duration::from_millis(100)), if app.engine.has_pending_digits() => {
                app.on_digit_timeout();
            }
            maybe = events.recv() => {
                match maybe {
                    Some(Event::Key(key)) => {
                        if app.on_key(key).await {
                            break;
                        }
                        if std::mem::take(&mut app.dash.redraw_requested) {
                            terminal.clear()?;
                        }
                    }
                    // Terminal resized: repaint everything at the new size.
                    Some(Event::Resize(..)) => terminal.clear()?,
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }
    Ok(())
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
    events_rx: Option<tokio::sync::mpsc::Receiver<crate::model::LibvirtEvent>>,
    view: View,
    pending_quit_z: bool,
    engine: crate::input::engine::KeyEngine,
    history: crate::command::history::History,
    uri: String,
    dry_run: bool,
    demo: bool,
    confirm: Option<crate::command::plan::PendingConfirm>,
    ex_line: Option<crate::input::textinput::TextInput>,
    messages: Vec<String>,
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
        if self.virsh_index.is_none() {
            self.virsh_index = Some(if self.demo {
                Vec::new()
            } else {
                crate::backend::virsh::parse_help::load_index(&self.uri)
                    .into_iter()
                    .map(|e| (e.name, e.summary))
                    .collect()
            });
        }
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
        // Flags of virsh subcommands, loaded once from `virsh help <cmd>`.
        let (uri, demo) = (self.uri.clone(), self.demo);
        let cache = std::cell::RefCell::new(&mut self.virsh_flags);
        let flags = |cmd: &str| -> Vec<(String, String)> {
            if demo || cmd.is_empty() {
                return Vec::new();
            }
            let mut cache = cache.borrow_mut();
            cache
                .entry(cmd.to_string())
                .or_insert_with(|| {
                    crate::backend::virsh::parse_help::load_doc(&uri, cmd)
                        .map(|d| d.options.into_iter().map(|o| (o.flag, o.desc)).collect())
                        .unwrap_or_default()
                })
                .clone()
        };
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
    async fn run_confirmed(&mut self) {
        let Some((_, build, targets)) = self.confirm.take() else {
            return;
        };
        run_plan_on_targets(
            &self.uri,
            &targets,
            &*build,
            self.dry_run,
            self.demo,
            &mut self.dash,
            &mut self.history,
            &mut self.messages,
        )
        .await;
        let msg = self.dash.message.clone();
        match self.view {
            View::Networks => {
                self.nets.message = msg;
                if !self.demo && !self.dry_run {
                    reload_nets(&self.uri, &mut self.nets).await;
                }
            }
            View::Storage => {
                self.store.message = msg;
                if !self.demo && !self.dry_run {
                    reload_store(&self.uri, &mut self.store).await;
                }
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
    async fn apply_confirm_settings(&mut self) {
        let Some((c, _, _)) = self.confirm.as_mut() else {
            return;
        };
        let cfg = crate::config::confirmations();
        let verb = c.confirm_label.trim_start_matches("y ").to_string();
        if verb == "undefine" && !cfg.type_name_for_undefine {
            c.type_name = None;
        }
        let ask = match verb.as_str() {
            "destroy" => cfg.destroy,
            "reset" => cfg.reset,
            "undefine" => cfg.undefine,
            "delete" => cfg.delete_volume,
            "wipe" => cfg.wipe,
            _ => true,
        };
        if !ask {
            self.run_confirmed().await;
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
        if !matches!(self.view, View::Gallery) {
            if self.confirm.is_some()
                || self.wizard.is_some()
                || self.palette.is_some()
                || self.help_filter.is_some()
            {
                crate::ui::widgets::modal::dim_background(f, &self.theme);
            }
            if let Some((c, _, _)) = &self.confirm {
                crate::ui::overlays::confirm::render(f, &self.theme, f.area(), c);
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
                    .is_some_and(|t| t.elapsed().as_millis() >= 300)
            {
                render_which_key(f, &self.theme, &prefix);
            }
            if let Some(wz) = &self.wizard {
                crate::ui::overlays::wizard::render(f, &self.theme, wz);
                render_wizard_status(f, &self.theme, wz);
            }
            if let Some(pal) = &self.palette {
                crate::ui::overlays::palette::render(f, &self.theme, pal);
            }
            if let Some(st) = &self.settings {
                crate::ui::views::settings::render(f, &self.theme, &self.config, st);
            }
            if let Some(filter) = &self.help_filter {
                crate::ui::overlays::help::render(f, &self.theme, filter, self.engine.map());
            }
        }
        crate::ui::widgets::modal::apply_transparency(f, &self.theme);
    }

    /// Keep the background poller in sync with the URI and the selection.
    fn sync_poller(&mut self) {
        // Restart the poller when the connection URI changes (`:connect`).
        if !self.demo && self.poller_uri != self.uri {
            self.poller = Some(poller::spawn(
                &self.uri,
                self.config.general.refresh_interval_secs.max(1),
                self.config
                    .monitoring
                    .balloon_on
                    .then_some(self.config.monitoring.balloon_period_secs.max(1)),
            ));
            self.poller_uri.clone_from(&self.uri);
            self.sent_selection = None;
        }
        // Tell the poller which domain is selected (config + IP are fetched for it).
        if let Some(p) = &self.poller {
            let sel = self
                .detail
                .as_ref()
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
        // Drain whatever else is ready so one redraw covers it all.
        while let Some(msg) = self.poller.as_mut().and_then(|p| p.rx.try_recv().ok()) {
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
    async fn on_tick(&mut self) {
        if let Some(rx) = self.events_rx.as_mut() {
            while let Ok(ev) = rx.try_recv() {
                self.dash.recent_events.push(ev.clone());
                if self.dash.recent_events.len() > 50 {
                    self.dash.recent_events.remove(0);
                }
                self.events_state.push(ev);
            }
        }
        // Refresh Host/Networks/Storage every time they are entered (they are
        // not polled every second like the domain list).
        if self.view != self.prev_view {
            self.prev_view = self.view;
            if !self.demo {
                self.entered.insert(self.view);
                spawn_load(&self.uri, self.view, &self.loads_tx);
            }
        }
    }

    /// A background view load finished: swap it in (keeping the status
    /// message and, when the item still exists, the selection).
    fn on_loaded(&mut self, loaded: Loaded) {
        let keep = |old: &mut String, fresh: &mut String| {
            if old != "loading…" && !old.is_empty() {
                *fresh = std::mem::take(old);
            }
        };
        match loaded {
            Loaded::Host(uri, mut fresh) if uri == self.uri => {
                keep(&mut self.host.message, &mut fresh.message);
                fresh.selected = self.host.selected.min(fresh.domains.len().saturating_sub(1));
                self.host = *fresh;
            }
            Loaded::Nets(uri, mut fresh) if uri == self.uri => {
                // The selected network's leases were loaded for the first row;
                // only keep the selection when it is that row.
                keep(&mut self.nets.message, &mut fresh.message);
                fresh.focus = self.nets.focus;
                self.nets = *fresh;
            }
            Loaded::Store(uri, mut fresh) if uri == self.uri => {
                keep(&mut self.store.message, &mut fresh.message);
                fresh.focus_pools = self.store.focus_pools;
                self.store = *fresh;
            }
            _ => {} // stale: the connection changed meanwhile
        }
    }

    /// Resolve pending count digits after the 300 ms timeout.
    fn on_digit_timeout(&mut self) {
        if self.last_key.elapsed().as_millis() >= 300 {
            for resolved in self.engine.flush() {
                switch_view(resolved.action, &mut self.view);
            }
        }
    }

    /// Handle one key press. Returns true when the app should quit.
    async fn on_key(&mut self, key: KeyEvent) -> bool {
        let before = self.messages.len();
        let had_confirm = self.confirm.is_some();
        let quit = self.on_key_inner(key).await;
        if !had_confirm && self.confirm.is_some() {
            self.apply_confirm_settings().await;
        }
        // Log every command virsh-tui ran (✓/✗ results) as an `app` event.
        let new: Vec<String> = self.messages.iter().skip(before).cloned().collect();
        for m in new {
            let (head, cmd) = m.split_once("  ── ").unwrap_or((m.as_str(), ""));
            if !(head.starts_with('✓') || head.starts_with('✗') || head.starts_with("[dry-run]")) {
                continue;
            }
            let ok = !head.starts_with('✗');
            let ev = crate::model::LibvirtEvent {
                timestamp: jiff::Zoned::now().strftime("%H:%M:%S").to_string(),
                object: cmd.split_whitespace().nth(2).unwrap_or("").to_string(),
                event: String::from(if ok { "done" } else { "failed" }),
                detail: crate::model::sanitize(head.trim_start_matches(['✓', '✗', ' '])),
                kind: String::from("action"),
                scope: String::from("app"),
            };
            self.dash.recent_events.push(ev.clone());
            if self.dash.recent_events.len() > 50 {
                self.dash.recent_events.remove(0);
            }
            self.events_state.push(ev);
        }
        quit
    }

    /// Key handling proper (see `on_key`).
    async fn on_key_inner(&mut self, key: KeyEvent) -> bool {
        // Ignore key release/repeat reports (kitty protocol, Windows).
        if key.kind != crossterm::event::KeyEventKind::Press {
            return false;
        }
        self.last_key = std::time::Instant::now();
        if self.engine.pending_prefix().is_some() && self.which_key_since.is_none() {
            self.which_key_since = Some(std::time::Instant::now());
        }
        // C-c always quits; `q`/`ZZ` are checked only after the overlays that
        // take text input (ex line, palette, wizard, confirm, forms) had their turn.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
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
                self.run_confirmed().await;
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
                        .await
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
                        .await
                    };
                    if quit {
                        return true;
                    }
                    // Results land in dash.message: show them in the current view too.
                    let msg = self.dash.message.clone();
                    self.show_message(msg);
                }
                KeyCode::Backspace => ex.backspace(),
                KeyCode::Left => ex.move_left(),
                KeyCode::Right => ex.move_right(),
                KeyCode::Char(c) if typing => ex.insert(c),
                _ => {}
            }
            return false;
        }
        // Wizard captures all keys while open.
        if self.wizard.is_some()
            && handle_wizard_key(
                key,
                &mut self.wizard,
                &self.uri,
                self.dry_run,
                self.demo,
                &mut self.history,
                &mut self.messages,
                &mut self.dash,
            )
            .await
        {
            return false;
        }
        // Palette and help overlays capture keys while open.
        if self.palette.is_some()
            && handle_palette_key(
                key,
                &mut self.palette,
                &mut self.detail,
                &mut self.view,
                &self.dash,
                &self.host,
                &self.uri,
                self.demo,
                &mut self.ex_line,
            )
            .await
        {
            return false;
        }
        if self.help_filter.is_some() && handle_help_key(key, &mut self.help_filter) {
            return false;
        }
        // Settings captures navigation while open.
        if self.settings.is_some() {
            let before = self.config.clone();
            if handle_settings_key(key, &mut self.settings, &mut self.config, &mut self.theme) {
                if self.config.general.refresh_interval_secs != before.general.refresh_interval_secs
                    || self.config.monitoring != before.monitoring
                {
                    // Restart the poller with the new interval / memory-stats option.
                    self.poller_uri.clear();
                }
                return false;
            }
        }
        // Storage raw keys (picker, forms, pool/vol actions) bypass the engine.
        if self.view == View::Storage
            && handle_storage_key(
                key,
                &mut self.store,
                &self.uri,
                self.dry_run,
                self.demo,
                &mut self.history,
                &mut self.messages,
                &mut self.confirm,
                &mut self.ex_line,
            )
            .await
        {
            return false;
        }
        // Snapshot modals capture keys (y/n/Esc + toggles).
        if self.view == View::Detail
            && self.detail.as_ref().is_some_and(|d| {
                d.tab == crate::ui::views::detail::DetailTab::Snapshots
                    && d.snap_modal != crate::ui::views::detail::SnapModal::None
            })
            && handle_snap_modal(
                key,
                &mut self.detail,
                &self.uri,
                self.dry_run,
                self.demo,
                &mut self.history,
                &mut self.messages,
            )
            .await
        {
            return false;
        }
        // Hardware tab keys bypass the engine (INSERT raw editing, J/K, Tab…).
        if self.view == View::Detail
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
        let text_input = self.nets.new_form.is_some()
            || self.store.picker.is_some()
            || self.detail.as_ref().is_some_and(|d| d.hw_insert);
        if !text_input && handle_key(key, &mut self.pending_quit_z, in_detail) {
            return true;
        }
        // Any non-Z key clears the pending Z for ZZ.
        if key.code != crossterm::event::KeyCode::Char('Z') {
            self.pending_quit_z = false;
        }
        // Normal dispatch through the key engine.
        let seq = crate::input::key::KeySeq::from_event(&key);
        if seq.token() == "Ignored" {
            return false;
        }
        // Networks raw keys (new form, e edit, form editing) bypass the engine.
        if self.view == View::Networks
            && handle_networks_key(
                key,
                &mut self.nets,
                &self.uri,
                self.dry_run,
                self.demo,
                &mut self.history,
                &mut self.messages,
            )
            .await
        {
            return false;
        }
        // ":" opens the ex line directly (also in the map).
        for resolved in self.engine.feed(&seq) {
            if switch_view(resolved.action, &mut self.view) {
                continue;
            }
            use crate::input::engine::KeyAction as A;
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
                            open_detail(
                                &mut self.detail,
                                &mut self.view,
                                &self.dash,
                                &self.host,
                                &self.uri,
                                self.demo,
                            )
                            .await;
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
                                open_detail(
                                    &mut self.detail,
                                    &mut self.view,
                                    &self.dash,
                                    &self.host,
                                    &self.uri,
                                    self.demo,
                                )
                                .await;
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
            if self.view == View::Networks {
                if dispatch_networks(
                    resolved.action,
                    &mut self.nets,
                    &mut self.detail,
                    &mut self.view,
                    &self.uri,
                    self.dry_run,
                    self.demo,
                    &mut self.history,
                    &mut self.messages,
                    &mut self.confirm,
                )
                .await
                {
                    return true;
                }
                // ⏎ on a lease: open the owning domain's detail view.
                if let Some(dom) = self.nets.pending_open.take() {
                    self.dash.select_name(&dom);
                    if self.dash.selected().name == dom {
                        self.view = View::Dashboard;
                        open_detail(
                            &mut self.detail,
                            &mut self.view,
                            &self.dash,
                            &self.host,
                            &self.uri,
                            self.demo,
                        )
                        .await;
                    }
                }
                continue;
            }
            match (self.view, resolved.action) {
                (_, A::InsertMedia) => {
                    open_picker(&mut self.store, &self.detail, &self.dash, &self.uri, self.demo).await;
                    continue;
                }
                (_, A::NewDomain) => {
                    let facts = wizard_host_facts(&self.dash, &self.nets, &self.store, self.demo);
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
                    if self.view != View::Detail {
                        open_detail(
                            &mut self.detail,
                            &mut self.view,
                            &self.dash,
                            &self.host,
                            &self.uri,
                            self.demo,
                        )
                        .await;
                    }
                    if let Some(d) = self.detail.as_mut() {
                        d.tab = crate::ui::views::detail::DetailTab::Hardware;
                        if let Some(i) = d.hw.devices.iter().position(|dev| dev.id == "boot") {
                            d.hw.selected = i;
                        }
                    }
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
                    let ctx = self
                        .detail
                        .as_ref()
                        .map(|d| d.domain.clone())
                        .unwrap_or_else(|| self.dash.selected().name.clone());
                    open_palette(&mut self.palette, &ctx, &self.uri, self.demo);
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
                    eject_media(
                        &self.uri,
                        &mut self.store,
                        self.dry_run,
                        self.demo,
                        &mut self.history,
                        &mut self.messages,
                    )
                    .await;
                    continue;
                }
                (View::Storage, _) => {
                    if dispatch_storage(
                        resolved.action,
                        &mut self.store,
                        &mut self.detail,
                        &mut self.view,
                        &self.dash,
                        &self.uri,
                        self.dry_run,
                        self.demo,
                        &mut self.history,
                        &mut self.messages,
                        &mut self.confirm,
                        &mut self.ex_line,
                    )
                    .await
                    {
                        return true;
                    }
                    continue;
                }
                (View::Dashboard | View::Host, A::Open) => {
                    open_detail(
                        &mut self.detail,
                        &mut self.view,
                        &self.dash,
                        &self.host,
                        &self.uri,
                        self.demo,
                    )
                    .await;
                    continue;
                }
                (View::Dashboard | View::Host, A::SnapshotNew | A::SnapshotRevert | A::SnapshotDelete) => {
                    open_detail(
                        &mut self.detail,
                        &mut self.view,
                        &self.dash,
                        &self.host,
                        &self.uri,
                        self.demo,
                    )
                    .await;
                    if let Some(d) = self.detail.as_mut() {
                        d.tab = crate::ui::views::detail::DetailTab::Snapshots;
                        d.snap_modal = match resolved.action {
                            A::SnapshotNew => crate::ui::views::detail::SnapModal::Create,
                            A::SnapshotRevert => crate::ui::views::detail::SnapModal::Revert,
                            _ => crate::ui::views::detail::SnapModal::Delete,
                        };
                    }
                    continue;
                }
                (_, A::Back) if self.view == View::Detail => {
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
                    open_detail(
                        &mut self.detail,
                        &mut self.view,
                        &self.dash,
                        &self.host,
                        &self.uri,
                        self.demo,
                    )
                    .await;
                    if let Some(d) = self.detail.as_mut() {
                        d.tab = crate::ui::views::detail::DetailTab::Xml;
                    }
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
                )
                .await
                {
                    return true;
                }
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
                )
                .await
                {
                    // Quit requested (handled above for q/ZZ; :qa returns true here).
                    return true;
                }
            }
            if self.demo {}
        }
        if self.engine.pending_prefix().is_none() {
            self.which_key_since = None;
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

fn handle_key(key: KeyEvent, pending_quit_z: &mut bool, in_detail: bool) -> bool {
    // q quits, ZZ quits, C-c quits.
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }
    match key.code {
        KeyCode::Char('q') if key.modifiers.is_empty() => !in_detail,
        KeyCode::Char('Z') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            if *pending_quit_z {
                return true;
            }
            *pending_quit_z = true;
            false
        }
        _ => false,
    }
}

use external::{detached_run, suspend_run};

#[cfg(test)]
mod tests {
    use super::handle_key;
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
    fn q_quits() {
        let mut pending = false;
        assert!(handle_key(key(KeyCode::Char('q')), &mut pending, false));
        assert!(!handle_key(key(KeyCode::Char('q')), &mut pending, true));
    }

    #[test]
    fn zz_quits() {
        let mut pending = false;
        let z = KeyEvent {
            code: KeyCode::Char('Z'),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        assert!(!handle_key(z, &mut pending, false));
        assert!(handle_key(z, &mut pending, false));
    }
}
