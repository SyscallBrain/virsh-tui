//! Settings view keys and `:set`.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Settings keys: j/k navigate+preview, h/l pane, Enter apply/cycle, Esc revert+close.
pub(super) fn handle_settings_key(
    key: KeyEvent,
    settings: &mut Option<ui::views::settings::SettingsState>,
    config: &mut Config,
    theme: &mut Theme,
    engine: &mut crate::input::engine::KeyEngine,
    keymap_file: &mut crate::input::keymap_file::KeymapFile,
) -> bool {
    let Some(st) = settings else { return false };
    if st.pane == ui::views::settings::SettingsPane::Options
        && category(st) == ui::views::settings::Category::Keybindings
        && super::settings_keys::handle(key, &mut st.keys, engine, keymap_file)
    {
        return true;
    }
    match key.code {
        KeyCode::Esc => {
            config.appearance.theme = st.before_theme.clone();
            *theme = current_theme(config);
            *settings = None;
            return true;
        }
        KeyCode::Char('j') if key.modifiers.is_empty() => {
            move_settings(st, 1, config, theme);
            return true;
        }
        KeyCode::Char('k') if key.modifiers.is_empty() => {
            move_settings(st, -1, config, theme);
            return true;
        }
        KeyCode::Char('h') if key.modifiers.is_empty() => {
            cycle_pane(st, -1);
            return true;
        }
        KeyCode::Char('l') if key.modifiers.is_empty() => {
            cycle_pane(st, 1);
            return true;
        }
        KeyCode::Enter if key.modifiers.is_empty() => {
            apply_settings(st, config, theme);
            return true;
        }
        _ => {}
    }
    false
}

pub(super) fn move_settings(
    st: &mut ui::views::settings::SettingsState,
    delta: i32,
    config: &mut Config,
    theme: &mut Theme,
) {
    use ui::views::settings::SettingsPane;
    match st.pane {
        SettingsPane::Categories => {
            st.cat =
                (st.cat as i32 + delta).rem_euclid(ui::views::settings::Category::ALL.len() as i32) as usize;
            st.opt_idx = 0;
        }
        SettingsPane::List => {
            st.move_theme(delta);
            // Live preview of the whole app.
            let (base, _) = ui::views::settings::resolve(st.selected_theme());
            *theme = ui::views::settings::apply_appearance(base, config);
        }
        SettingsPane::Options => {
            let n = ui::views::settings::option_count(category(st), config).max(1) as i32;
            st.opt_idx = (st.opt_idx as i32 + delta).rem_euclid(n) as usize;
        }
    }
}

pub(super) fn cycle_pane(st: &mut ui::views::settings::SettingsState, delta: i32) {
    use ui::views::settings::SettingsPane;
    // Only Appearance has the theme list in the middle.
    let panes: &[SettingsPane] = if category(st) == ui::views::settings::Category::Appearance {
        &[
            SettingsPane::Categories,
            SettingsPane::List,
            SettingsPane::Options,
        ]
    } else {
        &[SettingsPane::Categories, SettingsPane::Options]
    };
    let i = panes.iter().position(|p| *p == st.pane).unwrap_or(0) as i32;
    st.pane = panes[(i + delta).rem_euclid(panes.len() as i32) as usize];
}

/// Selected settings category.
pub(super) fn category(st: &ui::views::settings::SettingsState) -> ui::views::settings::Category {
    st.category()
}

