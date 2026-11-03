// Floating ring, recreated from yonathanamir/claude-usage-meter (MIT, 260e4f2).
// Paint order and sizes follow meter_widget.py; Qt points converted at 96 dpi (12pt = 16px).
// Attention badge for pending approvals/questions is ours.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const R = 30, M = 12, STEP = 50, RING_R = R - 5;
const PALETTE = {
  claude: { bg: "#1a1714", base: "#d9773c", amber: "#e8a838", red: "#e74c3c", short: "C" },
  codex: { bg: "#101820", base: "#2f80ed", amber: "#56ccf2", red: "#eb5757", short: "X" },
};
const svg = document.getElementById("ring");
let snap = null;
const mode = { claude: "5h", codex: "5h" };

function esc(s) { const d = document.createElement("div"); d.textContent = s == null ? "" : String(s); return d.innerHTML; }
function color(p, pct) { const c = PALETTE[p]; return pct > 80 ? c.red : pct > 50 ? c.amber : c.base; }
function providers() {
  if (!snap) return [];
  const w = snap.settings.providers;
  return ["claude", "codex"].filter((p) => w === "both" || w === p);
}
function usage(p) { return (snap.usage || []).find((u) => u.provider === p) || { status: "loading", windows: [] }; }
function win(p) { return usage(p).windows.find((w) => w.label === mode[p]) || null; }

// "Time Until" for 5h ("Xd Yh" / "Xh Ym" / "Xm"), "Date" for 7d ("Nov 07").
function resetText(p, w) {
  if (!w || !w.resets_at) return "";
  const ms = w.resets_at * 1000 - Date.now();
  if (mode[p] === "7d") return new Date(w.resets_at * 1000).toLocaleDateString("en-US", { month: "short", day: "2-digit" });
  if (ms <= 0) return "now";
  const mins = Math.floor(ms / 60000), d = Math.floor(mins / 1440), h = Math.floor((mins % 1440) / 60), m = mins % 60;
  return d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m}m` : `${m}m`;
}

function circle(p, i) {
  const c = PALETTE[p], cx = R + M, cy = R + M + i * STEP;
  const u = usage(p), w = win(p);
  const pct = w ? w.used_percent : 0;
  const arc = pct > 0 ? color(p, pct) : "#FFFFFF";
  const C = 2 * Math.PI * RING_R;
  let g = `<g data-p="${p}">`;
  g += `<defs><radialGradient id="sh${i}" cx="${cx}" cy="${cy + 2}" r="${R + 6}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="rgba(0,0,0,0.31)"/><stop offset="1" stop-color="rgba(0,0,0,0)"/></radialGradient></defs>`;
  g += `<circle cx="${cx}" cy="${cy + 2}" r="${R + 4}" fill="url(#sh${i})"/>`;
  g += `<circle cx="${cx}" cy="${cy}" r="${R}" fill="${c.bg}"/>`;
  g += `<circle cx="${cx}" cy="${cy}" r="${RING_R}" fill="none" stroke="${c.base}" stroke-opacity="0.118" stroke-width="5"/>`;
  if (pct > 0) {
    g += `<circle cx="${cx}" cy="${cy}" r="${RING_R}" fill="none" stroke="${arc}" stroke-width="5" stroke-linecap="round"
      stroke-dasharray="${(C * Math.min(pct, 100) / 100).toFixed(2)} ${C.toFixed(2)}" transform="rotate(-90 ${cx} ${cy})"/>`;
  }
  g += `<circle cx="${cx}" cy="${cy - RING_R}" r="3" fill="${arc}"/>`;
  if (!w) {
    const t = u.status === "loading" ? "..." : "!";
    g += `<text x="${cx}" y="${cy + 5}" text-anchor="middle" font-size="13" font-weight="700" fill="${arc}">${t}</text>`;
  } else {
    g += `<text x="${cx}" y="${cy + 1}" text-anchor="middle" font-size="16" font-weight="700" fill="${arc}">${Math.floor(pct)}</text>`;
    g += `<text x="${cx}" y="${cy + 15}" text-anchor="middle" font-size="9.3" fill="rgba(200,200,200,0.55)">${esc(resetText(p, w))}</text>`;
  }
  // Mode badge, top-left: "C5h" / "X7d"
  const label = c.short + mode[p];
  const bw = label.length * 6 + 6, bh = 13, bx = cx - R + 1, by = cy - R + 1;
  g += `<rect x="${bx}" y="${by}" width="${bw}" height="${bh}" rx="3" fill="${c.bg}" fill-opacity="0.86" stroke="${arc}" stroke-opacity="0.47"/>`;
  g += `<text x="${bx + bw / 2}" y="${by + 10}" text-anchor="middle" font-size="9.3" font-weight="700" fill="${arc}">${label}</text>`;
  // Warning badge, bottom-left: error/stale reading, or something waiting for you.
  const waiting = (snap.pending || []).some((x) => x.provider === p);
  if (waiting || (u.status !== "ok" && u.status !== "loading")) {
    const wy = cy + R - bh - 1;
    g += `<rect x="${bx}" y="${wy}" width="13" height="${bh}" rx="3" fill="${waiting ? "#e8a838" : "#e74c3c"}" stroke="rgba(255,255,255,0.47)"/>`;
    g += `<text x="${bx + 6.5}" y="${wy + 10}" text-anchor="middle" font-size="9.3" font-weight="700" fill="#fff">!</text>`;
  }
  return g + "</g>";
}

