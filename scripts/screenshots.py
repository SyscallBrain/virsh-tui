#!/usr/bin/env python3
"""Regenerate the PNG screenshots used by the README and the book.

Each shot renders one frame of the demo backend with
`virsh-tui --demo --frozen --dump`, turns the ANSI output into HTML and
photographs it with headless Chrome. Nothing touches libvirt.

    cargo build --release
    python3 scripts/screenshots.py            # all shots
    python3 scripts/screenshots.py dashboard  # only the named ones

Needs: google-chrome (or chromium), ImageMagick (`magick`), a monospace font
with box-drawing and braille glyphs (Fira Code / JetBrains Mono work).
"""

import html
from collections import Counter
from itertools import groupby
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "release" / "virsh-tui"
OUT = ROOT / "book" / "src" / "images"
SIZE = "174x43"

# name -> (screen, theme)
SHOTS = {
    "dashboard": ("dashboard", "tokyo-night"),
    "monitor": ("monitor", "tokyo-night"),
    "hardware": ("hardware", "tokyo-night"),
    "snapshots": ("snapshots", "tokyo-night"),
    "host": ("host", "tokyo-night"),
    "networks": ("networks", "tokyo-night"),
    "storage": ("storage", "tokyo-night"),
    "events": ("events", "tokyo-night"),
    "wizard": ("wizard", "tokyo-night"),
    "palette": ("palette", "tokyo-night"),
    "help": ("help", "tokyo-night"),
    "settings": ("settings", "tokyo-night"),
    "theme-catppuccin-mocha": ("dashboard", "catppuccin-mocha"),
    "theme-gruvbox-dark": ("host", "gruvbox-dark"),
    "theme-nord": ("networks", "nord"),
    "theme-dracula": ("monitor", "dracula"),
    "theme-tokyo-night-day": ("dashboard", "tokyo-night-day"),
}

SGR = re.compile(r"\x1b\[([0-9;]*)m")
FONT = "'Fira Code', 'JetBrains Mono', 'DejaVu Sans Mono', monospace"
CHROMA = "#ff00ff"
ROW = 16  # px per terminal row  # page background, trimmed away afterwards


def parse(text: str) -> list[list[tuple[str, tuple]]]:
    """ANSI frame -> rows of (char, (fg, bg, bold)) cells."""
    fg = bg = None
    bold = False
    rows = []
    for line in text.splitlines():
        row = []
        pos = 0
        for m in list(SGR.finditer(line)) + [None]:
            chunk = line[pos : m.start()] if m else line[pos:]
            row.extend((ch, (fg, bg, bold)) for ch in chunk)
            if m is None:
                break
            pos = m.end()
            codes = [int(c) for c in m.group(1).split(";") if c] or [0]
            i = 0
            while i < len(codes):
                c = codes[i]
                if c == 0:
                    fg = bg = None
                    bold = False
                elif c == 1:
                    bold = True
                elif c == 22:
                    bold = False
                elif c == 39:
                    fg = None
                elif c == 49:
                    bg = None
                elif c in (38, 48) and i + 4 < len(codes) and codes[i + 1] == 2:
                    rgb = "#%02x%02x%02x" % tuple(codes[i + 2 : i + 5])
                    if c == 38:
                        fg = rgb
                    else:
                        bg = rgb
                    i += 4
                i += 1
        rows.append(row)
    return rows


def ansi_to_html(text: str) -> tuple[str, str]:
    """Return (html body, theme background).

    Every row has a fixed height and every non-ASCII glyph a fixed 1ch box, so
    fallback fonts (braille, symbols) cannot push the grid out of line.
    """
    rows = parse(text)
    bgs = Counter(style[1] for row in rows for _, style in row if style[1])
    theme_bg = bgs.most_common(1)[0][0] if bgs else "#1a1b26"
    out = []
    for row in rows:
        parts = []
        for style, cells in groupby(row, key=lambda c: c[1]):
            chars = "".join(
                html.escape(ch) if ord(ch) < 128 else glyph(ch) for ch, _ in cells
            )
            parts.append(f'<span style="{css(style)}">{chars}</span>')
        out.append(f'<div class="r">{"".join(parts)}</div>')
    return "".join(out), theme_bg


def glyph(ch: str) -> str:
    # Box-drawing glyphs are stretched to the full row so borders join up.
    cls = ' class="b"' if 0x2500 <= ord(ch) <= 0x257F else ""
    return f"<i{cls}>{html.escape(ch)}</i>"


def css(style) -> str:
    fg, bg, bold = style
    out = []
    if fg:
        out.append(f"color:{fg}")
    if bg:
        out.append(f"background:{bg}")
    if bold:
        out.append("font-weight:600")
    return ";".join(out)


def page(body: str, bg: str) -> str:
    return f"""<!doctype html><meta charset="utf-8">
<style>
  html, body {{ margin: 0; background: {CHROMA}; }}
  .term {{ display: inline-block; margin: 24px; background: {bg}; padding: 14px 16px;
           font: 14px/{ROW}px {FONT}; font-variant-ligatures: none; color: #c0caf5; }}
  .r {{ height: {ROW}px; white-space: pre; }}
  .r span {{ display: inline-block; height: {ROW}px; vertical-align: top; }}
  .r i {{ display: inline-block; width: 1ch; font-style: normal; text-align: center;
          overflow: visible; }}
  .r i.b {{ transform: scaleY(1.3); }}
</style>
<div class="term">{body}</div>"""


def chrome() -> str:
    for name in ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser"):
        if shutil.which(name):
            return name
    sys.exit("headless Chrome/Chromium not found")


def shoot(name: str, screen: str, theme: str, tmp: Path, browser: str) -> Path:
    ans = tmp / f"{name}.ans"
    # An empty config keeps the shots independent of the local config.toml.
    config = tmp / "config.toml"
    config.write_text("")
    args = [str(BIN), "--demo", "--frozen", "--config", str(config), "--theme", theme]
    args += ["--dump", str(ans), "--size", SIZE]
    if screen != "dashboard":
        args += ["--screen", screen]
    subprocess.run(args, check=True)
    body, bg = ansi_to_html(ans.read_text())
    page_file = tmp / f"{name}.html"
    page_file.write_text(page(body, bg))
    raw = tmp / f"{name}.raw.png"
    subprocess.run(
        [
            browser,
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=1.5",
            "--window-size=1700,1000",
            f"--screenshot={raw}",
            page_file.as_uri(),
        ],
        check=True,
        capture_output=True,
    )
    out = OUT / f"{name}.png"
    subprocess.run(
        ["magick", str(raw), "-fuzz", "2%", "-trim", "+repage", "-strip", str(out)],
        check=True,
    )
    return out


def main() -> None:
    if not BIN.exists():
        sys.exit(f"{BIN} not found: run `cargo build --release` first")
    wanted = sys.argv[1:] or list(SHOTS)
    OUT.mkdir(parents=True, exist_ok=True)
    browser = chrome()
    with tempfile.TemporaryDirectory() as d:
        for name in wanted:
            screen, theme = SHOTS[name]
            print(shoot(name, screen, theme, Path(d), browser).relative_to(ROOT))


if __name__ == "__main__":
    main()
