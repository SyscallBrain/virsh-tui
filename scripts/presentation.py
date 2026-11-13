#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyte>=0.8"]
# ///
"""Record the presentation GIF shown at the top of the README.

The real binary runs in demo mode inside a pseudo-terminal; the script types
the keys of each scene, reads the screen back through a terminal emulator
(pyte), draws every frame as a slide (title, the keys just pressed and the
terminal window) with headless Chrome, and joins the frames into a GIF with
ffmpeg. Nothing touches libvirt, and HOME/XDG point at a temporary folder,
so your config, key map and wizard draft are left alone.

    cargo build --release
    uv run scripts/presentation.py            # writes book/src/images/demo.gif
    uv run scripts/presentation.py --frames   # also keep the PNG frames

Needs: uv (installs pyte), google-chrome or chromium, ffmpeg, gifsicle
(optional, shrinks the GIF), a monospace font with box-drawing and braille
glyphs (JetBrains Mono / DejaVu Sans Mono).
"""

from __future__ import annotations

import concurrent.futures
import fcntl
import hashlib
import html
import os
import pty
import re
import select
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
from dataclasses import dataclass, field
from pathlib import Path

import pyte

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "release" / "virsh-tui"
OUT = ROOT / "book" / "src" / "images" / "demo.gif"
WORDMARK = ROOT / "book" / "src" / "images" / "logo-wordmark.svg"
COLS, ROWS = 160, 42
CANVAS = (1280, 800)
THEME = "tokyo-night"

MONO = "'JetBrainsMono Nerd Font Mono', 'JetBrains Mono', 'DejaVu Sans Mono', monospace"
SANS = "'Inter', 'Noto Sans', 'DejaVu Sans', system-ui, sans-serif"

# ── the script ────────────────────────────────────────────────────────────────

KEYS = {
    "Enter": b"\r",
    "Esc": b"\x1b",
    "Space": b" ",
    "Tab": b"\t",
    "BS": b"\x7f",
    "C-p": b"\x10",
    "Down": b"\x1b[B",
    "Up": b"\x1b[A",
}
# How a key is drawn on its keycap.
CAP = {"Enter": "Enter", "Esc": "Esc", "Space": "Space", "Tab": "Tab", "BS": "⌫", "C-p": "Ctrl-p",
       "Down": "↓", "Up": "↑"}


@dataclass
class Step:
    keys: str = ""  # "<Enter>", "j", "<Space>sr": tokens in <>, other characters typed one by one
    hold: float = 1.0  # seconds this frame stays on screen
    caps: list[str] | None = None  # keycaps to show; default: derived from keys
    wait: float = 0.35  # real time to let the app react before reading the screen
    record: bool = True  # False: send the keys but keep no frame


@dataclass
class Scene:
    tag: str
    title: str
    subtitle: str  # may contain <kbd>…</kbd>
    steps: list[Step] = field(default_factory=list)


def key(keys: str, hold: float = 1.0, **kw) -> Step:
    return Step(keys=keys, hold=hold, **kw)


def typed(text: str, hold: float = 0.9, per: float = 0.09) -> list[Step]:
    """Type `text` one character per frame; the last frame is held."""
    steps = [Step(keys=ch, hold=per, caps=[], wait=0.12) for ch in text]
    steps[-1].hold = hold
    return steps


