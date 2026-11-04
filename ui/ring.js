// Floating ring. Drag, snap and tooltip behaviour from yonathanamir/claude-usage-meter (MIT, 260e4f2).
// Look and motion match the notch (island.css / island.js): pure black disc, white ink, inner
// session ring (lighter shade) and thin outer week ring in the shared OKLCH usage colours
// (usage-color.js), an activity ring inside (spinning while working, pulsing amber when it needs
// you). Ring values ride springs (motion.js), so changes sweep instead of jumping.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const R = 30, M = 12;
const SESS_R = 23, SESS_W = 4, WEEK_R = 27.6, WEEK_W = 2.2, ACT_R = 18.5;
const svg = document.getElementById("ring");
let snap = null;
const mode = { claude: "5h", codex: "5h" };
const { Spring, Animator, PRESET } = Motion;
const sp = {};
for (const p of ["claude", "codex"]) { sp[p + "s"] = new Spring(0, PRESET.content); sp[p + "w"] = new Spring(0, PRESET.content); }
const anim = new Animator(sp, () => drawArcs());

function esc(s) { const d = document.createElement("div"); d.textContent = s == null ? "" : String(s); return d.innerHTML; }
function providers() {
  if (!snap) return [];
  const w = snap.settings.providers;
  return ["claude", "codex"].filter((p) => w === "both" || w === p);
}
function usage(p) { return (snap.usage || []).find((u) => u.provider === p) || { status: "loading", windows: [] }; }
function winOf(p, label) { return usage(p).windows.find((w) => w.label === label) || null; }
function win(p) { return winOf(p, mode[p]); }
function missed() { return (snap ? snap.missed || [] : []).filter((m) => providers().includes(m.provider)); }
function openMissed() { const m = missed()[0]; if (m) invoke("jump_session", { key: m.session_key }).catch(() => {}); }
function dismissMissed() { for (const m of missed()) invoke("dismiss_missed", { id: m.id }).catch(() => {}); }
function activity(p) {
  const mine = (snap.sessions || []).filter((s) => s.provider === p);
  if (missed().some((x) => x.provider === p) || (snap.pending || []).some((x) => x.provider === p) || mine.some((s) => s.attention)) return "attention";
  return mine.some((s) => s.state === "working") ? "working" : "";
}

