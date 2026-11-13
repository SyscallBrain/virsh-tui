#!/usr/bin/env python3
"""Generates the virsh-tui logo files.

The mark is a rack of three VM blades. Each blade has a status LED in the
colours the app uses for domain states (green running, yellow paused, grey
shut off); the top blade is the active one and shows a prompt and a cursor.
The wordmark is drawn from a 5x7 pixel font as rectangles, so the SVGs need
no font. Colours come from the tokyo-night theme (docs/DESIGN.md §1).

Writes book/src/images/logo*.svg and book/theme/favicon.{svg,png} (the PNG
needs rsvg-convert).

    python3 scripts/make-logo.py
"""
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "book" / "src" / "images"
THEME = ROOT / "book" / "theme"

BG = "#1a1b26"      # bg
PANEL = "#24283b"   # bg.float
GUTTER = "#3b4261"  # gutter
COMMENT = "#565f89" # comment / shut off
FG = "#c0caf5"      # fg
BLUE = "#7aa2f7"    # accent
CYAN = "#7dcfff"    # cursor
GREEN = "#9ece6a"   # running
YELLOW = "#e0af68"  # paused

NAME = "virsh-tui"

# 5x7 glyphs, '#' = pixel. Every glyph is 5 wide so the spacing is even.
GLYPHS = {
    "v": [".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
    "i": ["..#..", ".....", ".##..", "..#..", "..#..", "..#..", ".###."],
    "r": [".....", ".....", "#.##.", "##..#", "#....", "#....", "#...."],
    "s": [".....", ".....", ".####", "#....", ".###.", "....#", "####."],
    "h": ["#....", "#....", "#.##.", "##..#", "#...#", "#...#", "#...#"],
    "-": [".....", ".....", ".....", "#####", ".....", ".....", "....."],
    "t": [".#...", ".#...", "####.", ".#...", ".#...", ".#..#", "..##."],
    "u": [".....", ".....", "#...#", "#...#", "#...#", "#..##", ".##.#"],
}


def mark(x: float, y: float, s: float) -> str:
    """The rack mark in a s×s box at (x, y)."""
    u = s / 64  # design grid: 64 units

    def r(v: float) -> float:
        return round(v * u, 3)

    def blade(top: float, led: str, active: bool) -> str:
        stroke = BLUE if active else GUTTER
        out = (
            f'    <rect x="{r(6)}" y="{r(top)}" width="{r(52)}" height="{r(15)}" rx="{r(4)}" '
            f'fill="{PANEL}" stroke="{stroke}" stroke-width="{r(2.6)}"/>\n'
            f'    <circle cx="{r(14)}" cy="{r(top + 7.5)}" r="{r(2.6)}" fill="{led}"/>\n'
        )
        if active:
            out += (
                f'    <path d="M{r(22)},{r(top + 4)} L{r(27)},{r(top + 7.5)} L{r(22)},{r(top + 11)}" '
                f'fill="none" stroke="{GREEN}" stroke-width="{r(2.6)}" '
                f'stroke-linecap="round" stroke-linejoin="round"/>\n'
                f'    <rect x="{r(31)}" y="{r(top + 9)}" width="{r(9)}" height="{r(3)}" '
                f'rx="{r(1)}" fill="{CYAN}"/>\n'
            )
        else:
            out += (
                f'    <rect x="{r(22)}" y="{r(top + 6.2)}" width="{r(28)}" height="{r(2.6)}" '
                f'rx="{r(1.3)}" fill="{GUTTER}"/>\n'
            )
        return out

    return (
        f'  <g transform="translate({x},{y})">\n'
        "    <!-- running blade: prompt and cursor -->\n"
        + blade(6, GREEN, True)
        + "    <!-- paused blade -->\n"
        + blade(25, YELLOW, False)
        + "    <!-- shut off blade -->\n"
        + blade(44, COMMENT, False)
        + "  </g>"
    )


def wordmark(x: float, y: float, px: float, text_colour: str) -> tuple[str, float]:
    """'virsh-tui' as pixel rectangles: 'virsh' in text_colour, a dim
    hyphen, 'tui' in blue. Returns (svg, width)."""
    groups: dict[str, list[str]] = {}
    cursor = x
    for idx, ch in enumerate(NAME):
        colour = COMMENT if ch == "-" else (BLUE if idx > NAME.index("-") else text_colour)
        for j, row in enumerate(GLYPHS[ch]):
            for i, cell in enumerate(row):
                if cell == "#":
                    groups.setdefault(colour, []).append(
                        f'<rect x="{cursor + i * px:.2f}" y="{y + j * px:.2f}" '
                        f'width="{px:.2f}" height="{px:.2f}"/>'
                    )
        cursor += 6 * px
    svg = "\n".join(
        f'  <g fill="{c}" shape-rendering="crispEdges">\n    ' + "\n    ".join(rects) + "\n  </g>" for c, rects in groups.items()
    )
    return svg, cursor - x - px


def write(path: Path, width: float, height: float, body: str, background: bool) -> None:
    bg = (
        f'  <rect width="{width:.2f}" height="{height:.2f}" rx="{round(height * 0.22, 2)}" fill="{BG}"/>\n'
        if background
        else ""
    )
    path.write_text(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0f}" height="{height:.0f}" '
        f'viewBox="0 0 {width:.2f} {height:.2f}" role="img" aria-label="{NAME}">\n'
        f"  <title>{NAME}</title>\n{bg}{body}\n</svg>\n"
    )


def lockup(text_colour: str) -> tuple[str, float, float]:
    px, height, mark_size, gap = 9, 120, 96, 28
    text, text_width = wordmark(0, 0, px, text_colour)
    tx, ty = 24 + mark_size + gap, (height - 7 * px) / 2
    body = mark(24, 12, mark_size) + f'\n  <g transform="translate({tx},{ty})">\n{text}\n  </g>'
    return body, tx + text_width + 32, height


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    THEME.mkdir(parents=True, exist_ok=True)
    # App icon: the mark on a rounded dark tile.
    write(OUT / "logo.svg", 256, 256, mark(32, 32, 192), background=True)
    # Mark only, transparent, for small sizes.
    write(OUT / "logo-mark.svg", 64, 64, mark(0, 0, 64), background=False)
    # Wordmark for dark backgrounds (README banner).
    body, w, h = lockup(FG)
    write(OUT / "logo-wordmark.svg", w, h, body, background=True)
    # Same wordmark, transparent with dark text, for light pages.
    body, w, h = lockup(BG)
    write(OUT / "logo-wordmark-light.svg", w, h, body, background=False)
    # Favicon for the book: the tile, so it reads on light and dark tabs.
    write(THEME / "favicon.svg", 64, 64, mark(4, 4, 56), background=True)
    if shutil.which("rsvg-convert"):
        subprocess.run(
            ["rsvg-convert", "-w", "64", "-h", "64", str(THEME / "favicon.svg"),
             "-o", str(THEME / "favicon.png")],
            check=True,
        )


if __name__ == "__main__":
    main()