SCENES = [
    Scene(
        "Domains",
        "Every VM at a glance",
        "State, CPU and memory for each domain, with live graphs for the one under the cursor.",
        [
            key("", 2.2, caps=[]),
            key("j", 0.7),
            key("j", 0.7),
            key("j", 0.9),
            key("/", 0.5),
            *typed("k8s", 1.4),
            key("<Esc>", 0.4),
            key("gg", 0.5),
            key("j", 0.8),
        ],
    ),
    Scene(
        "Actions",
        "Nothing happens behind your back",
        "Marked domains (<kbd>m</kbd>) act together, and every action shows the exact <kbd>virsh</kbd> commands first.",
        [
            key("D", 3.2),
            key("n", 0.9),
        ],
    ),
    Scene(
        "Monitor",
        "Live graphs, per domain",
        "Per-vCPU meters, memory, disks and NICs, with 60 s, 5 min and 1 h history (<kbd>t</kbd>).",
        [
            key("<Enter>", 0.9),
            key("L", 0.6),
            key("", 1.6),
            key("t", 1.6),
            key("t", 1.4),
        ],
    ),
    Scene(
        "Hardware",
        "Edit hardware, review the diff",
        "Change a field and see the XML diff and the commands <kbd>:w</kbd> will run.",
        [
            key("L", 1.2),
            key("<Tab>", 0.7),
            key("i", 0.5),
            key("<BS>", 0.4),
            *typed("16", 0.8, per=0.3),
            key("<Esc>", 2.6),
        ],
    ),
    Scene(
        "Snapshots",
        "Snapshots as a tree",
        "Create, revert and delete. A revert can take a safety snapshot first.",
        [
            key("L", 1.4),
            key("k", 0.7),
            key("<Space>sr", 3.4, caps=["Space", "s", "r"]),
            key("n", 0.5),
            # Drop the hardware edit, so the view keys may leave the domain.
            key(":q!<Enter>", 0, record=False),
        ],
    ),
    Scene(
        "Host",
        "The whole host, one key each",
        "Host, networks with DHCP leases, storage pools and volumes, and the libvirt event stream.",
        [
            key("<BS>", 0.4),
            key("2", 1.7),
            key("3", 1.7),
            key("4", 1.7),
            key("5", 1.3),
            key("1", 0.6),
        ],
    ),
    Scene(
        "Commands",
        "Every command, one key away",
        "<kbd>Space</kbd> lists what can follow, <kbd>Ctrl-p</kbd> finds any action, <kbd>:</kbd> completes commands and domains.",
        [
            key("<Space>", 2.0, wait=0.6),
            key("<Esc>", 0.3),
            key("<C-p>", 2.4),
            key("<Esc>", 0.3),
            key(":", 0.4),
            *typed("start ", 0.4),
            key("<Tab>", 1.4),
            key("<Down>", 0.6),
            key("<Down>", 1.0),
            key("<Esc>", 0.2),
            key("<Esc>", 0.4),
        ],
    ),
    Scene(
        "Wizard",
        "New domains in six steps",
        "The wizard builds the <kbd>virt-install</kbd> command as you type, ready to copy.",
        [
            key("<Space>n", 1.2, caps=["Space", "n"]),
            *typed("web-01", 0.8),
            key("<Tab>", 0.4),
            *typed("debian12", 2.0),
            key("<Esc>", 0.5),
        ],
    ),
    Scene(
        "Themes",
        "Make it yours",
        "Nine built-in themes, your own themes, remappable keys and a settings screen (<kbd>,</kbd>).",
        [
            key(":", 0.3),
            *typed("theme ", 0.3),
            key("<Tab>", 1.4),
            key("<Enter>", 1.6),
            key(":", 0.2),
            *typed("theme gruvbox-dark", 0.3, per=0.04),
            key("<Enter>", 1.6),
            key(":", 0.2),
            *typed("theme nord", 0.3, per=0.05),
            key("<Enter>", 1.8),
        ],
    ),
]

# ── terminal driving ──────────────────────────────────────────────────────────

Cell = tuple[str, str, str, bool]  # char, fg, bg, bold
Screen = list[list[Cell]]


class Terminal:
    def __init__(self, home: Path):
        config = home / "config.toml"
        config.write_text("")
        env = dict(os.environ)
        env.update(
            TERM="xterm-256color",
            COLORTERM="truecolor",
            HOME=str(home),
            XDG_CONFIG_HOME=str(home / "config"),
            XDG_STATE_HOME=str(home / "state"),
            XDG_DATA_HOME=str(home / "data"),
            XDG_CACHE_HOME=str(home / "cache"),
        )
        args = [str(BIN), "--demo", "--config", str(config), "--theme", THEME]
        self.pid, self.fd = pty.fork()
        if self.pid == 0:  # child
            os.execve(args[0], args, env)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        self.screen = pyte.Screen(COLS, ROWS)
        self.stream = pyte.ByteStream(self.screen)
        self.pump(1.5)

    def pump(self, seconds: float) -> None:
        end = time.monotonic() + seconds
        while (left := end - time.monotonic()) > 0:
            ready, _, _ = select.select([self.fd], [], [], min(left, 0.05))
            if not ready:
                continue
            try:
                data = os.read(self.fd, 1 << 16)
            except OSError:
                return
            self.stream.feed(data)
            # crossterm asks for the cursor position at start-up.
            if b"\x1b[6n" in data:
                os.write(self.fd, b"\x1b[1;1R")

    def send(self, token: str) -> None:
        data = KEYS[token[1:-1]] if token.startswith("<") and len(token) > 1 else token.encode()
        os.write(self.fd, data)

    def snapshot(self) -> Screen:
        rows = []
        for y in range(ROWS):
            line = self.screen.buffer[y]
            row = []
            for x in range(COLS):
                c = line[x]
                fg, bg = c.fg, c.bg
                if c.reverse:
                    fg, bg = bg, fg
                row.append((c.data, color(fg), color(bg), bool(c.bold)))
            rows.append(row)
        return rows

    def close(self) -> None:
        try:
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
        except OSError:
            pass