// "Time Until" for 5h ("Xd Yh" / "Xh Ym" / "Xm"), "Date" for 7d ("Nov 07").
function resetText(p, w) {
  if (!w || !w.resets_at) return "";
  const ms = w.resets_at * 1000 - Date.now();
  if (mode[p] === "7d") return new Date(w.resets_at * 1000).toLocaleDateString("en-US", { month: "short", day: "2-digit" });
  if (ms <= 0) return "now";
  const mins = Math.floor(ms / 60000), d = Math.floor(mins / 1440), h = Math.floor((mins % 1440) / 60), m = mins % 60;
  return d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m}m` : `${m}m`;
}

// Arc from 12 o'clock, clockwise (dir 1) or anticlockwise (dir -1), over `span` of the circle.
function arc(cx, cy, r, w, pct, light, span, dir) {
  const L = 2 * Math.PI * r, p = Math.max(0, Math.min(100, pct));
  if (p <= 0.2) return "";
  const tf = dir < 0 ? `translate(${2 * cx} 0) scale(-1 1) rotate(-90 ${cx} ${cy})` : `rotate(-90 ${cx} ${cy})`;
  return `<circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="${usageColor(p, light)}" stroke-width="${w}" stroke-linecap="round"
    stroke-dasharray="${((L * span * p) / 100).toFixed(2)} ${L.toFixed(2)}" transform="${tf}"/>`;
}
function track(cx, cy, r, w, a) { return `<circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="var(--notch-ink-${Math.round(a * 100)})" stroke-width="${w}"/>`; }

// Static layer (disc, tracks, activity ring, text) is rebuilt only when it changes, so the CSS
// spin/pulse keeps running. The arcs layer is redrawn on every spring frame.
let lastStatic = "";
function render() {
  if (!snap) return;
  const ps = providers();
  if (!ps.length) return;
  for (const p of ps) {
    const s = winOf(p, "5h") || usage(p).windows[0], w = winOf(p, "7d");
    sp[p + "s"].to(s ? s.used_percent : 0);
    sp[p + "w"].to(w && w !== s ? w.used_percent : 0);
  }
  const cx = R + M, cy = R + M;
  const split = ps.length === 2;
  const acts = ps.map(activity);
  const act = acts.includes("attention") ? "attention" : acts.includes("working") ? "working" : "";
  const stale = ps.every((p) => usage(p).status !== "ok" && usage(p).status !== "loading");
  const num = (p) => { const w = win(p); return w ? String(Math.floor(w.used_percent)) : usage(p).status === "loading" ? "…" : "!"; };
  let g = `<defs><radialGradient id="sh" cx="${cx}" cy="${cy + 2}" r="${R + 6}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="var(--notch-ring-shadow)"/><stop offset="1" stop-color="var(--notch-clear)"/></radialGradient></defs>`;
  g += `<circle cx="${cx}" cy="${cy + 2}" r="${R + 4}" fill="url(#sh)"/>`;
  g += `<circle cx="${cx}" cy="${cy}" r="${R}" fill="var(--notch-bg)" stroke="var(--notch-outline)" stroke-width="1"/>`;
  g += `<g class="${stale ? "stale" : ""}">`;
  g += track(cx, cy, SESS_R, SESS_W, 0.12) + track(cx, cy, WEEK_R, WEEK_W, 0.08);
  g += `<g id="arcs"></g>`;
  if (split) {
    // Two separate halves: left Claude, right Codex. Arcs are clipped to their own half (see
    // drawArcs), and a gap through the middle keeps one side's colour from touching the other.
    g += `<rect x="${cx - 1.5}" y="${cy - R}" width="3" height="${2 * R}" fill="var(--notch-bg)"/>`;
    // Activity per half: a D-shaped loop (that half's round edge plus the centre line). Working =
    // a dash running round the loop; attention = the loop pulsing amber; idle = nothing.
    g += `<line x1="${cx}" y1="${cy - WEEK_R}" x2="${cx}" y2="${cy + WEEK_R}" stroke="var(--notch-ink-16)" stroke-width="1"/>`;
    for (const [p, sweep, dx] of [["claude", 0, -1.5], ["codex", 1, 1.5]]) {
      const a = activity(p), x = cx + dx, r = ACT_R;
      const d = `M ${x} ${cy - r} A ${r} ${r} 0 0 ${sweep} ${x} ${cy + r} Z`;
      if (a === "working") {
        g += `<path class="run" d="${d}" pathLength="100" fill="none" stroke="var(--notch-activity)" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="22 78"/>`;
      } else if (a === "attention") {
        g += `<path class="pulse" d="${d}" fill="none" stroke="var(--warn)" stroke-width="1.4" stroke-linejoin="round"/>`;
      }
    }
    // Each percentage sits in the middle of its own half.
    g += `<text class="n h" x="${cx - 10}" y="${cy + 2}" text-anchor="middle">${num("claude")}</text>`;
    g += `<text class="n h" x="${cx + 10}" y="${cy + 2}" text-anchor="middle">${num("codex")}</text>`;
    g += `<text class="s" x="${cx - 10}" y="${cy + 12}" text-anchor="middle">${mode.claude}</text>`;
    g += `<text class="s" x="${cx + 10}" y="${cy + 12}" text-anchor="middle">${mode.codex}</text>`;
  } else {
    if (act === "working") g += `<g class="spin"><circle cx="${cx}" cy="${cy}" r="${ACT_R}" fill="none" stroke="var(--notch-activity)" stroke-width="1.4" stroke-linecap="round" stroke-dasharray="${(2 * Math.PI * ACT_R * 0.28).toFixed(2)} 200"/></g>`;
    if (act === "attention") g += `<circle class="pulse" cx="${cx}" cy="${cy}" r="${ACT_R}" fill="none" stroke="var(--warn)" stroke-width="1.4"/>`;
    const p = ps[0];
    g += `<text class="n" x="${cx}" y="${cy + 3}" text-anchor="middle">${num(p)}</text>`;
    g += `<text class="s" x="${cx}" y="${cy + 12}" text-anchor="middle">${esc(resetText(p, win(p)))}</text>`;
  }
  g += "</g>";
  const count = missed().length;
  if (count) g += `<g id="missed" role="button" tabindex="0" aria-label="${count} waiting in terminal. Open terminal. Right-click or press Delete to dismiss.">
    <title>${count} waiting in terminal. Click to open terminal. Right-click to dismiss.</title>
    <rect x="56" y="9" width="25" height="17" rx="8.5" fill="var(--warn)"/>
    <text class="missed-count" x="68.5" y="21" text-anchor="middle">${count > 99 ? "99+" : count}</text></g>`;
  svg.setAttribute("height", 2 * (R + M));
  if (g !== lastStatic) { svg.innerHTML = g; lastStatic = g; drawArcs(); }
  anim.kick();
}
function drawArcs() {
  const box = svg.querySelector("#arcs");
  if (!box || !snap) return;
  const ps = providers(), cx = R + M, cy = R + M;
  let g = "";
  if (ps.length === 2) {
    // Clip each half so round caps never cross the middle, even at 100 %.
    g += `<defs><clipPath id="lh"><rect x="0" y="0" width="${cx - 1.5}" height="${2 * cy}"/></clipPath>
      <clipPath id="rh"><rect x="${cx + 1.5}" y="0" width="${cx}" height="${2 * cy}"/></clipPath></defs>`;
    g += `<g clip-path="url(#lh)">` + arc(cx, cy, SESS_R, SESS_W, sp.claudes.x, true, 0.5, -1) + arc(cx, cy, WEEK_R, WEEK_W, sp.claudew.x, false, 0.5, -1) + "</g>";
    g += `<g clip-path="url(#rh)">` + arc(cx, cy, SESS_R, SESS_W, sp.codexs.x, true, 0.5, 1) + arc(cx, cy, WEEK_R, WEEK_W, sp.codexw.x, false, 0.5, 1) + "</g>";
  } else if (ps.length) {
    const p = ps[0];
    g += arc(cx, cy, SESS_R, SESS_W, sp[p + "s"].x, true, 1, 1) + arc(cx, cy, WEEK_R, WEEK_W, sp[p + "w"].x, false, 1, 1);
  }
  box.innerHTML = g;
}