/// Change option `idx` of a non-Appearance category and save.
pub(super) fn change_option(cat: ui::views::settings::Category, idx: usize, config: &mut Config) {
    use ui::views::settings::Category as C;
    fn next<T: Clone + PartialEq>(cur: &T, all: &[T]) -> T {
        let i = all
            .iter()
            .position(|v| v == cur)
            .map_or(0, |i| (i + 1) % all.len());
        all[i].clone()
    }
    let g = &mut config.general;
    let m = &mut config.monitoring;
    let c = &mut config.confirmations;
    match (cat, idx) {
        (C::General, 0) => {
            let uris = config.connections.uris.clone();
            if !uris.is_empty() {
                g.default_uri = next(&g.default_uri, &uris);
            }
        }
        (C::General, 1) => g.refresh_interval_secs = next(&g.refresh_interval_secs, &[1, 2, 5, 10]),
        (C::General, 2) => {
            let views = ["dashboard", "host", "networks", "storage", "events"].map(String::from);
            g.start_view = next(&g.start_view, &views);
        }
        (C::General, 3) => g.key_hints = !g.key_hints,
        (C::General, 4) => {
            let names = ui::views::dashboard::Sort::NAMES.map(String::from);
            g.default_sort = next(&g.default_sort, &names);
        }
        (C::General, 5) => {
            let names = ui::views::dashboard::Chip::NAMES.map(String::from);
            g.default_filter = next(&g.default_filter, &names);
        }
        (C::General, 6) => {
            g.shutdown_timeout_secs = next(&g.shutdown_timeout_secs, &[30, 60, 90, 180, 300]);
        }
        (C::General, 7) => g.history_size = next(&g.history_size, &[0, 200, 500, 1000, 5000]),
        (C::General, 8) => g.read_only = !g.read_only,
        (C::Connections, i) => {
            if let Some(u) = config.connections.uris.get(i) {
                g.default_uri = u.clone();
                config.connections.default = u.clone();
            }
        }
        (C::Monitoring, 0) => m.balloon_on = !m.balloon_on,
        (C::Monitoring, 1) => m.balloon_period_secs = next(&m.balloon_period_secs, &[2, 5, 10, 30]),
        (C::Monitoring, 2) => m.thread_sampling = !m.thread_sampling,
        (C::Monitoring, 3) => {
            m.history_window = next(&m.history_window, &["60s", "5m", "1h"].map(String::from));
            crate::metrics::store::set_window(&m.history_window);
        }
        (C::Confirmations, 0) => c.destroy = !c.destroy,
        (C::Confirmations, 1) => c.reset = !c.reset,
        (C::Confirmations, 2) => c.undefine = !c.undefine,
        (C::Confirmations, 3) => c.type_name_for_undefine = !c.type_name_for_undefine,
        (C::Confirmations, 4) => c.revert = !c.revert,
        (C::Confirmations, 5) => c.delete_volume = !c.delete_volume,
        (C::Confirmations, 6) => c.wipe = !c.wipe,
        (C::Confirmations, 7) => c.bulk_threshold = next(&c.bulk_threshold, &[0, 3, 5, 10, 20]),
        (C::Console, 0) => {
            let viewers = ["virt-viewer", "remote-viewer"].map(String::from);
            config.console.viewer = next(&config.console.viewer, &viewers);
        }
        _ => return,
    }
    crate::config::set_runtime(config);
    let _ = config.save();
}

/// Enter: apply theme (list pane) or cycle the option (options pane).
pub(super) fn apply_settings(
    st: &mut ui::views::settings::SettingsState,
    config: &mut Config,
    theme: &mut Theme,
) {
    use ui::views::settings::SettingsPane;
    match st.pane {
        SettingsPane::List => {
            config.appearance.theme = st.selected_theme().to_string();
            st.before_theme = config.appearance.theme.clone();
            if config.save().is_err() {
                // Message surfaces via the next settings render.
            }
            *theme = current_theme(config);
        }
        SettingsPane::Options => {
            let cat = category(st);
            if cat == ui::views::settings::Category::Appearance {
                cycle_option(st.opt_idx, config, theme);
            } else {
                change_option(cat, st.opt_idx, config);
            }
        }
        SettingsPane::Categories => {
            st.pane = if category(st) == ui::views::settings::Category::Appearance {
                SettingsPane::List
            } else {
                SettingsPane::Options
            };
        }
    }
}

/// Cycle appearance option N.
pub(super) fn cycle_option(idx: usize, config: &mut Config, theme: &mut Theme) {
    let a = &mut config.appearance;
    match idx {
        0 => {
            a.borders = match a.borders.as_str() {
                "rounded" => String::from("plain"),
                "plain" => String::from("double"),
                "double" => String::from("thick"),
                _ => String::from("rounded"),
            }
        }
        1 => {
            a.icons = match a.icons.as_str() {
                "unicode" => String::from("nerd"),
                "nerd" => String::from("ascii"),
                _ => String::from("unicode"),
            }
        }
        2 => {
            a.graphs = match a.graphs.as_str() {
                "braille" => String::from("block"),
                "block" => String::from("tty"),
                _ => String::from("braille"),
            }
        }
        3 => a.gradient = !a.gradient,
        4 => a.transparent = !a.transparent,
        _ => a.dim_modals = !a.dim_modals,
    }
    let _ = config.save();
    crate::config::set_runtime(config);
    *theme = current_theme(config);
}