def color(value: str) -> str:
    if value == "default":
        return ""
    if re.fullmatch(r"[0-9a-fA-F]{6}", value):
        return "#" + value.lower()
    return value  # a named ANSI colour; CSS understands most of them


def tokens(keys: str) -> list[str]:
    return re.findall(r"<[^>]+>|.", keys, re.S)


def caps_for(keys: str) -> list[str]:
    out = []
    for t in tokens(keys):
        out.append(CAP.get(t[1:-1], t[1:-1]) if t.startswith("<") and len(t) > 1 else t)
    return out


@dataclass
class Frame:
    scene: int  # -1: title card, -2: closing card
    caps: list[str]
    screen: Screen | None
    hold: float


def record() -> list[Frame]:
    frames: list[Frame] = []
    with tempfile.TemporaryDirectory(prefix="virsh-tui-presentation-") as home:
        term = Terminal(Path(home))
        try:
            for index, scene in enumerate(SCENES):
                print(f"  recording {scene.tag}…", file=sys.stderr)
                caps: list[str] = []
                for step in scene.steps:
                    for t in tokens(step.keys):
                        term.send(t)
                        term.pump(0.04)
                    term.pump(step.wait)
                    if not step.record:
                        continue
                    if step.caps is not None:
                        caps = step.caps
                    elif step.keys:
                        caps = caps_for(step.keys)
                    frames.append(Frame(index, list(caps), term.snapshot(), step.hold))
        finally:
            term.close()
    return frames


# ── drawing ───────────────────────────────────────────────────────────────────


def glyph(ch: str, wide: bool) -> str:
    if ch == "▄":
        return '<i class="lo"> </i>'
    if ch == "▀":
        return '<i class="hi"> </i>'
    cls = ["w"] if wide else []
    if 0x2500 <= ord(ch) <= 0x257F:
        cls.append("b")
    attr = f' class="{" ".join(cls)}"' if cls else ""
    return f"<i{attr}>{html.escape(ch)}</i>"


def screen_html(screen: Screen) -> tuple[str, str]:
    """Return (rows html, the theme background)."""
    counts: dict[str, int] = {}
    for row in screen:
        for _, _, bg, _ in row:
            if bg:
                counts[bg] = counts.get(bg, 0) + 1
    theme_bg = max(counts, key=counts.get) if counts else "#1a1b26"
    out = []
    for row in screen:
        parts = []
        run_style = None
        run: list[str] = []
        x = 0
        while x < len(row):
            ch, fg, bg, bold = row[x]
            wide = x + 1 < len(row) and row[x + 1][0] == ""
            style = (fg, bg, bold)
            piece = html.escape(ch) if ch and ord(ch[0]) < 128 else glyph(ch or " ", wide)
            if style != run_style and run:
                parts.append(span(run_style, run))
                run = []
            run_style = style
            run.append(piece)
            x += 2 if wide else 1
        if run:
            parts.append(span(run_style, run))
        out.append(f'<div class="r">{"".join(parts)}</div>')
    return "".join(out), theme_bg


def span(style, run: list[str]) -> str:
    fg, bg, bold = style
    css = []
    if fg:
        css.append(f"color:{fg}")
    if bg:
        css.append(f"background:{bg}")
    if bold:
        css.append("font-weight:700")
    return f'<span style="{";".join(css)}">{"".join(run)}</span>'