// ---- drag + snap (snap animation runs in Rust: 250 ms OutCubic) ----
let down = null, dragged = false, hoverP = null;
function providerAt(x) { const ps = providers(); return ps.length === 2 ? (x < R + M ? "claude" : "codex") : ps[0]; }
document.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  if (e.target.closest("#missed")) { down = null; dragged = false; return; }
  down = { sx: e.screenX, sy: e.screenY, ox: e.screenX - window.screenX, oy: e.screenY - window.screenY };
  dragged = false;
});
document.addEventListener("mousemove", (e) => {
  if (down) {
    dragged = true;
    const k = window.devicePixelRatio || 1;
    invoke("ring_move", { x: Math.round((e.screenX - down.ox) * k), y: Math.round((e.screenY - down.oy) * k) });
    invoke("ring_tip", { show: false, provider: "", mode: "", height: 0 });
    return;
  }
  const p = providerAt(e.clientX);
  if (p !== hoverP) { hoverP = p; showTip(); }
});
document.addEventListener("mouseup", (e) => {
  if (e.button !== 0) return;
  if (e.target.closest("#missed")) { down = null; if (!dragged) openMissed(); return; }
  if (!down) return;
  const wasDrag = dragged;
  down = null;
  if (wasDrag) { invoke("ring_drop"); return; }
  const p = providerAt(e.clientX);
  if ((snap.pending || []).length) { invoke("show_requests"); return; }
  if (missed().length) { openMissed(); return; }
  const next = mode[p] === "5h" ? "7d" : "5h";
  if (providers().length === 2) { mode.claude = next; mode.codex = next; } else mode[p] = next;
  render();
  showTip();
});
document.addEventListener("mouseleave", () => { hoverP = null; invoke("ring_tip", { show: false, provider: "", mode: "", height: 0 }); });
document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  if (e.target.closest("#missed")) dismissMissed(); else invoke("show_panel");
});
document.addEventListener("keydown", (e) => {
  if (!e.target.closest("#missed")) return;
  if (e.key === "Enter" || e.key === " ") { e.preventDefault(); openMissed(); }
  if (e.key === "Delete") { e.preventDefault(); dismissMissed(); }
});

function showTip() {
  if (!hoverP || !snap) return;
  const rows = usage(hoverP).windows.length;
  const height = Math.max(80, 46 + rows * 44 + 30 + 30 + (missed().length ? 42 : 0));
  invoke("ring_tip", { show: true, provider: hoverP, mode: mode[hoverP], height });
}

listen("state", (e) => { snap = e.payload; render(); showTip(); });
invoke("get_state").then((s) => { snap = s; render(); });
setInterval(render, 30000);
