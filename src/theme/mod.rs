//! Theme tokens, built-in themes, and style helpers.

pub mod builtin;
pub mod load;

use ratatui::style::{Color, Modifier, Style};

/// The 16 theme tokens in binding order:
/// bg bg2 hl sel gutter comment fg fg2 blue cyan magenta green yellow orange red teal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeTokens {
    pub bg: Color,
    pub bg2: Color,
    pub hl: Color,
    pub sel: Color,
    pub gutter: Color,
    pub comment: Color,
    pub fg: Color,
    pub fg2: Color,
    pub blue: Color,
    pub cyan: Color,
    pub magenta: Color,
    pub green: Color,
    pub yellow: Color,
    pub orange: Color,
    pub red: Color,
    pub teal: Color,
}

impl ThemeTokens {
    /// Token hex values in binding order.
    pub fn hex_list(&self) -> [String; 16] {
        [
            color_hex(self.bg),
            color_hex(self.bg2),
            color_hex(self.hl),
            color_hex(self.sel),
            color_hex(self.gutter),
            color_hex(self.comment),
            color_hex(self.fg),
            color_hex(self.fg2),
            color_hex(self.blue),
            color_hex(self.cyan),
            color_hex(self.magenta),
            color_hex(self.green),
            color_hex(self.yellow),
            color_hex(self.orange),
            color_hex(self.red),
            color_hex(self.teal),
        ]
    }
}

/// Area-chart rendering style (Settings › Appearance › Graphs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphStyle {
    #[default]
    Braille,
    Block,
    Tty,
}

impl GraphStyle {
    pub fn parse(s: &str) -> Self {
        match s {
            "block" => Self::Block,
            "tty" => Self::Tty,
            _ => Self::Braille,
        }
    }
}

/// Icon set for state glyphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconSet {
    #[default]
    Unicode,
    Nerd,
    Ascii,
}

impl IconSet {
    pub fn parse(s: &str) -> Self {
        match s {
            "nerd" => Self::Nerd,
            "ascii" => Self::Ascii,
            _ => Self::Unicode,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Unicode => "unicode",
            Self::Nerd => "nerd",
            Self::Ascii => "ascii",
        }
    }
}

/// A named theme: tokens plus metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    pub tokens: ThemeTokens,
    pub border: ratatui::widgets::BorderType,
    pub transparent: bool,
    pub icons: IconSet,
    /// Dim everything behind a modal (Settings › Appearance).
    pub dim_modals: bool,
    /// Area chart style.
    pub graphs: GraphStyle,
    /// Vertical colour gradient on area charts.
    pub gradient: bool,
}

impl Theme {
    /// Row colours for an area chart: the gradient, or its middle colour on
    /// every row when the gradient is turned off.
    pub fn chart_rows(&self, gradient: &[Color]) -> Vec<Color> {
        if self.gradient || gradient.is_empty() {
            gradient.to_vec()
        } else {
            vec![gradient[gradient.len() / 2]; gradient.len()]
        }
    }

    /// Effective background (`Reset` when transparent).
    pub fn bg(&self) -> Color {
        if self.transparent {
            Color::Reset
        } else {
            self.tokens.bg
        }
    }

