//! Custom theme loading from ~/.config/virsh-tui/themes/<name>.toml.

use color_eyre::{Result, eyre::eyre};

use super::{IconSet, Theme, ThemeTokens, rgb};

const KEYS: [&str; 16] = [
    "bg", "bg2", "hl", "sel", "gutter", "comment", "fg", "fg2", "blue", "cyan", "magenta", "green", "yellow",
    "orange", "red", "teal",
];

/// Parse a custom theme from TOML text.
///
/// Expects a `[tokens]` table with all 16 keys as `"#rrggbb"`.
/// A missing key is an error naming the key.
pub fn from_toml(name: &str, text: &str) -> Result<Theme> {
    let value: toml::Value = toml::from_str(text)?;
    let tokens = value
        .get("tokens")
        .and_then(|t| t.as_table())
        .ok_or_else(|| eyre!("theme {name}: missing [tokens]"))?;
    let mut hexes = Vec::new();
    for key in KEYS {
        let hex = tokens
            .get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| eyre!("theme {name}: missing key {key}"))?;
        hexes.push(hex.to_string());
    }
    let colors: Vec<ratatui::style::Color> = hexes.iter().map(|h| rgb(h)).collect();
    let owned: &'static str = Box::leak(name.to_string().into_boxed_str());
    Ok(Theme {
        name: owned,
        tokens: ThemeTokens {
            bg: colors[0],
            bg2: colors[1],
            hl: colors[2],
            sel: colors[3],
            gutter: colors[4],
            comment: colors[5],
            fg: colors[6],
            fg2: colors[7],
            blue: colors[8],
            cyan: colors[9],
            magenta: colors[10],
            green: colors[11],
            yellow: colors[12],
            orange: colors[13],
            red: colors[14],
            teal: colors[15],
        },
        border: ratatui::widgets::BorderType::Rounded,
        transparent: false,
        dim_modals: true,
        graphs: crate::theme::GraphStyle::Braille,
        gradient: true,
        icons: IconSet::Unicode,
    })
}

/// Load a custom theme file. Returns an error string for the message line on failure.
pub fn load_custom(name: &str) -> Result<Theme> {
    let mut path = directories::ProjectDirs::from("", "", "virsh-tui")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    path.push("themes");
    path.push(format!("{name}.toml"));
    let text = std::fs::read_to_string(&path)?;
    from_toml(name, &text)
}
