//! Chart maths ported 1:1 from docs/mockups/Main.dc.html.
//!
//! Functions: rng, series, sample, braille, spark_rows, hbar, lg, lvl, grad.

/// Braille rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrailleMode {
    Line,
    Fill,
    Down,
}

/// Xorshift32 RNG returning floats in [0, 1).
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        let s = if seed == 0 { 1 } else { seed };
        Self(s)
    }

    pub fn next_f64(&mut self) -> f64 {
        let mut s = self.0;
        s ^= s.wrapping_shl(13);
        s ^= s.wrapping_shr(17);
        s ^= s.wrapping_shl(5);
        self.0 = s;
        f64::from(s) / 4294967296.0
    }
}

/// Deterministic random-walk series.
pub fn series(seed: u32, n: usize, base: f64, amp: f64, pull: f64) -> Vec<f64> {
    let pull = if pull == 0.0 { 0.15 } else { pull };
    let mut r = Rng::new(seed);
    let mut v = base;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        v += (r.next_f64() - 0.5) * amp;
        v += (base - v) * pull;
        v = v.clamp(0.02, 0.98);
        out.push(v);
    }
    out
}

/// Linear interpolation sampler.
pub fn sample(d: &[f64], x: usize, w: usize) -> f64 {
    if d.is_empty() {
        return 0.0;
    }
    if w <= 1 {
        return d[0];
    }
    let t = x as f64 * (d.len() - 1) as f64 / (w - 1) as f64;
    let i = t.floor() as usize;
    let f = t - i as f64;
    let a = d[i.min(d.len() - 1)];
    let b = d[(i + 1).min(d.len() - 1)];
    a * (1.0 - f) + b * f
}

/// Braille area chart: returns h rows of w characters each.
#[allow(clippy::needless_range_loop)]
pub fn braille(d: &[f64], w: usize, h: usize, mode: BrailleMode) -> Vec<String> {
    let gw = w * 2;
    let gh = h * 4;
    let mut g = vec![vec![false; gw]; gh];
    let mut py: Option<usize> = None;
    for x in 0..gw {
        let v = sample(d, x, gw);
        match mode {
            BrailleMode::Down => {
                let y = (v * (gh - 1) as f64).round() as usize;
                for yy in 0..=y.min(gh - 1) {
                    g[yy][x] = true;
                }
            }
            BrailleMode::Fill => {
                let y = ((1.0 - v) * (gh - 1) as f64).round() as usize;
                for yy in y.min(gh)..gh {
                    g[yy][x] = true;
                }
            }
            BrailleMode::Line => {
                let y = ((1.0 - v) * (gh - 1) as f64).round() as usize;
                let prev = py.unwrap_or(y);
                py = Some(y);
                for yy in prev.min(y)..=prev.max(y).min(gh - 1) {
                    g[yy][x] = true;
                }
            }
        }
    }
    const BITS: [[u32; 2]; 4] = [[0x1, 0x8], [0x2, 0x10], [0x4, 0x20], [0x40, 0x80]];
    let mut rows = Vec::with_capacity(h);
    for r in 0..h {
        let mut s = String::with_capacity(w);
        for c in 0..w {
            let mut code = 0u32;
            for dy in 0..4 {
                for dx in 0..2 {
                    if g[r * 4 + dy][c * 2 + dx] {
                        code |= BITS[dy][dx];
                    }
                }
            }
            s.push(char::from_u32(0x2800 + code).unwrap_or(' '));
        }
        rows.push(s);
    }
    rows
}