    /// Reverse lookup: token name for a colour (`bg`, `fg`, … or `?`).
    pub fn token_name(&self, color: Color) -> &'static str {
        let t = &self.tokens;
        let pairs = [
            (t.bg, "bg"),
            (t.bg2, "bg2"),
            (t.hl, "hl"),
            (t.sel, "sel"),
            (t.gutter, "gutter"),
            (t.comment, "comment"),
            (t.fg, "fg"),
            (t.fg2, "fg2"),
            (t.blue, "blue"),
            (t.cyan, "cyan"),
            (t.magenta, "magenta"),
            (t.green, "green"),
            (t.yellow, "yellow"),
            (t.orange, "orange"),
            (t.red, "red"),
            (t.teal, "teal"),
        ];
        pairs
            .into_iter()
            .find(|(c, _)| *c == color)
            .map(|(_, n)| n)
            .unwrap_or("?")
    }

    pub fn bg_hex(&self) -> String {
        color_hex(self.tokens.bg)
    }

    /// Base style of a whole area (text colour on the theme background). Use it
    /// as the style of paragraphs/blocks; spans use the fg-only helpers below so
    /// they inherit the surface they sit on (chips, bars, selected rows).
    pub fn base(&self) -> Style {
        Style::default().fg(self.tokens.fg).bg(self.bg())
    }

    /// Primary text colour (inherits the background).
    pub fn text(&self) -> Style {
        Style::default().fg(self.tokens.fg)
    }

    /// Dim text (labels, hints); inherits the background.
    pub fn dim(&self) -> Style {
        Style::default().fg(self.tokens.comment)
    }

    /// Secondary text (values); inherits the background.
    pub fn secondary(&self) -> Style {
        Style::default().fg(self.tokens.fg2)
    }

    /// Key in a hint (orange bold); inherits the background.
    pub fn key(&self) -> Style {
        Style::default()
            .fg(self.tokens.orange)
            .add_modifier(Modifier::BOLD)
    }

    pub fn accent(&self) -> Style {
        Style::default().fg(self.tokens.blue)
    }

    pub fn topbar(&self) -> Style {
        Style::default().fg(self.tokens.fg).bg(self.tokens.bg2)
    }

    pub fn panel_border(&self, focused: bool) -> Style {
        Style::default()
            .fg(if focused {
                self.tokens.blue
            } else {
                self.tokens.gutter
            })
            .bg(self.bg())
    }

    pub fn panel_title(&self, focused: bool) -> Style {
        Style::default()
            .fg(if focused {
                self.tokens.blue
            } else {
                self.tokens.fg2
            })
            .bg(self.bg())
            .add_modifier(Modifier::BOLD)
    }

    pub fn row_selected(&self, focused: bool) -> Style {
        Style::default()
            .fg(self.tokens.fg)
            .bg(if focused { self.tokens.sel } else { self.tokens.hl })
    }
}

fn color_hex(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => String::from("?"),
    }
}

fn rgb(hex: &str) -> Color {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    Color::Rgb(r, g, b)
}

/// Nearest xterm-256 index for an RGB colour (6x6x6 cube + greyscale).
pub fn xterm256(color: Color) -> u8 {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return 7,
    };
    // Greyscale ramp.
    if r == g && g == b {
        if r < 8 {
            return 16;
        }
        if r > 248 {
            return 231;
        }
        return (232.0 + ((f64::from(r) - 8.0) / 10.0).round()) as u8;
    }
    let level = |v: u8| {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else if v < 155 {
            2
        } else if v < 195 {
            3
        } else if v < 235 {
            4
        } else {
            5
        }
    };
    16 + 36 * level(r) + 6 * level(g) + level(b)
}

/// Downgrade a theme to xterm-256 indexes when truecolor is unavailable.
pub fn downgrade(mut theme: Theme) -> Theme {
    use ratatui::style::Color as C;
    let map = |c: C| match c {
        C::Rgb(_, _, _) => C::Indexed(xterm256(c)),
        other => other,
    };
    let t = &mut theme.tokens;
    t.bg = map(t.bg);
    t.bg2 = map(t.bg2);
    t.hl = map(t.hl);
    t.sel = map(t.sel);
    t.gutter = map(t.gutter);
    t.comment = map(t.comment);
    t.fg = map(t.fg);
    t.fg2 = map(t.fg2);
    t.blue = map(t.blue);
    t.cyan = map(t.cyan);
    t.magenta = map(t.magenta);
    t.green = map(t.green);
    t.yellow = map(t.yellow);
    t.orange = map(t.orange);
    t.red = map(t.red);
    t.teal = map(t.teal);
    theme
}

/// Truecolor available (`COLORTERM=truecolor|24bit`).
pub fn truecolor() -> bool {
    std::env::var("COLORTERM")
        .map(|v| {
            let v = v.to_lowercase();
            v.contains("truecolor") || v.contains("24bit")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::builtin;

    #[test]
    fn tokyo_night_matches_spec_order() {
        let t = builtin::tokyo_night();
        let hexes = t.tokens.hex_list();
        assert_eq!(
            hexes,
            [
                "#1a1b26", "#16161e", "#292e42", "#283457", "#3b4261", "#565f89", "#c0caf5", "#a9b1d6",
                "#7aa2f7", "#7dcfff", "#bb9af7", "#9ece6a", "#e0af68", "#ff9e64", "#f7768e", "#73daca",
            ]
            .map(String::from)
        );
    }
}
