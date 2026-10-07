//! Settings view keys and `:set`.

#![allow(clippy::too_many_arguments)]

use super::*;

/// Settings keys: j/k navigate+preview, h/l pane, Enter apply/cycle, Esc revert+close.
pub(super) fn handle_settings_key(
    key: KeyEvent,
    settings: &mut Option<ui::views::settings::SettingsState>,
    config: &mut Config,
    theme: &mut Theme,
) -> bool {
    let Some(st) = settings else { return false };
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
            st.cat = (st.cat as i32 + delta).rem_euclid(7) as usize;
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
    ui::views::settings::Category::ALL[st.cat.min(6)].0
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
        (C::Connections, i) => {
            if let Some(u) = config.connections.uris.get(i) {
                g.default_uri = u.clone();
                config.connections.default = u.clone();
            }
        }
        (C::Monitoring, 0) => m.balloon_on = !m.balloon_on,
        (C::Monitoring, 1) => m.balloon_period_secs = next(&m.balloon_period_secs, &[2, 5, 10, 30]),
        (C::Monitoring, 2) => m.thread_sampling = !m.thread_sampling,
        (C::Confirmations, 0) => c.destroy = !c.destroy,
        (C::Confirmations, 1) => c.reset = !c.reset,
        (C::Confirmations, 2) => c.undefine = !c.undefine,
        (C::Confirmations, 3) => c.type_name_for_undefine = !c.type_name_for_undefine,
        (C::Confirmations, 4) => c.revert = !c.revert,
        (C::Confirmations, 5) => c.delete_volume = !c.delete_volume,
        (C::Confirmations, 6) => c.wipe = !c.wipe,
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

/// Apply `:set key=value` to config, theme, and disk. Returns the message.
pub(super) fn apply_setting(
    _uri: &str,
    config: &mut Config,
    theme: &mut Theme,
    key: &str,
    value: &str,
) -> String {
    let v = value.trim();
    match key {
        "theme" => {
            config.appearance.theme = v.to_string();
            *theme = current_theme(config);
        }
        "transparent" => config.appearance.transparent = v == "true",
        "borders" => config.appearance.borders = v.to_string(),
        "icons" => config.appearance.icons = v.to_string(),
        "graphs" => config.appearance.graphs = v.to_string(),
        "gradient" => config.appearance.gradient = v == "true",
        "dim_modals" => config.appearance.dim_modals = v == "true",
        "default_uri" => config.general.default_uri = v.to_string(),
        "editor" => config.general.editor = v.to_string(),
        "start_view" => config.general.start_view = v.to_string(),
        "viewer" => config.console.viewer = v.to_string(),
        "escape" => config.console.escape = v.to_string(),
        "refresh_interval_secs" => {
            config.general.refresh_interval_secs = v.parse().unwrap_or(1);
        }
        _ => return format!("✗ unknown setting: {key}"),
    }
    *theme = current_theme(config);
    match config.save() {
        Ok(()) => format!("set {key}={v}"),
        Err(e) => format!("✗ save failed: {e}"),
    }
}