/// Apply `:set key=value` to config, theme, runtime config and disk.
/// Values are validated; a rejected value changes nothing. Returns the message.
pub(super) fn apply_setting(
    _uri: &str,
    config: &mut Config,
    theme: &mut Theme,
    key: &str,
    value: &str,
) -> String {
    let v = value.trim();
    let boolean = || match v {
        "true" | "on" | "yes" => Ok(true),
        "false" | "off" | "no" => Ok(false),
        _ => Err(format!("✗ {key} takes true or false")),
    };
    let one_of = |allowed: &[&str]| {
        if allowed.contains(&v) {
            Ok(v.to_string())
        } else {
            Err(format!("✗ {key} must be one of: {}", allowed.join(", ")))
        }
    };
    let mut next = config.clone();
    let result: Result<(), String> = (|| {
        match key {
            "theme" => {
                if ui::views::settings::resolve(v).1.is_some() {
                    return Err(format!("✗ unknown theme `{v}`"));
                }
                next.appearance.theme = v.to_string();
            }
            "transparent" => next.appearance.transparent = boolean()?,
            "borders" => next.appearance.borders = one_of(&["rounded", "plain", "double", "thick"])?,
            "icons" => next.appearance.icons = one_of(&["unicode", "nerd", "ascii"])?,
            "graphs" => next.appearance.graphs = one_of(&["braille", "block", "tty"])?,
            "gradient" => next.appearance.gradient = boolean()?,
            "dim_modals" => next.appearance.dim_modals = boolean()?,
            "key_hints" => next.general.key_hints = boolean()?,
            "read_only" => next.general.read_only = boolean()?,
            "clock" => next.appearance.clock = one_of(&["24h", "12h", "off"])?,
            "mouse" => next.appearance.mouse = boolean()?,
            "columns" => {
                let names: Vec<String> = v
                    .split(',')
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .collect();
                let known: Vec<&str> = ui::views::dashboard::Col::ALL.iter().map(|c| c.name()).collect();
                if let Some(bad) = names.iter().find(|n| !known.contains(&n.as_str())) {
                    return Err(format!("✗ unknown column `{bad}` (use {})", known.join(", ")));
                }
                if names.is_empty() {
                    return Err(format!("✗ {key} takes a comma list: {}", known.join(",")));
                }
                next.appearance.columns = names;
            }
            "notifications" => {
                next.notifications.method = one_of(&["auto", "desktop", "osc9", "osc777", "off"])?;
            }
            "default_uri" if !v.is_empty() => next.general.default_uri = v.to_string(),
            "editor" if !v.is_empty() => next.general.editor = v.to_string(),
            "start_view" => {
                next.general.start_view = one_of(&["domains", "host", "networks", "storage", "events"])?;
            }
            "viewer" => next.console.viewer = one_of(&["virt-viewer", "remote-viewer"])?,
            "escape" if !v.is_empty() => next.console.escape = v.to_string(),
            "refresh_interval_secs" => {
                next.general.refresh_interval_secs = v
                    .parse()
                    .ok()
                    .filter(|n| (1..=60).contains(n))
                    .ok_or_else(|| format!("✗ {key} takes 1-60 (seconds)"))?;
            }
            "default_sort" => {
                next.general.default_sort = one_of(&ui::views::dashboard::Sort::NAMES)?;
            }
            "default_filter" => {
                next.general.default_filter = one_of(&ui::views::dashboard::Chip::NAMES)?;
            }
            "history_window" => next.monitoring.history_window = one_of(&["60s", "5m", "1h"])?,
            "log_level" => {
                next.general.log_level = one_of(&["error", "warn", "info", "debug", "trace", "off"])?;
            }
            "history_size" => {
                next.general.history_size = v
                    .parse()
                    .ok()
                    .filter(|n| *n <= 100_000)
                    .ok_or_else(|| format!("✗ {key} takes 0-100000 (lines; 0: not saved)"))?;
            }
            "shutdown_timeout_secs" => {
                next.general.shutdown_timeout_secs = v
                    .parse()
                    .ok()
                    .filter(|n| (5..=3600).contains(n))
                    .ok_or_else(|| format!("✗ {key} takes 5-3600 (seconds)"))?;
            }
            "bulk_threshold" => {
                next.confirmations.bulk_threshold = v
                    .parse()
                    .ok()
                    .filter(|n| *n <= 1000)
                    .ok_or_else(|| format!("✗ {key} takes 0-1000 (domains; 0: off)"))?;
            }
            "default_uri" | "editor" | "escape" => return Err(format!("✗ {key} cannot be empty")),
            _ => return Err(format!("✗ unknown setting: {key}")),
        }
        Ok(())
    })();
    if let Err(message) = result {
        return message;
    }
    if key == "history_window" {
        crate::metrics::store::set_window(v);
    }
    *config = next;
    *theme = current_theme(config);
    // Code without a handle on the config (viewer, editor, confirmations)
    // reads the runtime copy.
    crate::config::set_runtime(config);
    let note = match key {
        "log_level" | "mouse" => "  (takes effect at the next start)",
        "default_sort" | "default_filter" => "  (used at the next start; o / f change the list now)",
        _ => "",
    };
    match config.save() {
        Ok(()) => format!("set {key}={v}{note}"),
        Err(e) => format!("✗ save failed: {e}"),
    }
}