/// Area chart in the configured style (braille, block characters, or plain
/// ASCII for terminals without good Unicode fonts).
pub fn area(
    style: crate::theme::GraphStyle,
    d: &[f64],
    w: usize,
    h: usize,
    mode: BrailleMode,
) -> Vec<String> {
    use crate::theme::GraphStyle as G;
    match (style, mode) {
        (G::Braille, _) | (_, BrailleMode::Line) => braille(d, w, h, mode),
        (G::Block, BrailleMode::Fill) => spark_rows(d, w, h),
        (G::Block, BrailleMode::Down) => (0..h)
            .map(|r| {
                (0..w)
                    .map(|x| {
                        let eighths = (sample(d, x, w) * h as f64 * 8.0).round() as i32 - r as i32 * 8;
                        match eighths {
                            e if e >= 8 => '█',
                            e if e >= 4 => '▀',
                            e if e >= 1 => '▔',
                            _ => ' ',
                        }
                    })
                    .collect()
            })
            .collect(),
        (G::Tty, _) => (0..h)
            .map(|r| {
                (0..w)
                    .map(|x| {
                        let filled = sample(d, x, w) * h as f64;
                        let row = if mode == BrailleMode::Down {
                            r as f64
                        } else {
                            (h - 1 - r) as f64
                        };
                        if filled >= row + 0.5 { '#' } else { ' ' }
                    })
                    .collect()
            })
            .collect(),
    }
}

const BL: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Multi-row sparkline.
pub fn spark_rows(d: &[f64], w: usize, h: usize) -> Vec<String> {
    let mut rows = Vec::with_capacity(h);
    for r in 0..h {
        let mut s = String::with_capacity(w);
        for x in 0..w {
            let lv = (sample(d, x, w) * h as f64 * 8.0).round() as i32 - (h - 1 - r) as i32 * 8;
            if lv <= 0 {
                s.push(' ');
            } else {
                let idx = (lv - 1).min(7) as usize;
                s.push(BL[idx]);
            }
        }
        rows.push(s);
    }
    rows
}

/// Single-row sparkline.
pub fn spark(d: &[f64], w: usize) -> String {
    spark_rows(d, w, 1).into_iter().next().unwrap_or_default()
}

/// Eighth-block bar: returns (fill, track).
pub fn hbar(v: f64, w: usize) -> (String, String) {
    const PARTS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let tot = (v * w as f64 * 8.0).round() as usize;
    let full = tot / 8;
    let rem = tot % 8;
    let mut fill = "█".repeat(full);
    if rem > 0 {
        fill.push(PARTS[rem - 1]);
    }
    let fill_chars = fill.chars().count();
    let rest = "░".repeat(w.saturating_sub(fill_chars));
    (fill, rest)
}

/// Line gauge: returns (on, off) runs of `━`.
pub fn lg(v: f64, w: usize) -> (String, String) {
    let n = (v * w as f64).round() as usize;
    let n = n.min(w);
    ("━".repeat(n), "━".repeat(w - n))
}

/// Level colour token name.
pub fn lvl(v: f64) -> &'static str {
    if v < 0.5 {
        "green"
    } else if v < 0.8 {
        "yellow"
    } else {
        "red"
    }
}

/// Gradient: assign each row a colour from top to bottom.
pub fn grad(rows: Vec<String>, colors: &[&str]) -> Vec<(String, String)> {
    let n = rows.len().max(1);
    rows.into_iter()
        .enumerate()
        .map(|(i, t)| {
            let idx = (i * colors.len() / n).min(colors.len() - 1);
            (t, colors[idx].to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{BrailleMode, braille, hbar, lg, series, spark_rows};

    fn load(name: &str) -> serde_json::Value {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/charts")
            .join(format!("{name}.json"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn braille_fill_matches_fixture() {
        let data = series(7, 90, 0.42, 0.22, 0.1);
        let got = braille(&data, 60, 8, BrailleMode::Fill);
        assert_eq!(serde_json::to_value(&got).unwrap(), load("braille_fill_7_60x8"));
    }

    #[test]
    fn spark_mem_matches_fixture() {
        let data = series(11, 60, 0.74, 0.06, 0.2);
        assert_eq!(
            serde_json::to_value(spark_rows(&data, 28, 3)).unwrap(),
            load("spark_mem_11_28x3")
        );
    }

    #[test]
    fn hbar_lg_match_fixtures() {
        let (fill, rest) = hbar(0.42, 16);
        assert_eq!(
            serde_json::json!({"fill": fill, "rest": rest}),
            load("hbar_042_16")
        );
        let (on, off) = lg(0.38, 22);
        assert_eq!(serde_json::json!({"on": on, "off": off}), load("lg_038_22"));
    }
}