STYLE = f"""
  * {{ box-sizing: border-box; }}
  html, body {{ margin: 0; width: {CANVAS[0]}px; height: {CANVAS[1]}px; overflow: hidden; }}
  body {{ background: radial-gradient(1200px 700px at 30% 0%, #1f2335 0%, #16161e 60%, #111118 100%);
         font-family: {SANS}; color: #c0caf5; }}
  kbd {{ font-family: {MONO}; font-size: 0.92em; color: #c0caf5; background: #292e42;
         border: 1px solid #3b4261; border-radius: 4px; padding: 0 5px; }}
  .head {{ position: absolute; left: 52px; right: 52px; top: 26px; height: 92px; }}
  .tag {{ font: 600 11px/1 {MONO}; letter-spacing: 0.18em; text-transform: uppercase; color: #7aa2f7; }}
  .tag::before {{ content: ""; display: inline-block; width: 18px; height: 1px; background: #7aa2f7;
                  vertical-align: middle; margin-right: 8px; }}
  .tag b {{ color: #565f89; font-weight: 600; margin-left: 10px; }}
  h1 {{ margin: 12px 0 6px; font-size: 26px; font-weight: 700; color: #e6e9f8; letter-spacing: -0.01em; }}
  p.sub {{ margin: 0; font-size: 14px; color: #8b93b8; }}
  .caps {{ position: absolute; right: 0; top: 18px; display: flex; gap: 8px; }}
  .cap {{ min-width: 34px; height: 34px; padding: 0 10px; display: flex; align-items: center;
          justify-content: center; font: 600 14px/1 {MONO}; color: #e6e9f8; background: #24283b;
          border: 1px solid #3b4261; border-bottom-width: 3px; border-radius: 7px; }}
  .win {{ position: absolute; left: 52px; top: 132px; border-radius: 10px; overflow: hidden;
          box-shadow: 0 22px 60px rgba(0,0,0,.55), 0 0 0 1px rgba(122,162,247,.14); }}
  .bar {{ height: 26px; display: flex; align-items: center; padding: 0 12px; gap: 7px;
          background: #101016; font: 11px/1 {MONO}; color: #565f89; position: relative; }}
  .bar i {{ width: 11px; height: 11px; border-radius: 50%; display: inline-block; }}
  .bar span {{ position: absolute; left: 0; right: 0; text-align: center; }}
  .term {{ padding: 8px 12px 10px; font: 12px/14px {MONO}; font-variant-ligatures: none; color: #c0caf5; }}
  .r {{ height: 14px; white-space: pre; }}
  .r span {{ display: inline-block; height: 14px; vertical-align: top; }}
  .r i {{ display: inline-block; width: 1ch; font-style: normal; text-align: center; overflow: visible; }}
  .r i.w {{ width: 2ch; }}
  .r i.b {{ transform: scaleY(1.2); }}
  .r i.lo {{ background: linear-gradient(transparent 50%, currentColor 50%); height: 14px; vertical-align: top; }}
  .r i.hi {{ background: linear-gradient(currentColor 50%, transparent 50%); height: 14px; vertical-align: top; }}
  .card {{ position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center;
           justify-content: center; text-align: center; }}
  .card .tagline {{ margin-top: 26px; font-size: 22px; color: #a9b1d6; }}
  .card .tagline b {{ color: #e6e9f8; }}
  .card .small {{ margin-top: 12px; font-size: 14px; color: #565f89; }}
  .code {{ margin-top: 34px; text-align: left; background: #13131a; border: 1px solid #292e42;
           border-radius: 10px; padding: 18px 26px; font: 14px/1.75 {MONO}; color: #c0caf5; }}
  .code .c {{ color: #565f89; }}
  .code .p {{ color: #9ece6a; }}
  .links {{ margin-top: 30px; font: 14px/1 {MONO}; color: #7aa2f7; }}
  .links span {{ color: #3b4261; margin: 0 14px; }}
  .fine {{ margin-top: 14px; font-size: 12px; color: #565f89; }}
"""


def page(body: str) -> str:
    return f'<!doctype html><meta charset="utf-8"><style>{STYLE}</style>{body}'


def scene_page(frame: Frame) -> str:
    scene = SCENES[frame.scene]
    rows, bg = screen_html(frame.screen)
    caps = "".join(f'<div class="cap">{html.escape(c)}</div>' for c in frame.caps)
    return page(
        f"""
<div class="head">
  <div class="tag">{html.escape(scene.tag)}<b>{frame.scene + 1:02d} / {len(SCENES):02d}</b></div>
  <h1>{html.escape(scene.title)}</h1>
  <p class="sub">{scene.subtitle}</p>
  <div class="caps">{caps}</div>
</div>
<div class="win" style="background:{bg}">
  <div class="bar"><i style="background:#f7768e"></i><i style="background:#e0af68"></i><i style="background:#9ece6a"></i>
    <span>virsh-tui --demo</span></div>
  <div class="term">{rows}</div>
</div>"""
    )


def wordmark(width: int) -> str:
    svg = WORDMARK.read_text()
    svg = re.sub(r'width="[\d.]+" height="[\d.]+"', f'width="{width}"', svg, count=1)
    return svg


