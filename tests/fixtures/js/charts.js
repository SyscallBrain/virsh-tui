function rng(seed) { let s = seed >>> 0 || 1; return function () { s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s / 4294967296; }; }
function series(seed, n, base, amp, pull) { const r = rng(seed); let v = base; const out = []; for (let i = 0; i < n; i++) { v += (r() - 0.5) * amp; v += (base - v) * (pull || 0.15); v = Math.max(0.02, Math.min(0.98, v)); out.push(v); } return out; }
function sample(d, x, W) { const t = x * (d.length - 1) / Math.max(1, W - 1); const i = Math.floor(t); const f = t - i; return d[i] * (1 - f) + d[Math.min(i + 1, d.length - 1)] * f; }
function braille(d, w, h, mode) {
  const W = w * 2, H = h * 4, g = [];
  for (let y = 0; y < H; y++) g.push(new Array(W).fill(0));
  let py = null;
  for (let x = 0; x < W; x++) {
    const v = sample(d, x, W);
    if (mode === 'down') { const y = Math.round(v * (H - 1)); for (let yy = 0; yy <= y; yy++) g[yy][x] = 1; }
    else if (mode === 'fill') { const y = Math.round((1 - v) * (H - 1)); for (let yy = y; yy < H; yy++) g[yy][x] = 1; }
    else { const y = Math.round((1 - v) * (H - 1)); if (py === null) py = y; for (let yy = Math.min(py, y); yy <= Math.max(py, y); yy++) g[yy][x] = 1; py = y; }
  }
  const bits = [[0x1, 0x8], [0x2, 0x10], [0x4, 0x20], [0x40, 0x80]], rows = [];
  for (let r = 0; r < h; r++) { let s = ''; for (let c = 0; c < w; c++) { let code = 0; for (let dy = 0; dy < 4; dy++) for (let dx = 0; dx < 2; dx++) if (g[r * 4 + dy][c * 2 + dx]) code |= bits[dy][dx]; s += String.fromCharCode(0x2800 + code); } rows.push(s); }
  return rows;
}
const BL = '▁▂▃▄▅▆▇█';
function sparkRows(d, w, h) { const rows = []; for (let r = 0; r < h; r++) { let s = ''; for (let x = 0; x < w; x++) { const lv = Math.round(sample(d, x, w) * h * 8) - (h - 1 - r) * 8; s += lv <= 0 ? ' ' : BL[Math.min(7, lv - 1)]; } rows.push(s); } return rows; }
function hbar(v, w) { const tot = Math.round(v * w * 8); const full = Math.floor(tot / 8), rem = tot % 8; const fill = '█'.repeat(full) + (rem ? '▏▎▍▌▋▊▉'[rem - 1] : ''); return { fill: fill, rest: '░'.repeat(Math.max(0, w - fill.length)) }; }
function lg(v, w) { const n = Math.round(v * w); return { on: '━'.repeat(n), off: '━'.repeat(w - n) }; }
if (typeof module !== 'undefined') module.exports = { rng, series, sample, braille, sparkRows, hbar, lg };