// Both providers: one ring split down the middle. Left half = Claude, right half = Codex,
// each half filling from the top toward the bottom.
function split() {
  const cx = R + M, cy = R + M, half = Math.PI * RING_R, C = 2 * Math.PI * RING_R;
  const side = (p) => { const w = win(p); return { w, pct: w ? w.used_percent : 0, u: usage(p) }; };
  const L = side("claude"), Rt = side("codex");
  const mirror = `translate(${2 * cx} 0) scale(-1 1) rotate(-90 ${cx} ${cy})`;
  const arc = (pct, col, tf) => pct > 0 ? `<circle cx="${cx}" cy="${cy}" r="${RING_R}" fill="none" stroke="${col}" stroke-width="5" stroke-linecap="round"
      stroke-dasharray="${(half * Math.min(pct, 100) / 100).toFixed(2)} ${C.toFixed(2)}" transform="${tf}"/>` : "";
  const track = (col, tf) => `<circle cx="${cx}" cy="${cy}" r="${RING_R}" fill="none" stroke="${col}" stroke-opacity="0.18" stroke-width="5"
      stroke-dasharray="${half.toFixed(2)} ${C.toFixed(2)}" transform="${tf}"/>`;
  const lc = L.pct > 0 ? color("claude", L.pct) : "#FFFFFF", rc = Rt.pct > 0 ? color("codex", Rt.pct) : "#FFFFFF";
  const num = (s) => (s.w ? Math.floor(s.pct) : s.u.status === "loading" ? "…" : "!");
  let g = `<defs><radialGradient id="sh" cx="${cx}" cy="${cy + 2}" r="${R + 6}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="rgba(0,0,0,0.31)"/><stop offset="1" stop-color="rgba(0,0,0,0)"/></radialGradient>
    <clipPath id="lh"><rect x="0" y="0" width="${cx}" height="${2 * cy}"/></clipPath>
    <clipPath id="rh"><rect x="${cx}" y="0" width="${cx}" height="${2 * cy}"/></clipPath></defs>`;
  g += `<circle cx="${cx}" cy="${cy + 2}" r="${R + 4}" fill="url(#sh)"/>`;
  g += `<circle cx="${cx}" cy="${cy}" r="${R}" fill="${PALETTE.claude.bg}" clip-path="url(#lh)"/>`;
  g += `<circle cx="${cx}" cy="${cy}" r="${R}" fill="${PALETTE.codex.bg}" clip-path="url(#rh)"/>`;
  g += track(PALETTE.claude.base, mirror) + track(PALETTE.codex.base, `rotate(-90 ${cx} ${cy})`);
  g += arc(L.pct, lc, mirror) + arc(Rt.pct, rc, `rotate(-90 ${cx} ${cy})`);
  // Divider through the ring at top and bottom
  g += `<line x1="${cx}" y1="${cy - R}" x2="${cx}" y2="${cy - RING_R + 4}" stroke="#000" stroke-width="2"/>`;
  g += `<line x1="${cx}" y1="${cy + RING_R - 4}" x2="${cx}" y2="${cy + R}" stroke="#000" stroke-width="2"/>`;
  g += `<line x1="${cx}" y1="${cy - 12}" x2="${cx}" y2="${cy + 8}" stroke="rgba(255,255,255,0.15)" stroke-width="1"/>`;
  g += `<text x="${cx - 9}" y="${cy + 2}" text-anchor="middle" font-size="13" font-weight="700" fill="${lc}">${num(L)}</text>`;
  g += `<text x="${cx + 9}" y="${cy + 2}" text-anchor="middle" font-size="13" font-weight="700" fill="${rc}">${num(Rt)}</text>`;
  g += `<text x="${cx}" y="${cy + 15}" text-anchor="middle" font-size="9.3" fill="rgba(200,200,200,0.55)">${mode.claude}</text>`;
  const waiting = (snap.pending || []).length > 0;
  const bad = [L, Rt].some((s) => s.u.status !== "ok" && s.u.status !== "loading");
  if (waiting || bad) {
    const bx = cx - R + 1, wy = cy + R - 14;
    g += `<rect x="${bx}" y="${wy}" width="13" height="13" rx="3" fill="${waiting ? "#e8a838" : "#e74c3c"}" stroke="rgba(255,255,255,0.47)"/>`;
    g += `<text x="${bx + 6.5}" y="${wy + 10}" text-anchor="middle" font-size="9.3" font-weight="700" fill="#fff">!</text>`;
  }
  return g;
}

function render() {
  if (!snap) return;
  const ps = providers();
  svg.setAttribute("height", 2 * (R + M));
  svg.innerHTML = ps.length === 2 ? split() : circle(ps[0], 0);
}

// ---- drag + snap (snap animation runs in Rust: 250 ms OutCubic) ----
let down = null, dragged = false, hoverP = null;
function providerAt(x) { const ps = providers(); return ps.length === 2 ? (x < R + M ? "claude" : "codex") : ps[0]; }
document.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
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
  if (!down) return;
  const wasDrag = dragged;
  down = null;
  if (wasDrag) { invoke("ring_drop"); return; }
  const p = providerAt(e.clientX);
  if ((snap.pending || []).length) { invoke("show_panel"); return; }
  const next = mode[p] === "5h" ? "7d" : "5h";
  if (providers().length === 2) { mode.claude = next; mode.codex = next; } else mode[p] = next;
  render();
  showTip();
});
document.addEventListener("mouseleave", () => { hoverP = null; invoke("ring_tip", { show: false, provider: "", mode: "", height: 0 }); });
document.addEventListener("contextmenu", (e) => { e.preventDefault(); invoke("show_panel"); });

function showTip() {
  if (!hoverP || !snap) return;
  const rows = usage(hoverP).windows.length;
  const height = Math.max(80, 46 + rows * 44 + 30 + 30);
  invoke("ring_tip", { show: true, provider: hoverP, mode: mode[hoverP], height });
}

listen("state", (e) => { snap = e.payload; render(); });
invoke("get_state").then((s) => { snap = s; render(); });
setInterval(render, 30000);