def title_page() -> str:
    return page(
        f"""
<div class="card">
  {wordmark(560)}
  <div class="tagline">Your <b>libvirt</b> machines, in the terminal.</div>
  <div class="small">vim keys · live graphs · every action is a virsh command</div>
</div>"""
    )


def closing_page() -> str:
    return page(
        f"""
<div class="card">
  {wordmark(420)}
  <div class="tagline">A keyboard-driven terminal UI for libvirt. Free and open source.</div>
  <div class="code">
    <div class="c"># needs Rust 1.88+ and the libvirt client tools (virsh)</div>
    <div><span class="p">$</span> cargo install --locked --git https://github.com/SyscallBrain/virsh-tui</div>
    <div class="c"># look around first: example data, nothing runs</div>
    <div><span class="p">$</span> virsh-tui --demo</div>
  </div>
  <div class="links">github.com/SyscallBrain/virsh-tui<span>·</span>syscallbrain.github.io/virsh-tui</div>
  <div class="fine">MIT licence · Linux · qemu:///system, qemu:///session and qemu+ssh://</div>
</div>"""
    )


def chrome() -> str:
    for name in ("google-chrome", "google-chrome-stable", "chromium", "chromium-browser"):
        if shutil.which(name):
            return name
    sys.exit("headless Chrome/Chromium not found")


def shoot(browser: str, html_text: str, out: Path) -> None:
    src = out.with_suffix(".html")
    src.write_text(html_text)
    profile = out.with_suffix(".profile")
    subprocess.run(
        [
            browser,
            "--headless=new",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=1",
            f"--user-data-dir={profile}",
            f"--window-size={CANVAS[0]},{CANVAS[1]}",
            f"--screenshot={out}",
            src.as_uri(),
        ],
        check=True,
        capture_output=True,
    )
    shutil.rmtree(profile, ignore_errors=True)


# ── assembly ──────────────────────────────────────────────────────────────────


def main() -> None:
    keep = "--frames" in sys.argv
    if not BIN.exists():
        sys.exit(f"{BIN} not found: run `cargo build --release` first")
    for tool in ("ffmpeg",):
        if not shutil.which(tool):
            sys.exit(f"{tool} not found")
    browser = chrome()

    print("recording the demo…", file=sys.stderr)
    frames = record()
    timeline: list[tuple[str, float]] = [("title", 3.0)]
    pages: dict[str, str] = {"title": title_page(), "closing": closing_page()}
    for frame in frames:
        text = scene_page(frame)
        digest = hashlib.sha1(text.encode()).hexdigest()[:16]
        pages.setdefault(digest, text)
        # Merge identical consecutive frames.
        if timeline and timeline[-1][0] == digest:
            timeline[-1] = (digest, timeline[-1][1] + frame.hold)
        else:
            timeline.append((digest, frame.hold))
    timeline.append(("closing", 5.0))

    work = Path(tempfile.mkdtemp(prefix="virsh-tui-frames-"))
    print(f"drawing {len(pages)} frames in {work}…", file=sys.stderr)
    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as pool:
        list(pool.map(lambda kv: shoot(browser, kv[1], work / f"{kv[0]}.png"), pages.items()))

    concat = work / "frames.txt"
    lines = []
    for name, hold in timeline:
        lines.append(f"file '{work / (name + '.png')}'\nduration {hold:.3f}")
    lines.append(f"file '{work / (timeline[-1][0] + '.png')}'")  # ffmpeg ignores the last duration
    concat.write_text("\n".join(lines) + "\n")
    total = sum(h for _, h in timeline)
    print(f"encoding {total:.1f} s of GIF…", file=sys.stderr)

    raw = work / "demo.gif"
    subprocess.run(
        [
            "ffmpeg", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", str(concat),
            "-vf", "fps=10,split[a][b];[a]palettegen=max_colors=256:stats_mode=full[p];"
                   "[b][p]paletteuse=dither=none:diff_mode=rectangle",
            "-loop", "0", str(raw),
        ],
        check=True,
    )
    OUT.parent.mkdir(parents=True, exist_ok=True)
    if shutil.which("gifsicle"):
        subprocess.run(["gifsicle", "-O3", "--no-comments", str(raw), "-o", str(OUT)], check=True)
    else:
        shutil.copy(raw, OUT)
    size = OUT.stat().st_size / 1e6
    print(f"{OUT.relative_to(ROOT)}  {total:.1f} s  {size:.1f} MB", file=sys.stderr)
    if keep:
        print(f"frames kept in {work}", file=sys.stderr)
    else:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
