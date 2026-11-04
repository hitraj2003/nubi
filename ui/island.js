window.addEventListener("error", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/island.js: ${e.message} @${e.lineno}` }));
window.addEventListener("unhandledrejection", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/island.js: ${e.reason}` }));
// Top island (display mode "island"). One black shape hanging from the top edge of the screen.
// Closed it is a small pill; on hover, a new request or an alert, its width, height and corner
// radius spring to the new view while the old content blurs out and the new one blurs in.
// All dynamic text goes through textContent; only the bundled provider glyphs use innerHTML.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { Spring, Animator, PRESET, slider, pressable, leave } = window.Motion;

const $ = (id) => document.getElementById(id);
const NAME = { claude: "Claude", codex: "Codex" };
const LABEL = { "5h": "Session", "7d": "Week", "7d Opus": "Week, Opus", "7d Sonnet": "Week, Sonnet" };
const ICONS = {
  success: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>',
  warning: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 9v4"/><path d="M12 17h.01"/><path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/></svg>',
  info: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 16v-4"/><path d="M12 8h.01"/></svg>',
};
const KIND = { done: ["success", "success"], permission: ["warning", "error"], question: ["info", "success"] };

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}
function btn(text, cls, fn) {
  const b = el("button", cls, text);
  b.type = "button";
  b.addEventListener("click", (e) => { e.stopPropagation(); fn(); });
  return b;
}
const glyphs = {};
function glyph(p) {
  const g = el("span", "glyph p-" + p);
  g.dataset.glyph = p;
  g.innerHTML = glyphs[p] || "";
  return g;
}
for (const p of ["claude", "codex"]) {
  fetch("glyphs/" + p + ".svg").then((r) => r.text()).then((t) => {
    glyphs[p] = t.replace(/<metadata>[\s\S]*?<\/metadata>/, "");
    document.querySelectorAll(`[data-glyph="${p}"]`).forEach((g) => (g.innerHTML = glyphs[p]));
  }).catch(() => {});
}
function ago(ms) { const s = Math.max(0, Math.round((Date.now() - ms) / 1000)); return s < 60 ? s + "s" : s < 3600 ? Math.round(s / 60) + "m" : Math.round(s / 3600) + "h"; }
function resetIn(sec) {
  if (!sec) return "";
  const m = Math.round((sec * 1000 - Date.now()) / 60000);
  if (m <= 0) return "resetting";
  return "resets in " + (m < 60 ? m + "m" : m < 1440 ? Math.floor(m / 60) + "h " + (m % 60) + "m" : Math.round(m / 1440) + "d");
}
function summary(a) {
  try {
    const v = JSON.parse(a.tool_input || "{}");
    const s = v.command || v.file_path || v.path || v.url || v.pattern || Object.values(v).find((x) => typeof x === "string") || "";
    return String(s).trim();
  } catch (_) { return ""; }
}
function ring(cls, size, stroke, pct, color) {
  const ns = "http://www.w3.org/2000/svg", r = (size - stroke) / 2, c = 2 * Math.PI * r;
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("class", cls);
  svg.setAttribute("viewBox", `0 0 ${size} ${size}`);
  for (const k of ["tr", "fg"]) {
    const ci = document.createElementNS(ns, "circle");
    ci.setAttribute("class", k); ci.setAttribute("cx", size / 2); ci.setAttribute("cy", size / 2); ci.setAttribute("r", r);
    if (k === "fg") { ci.style.strokeDasharray = `${(c * Math.min(100, pct)) / 100} ${c}`; if (color) ci.style.stroke = color; }
    svg.append(ci);
  }
  return svg;
}
// Week ring: always amber orange, so it never reads as the session ring.
// usageColor() comes from usage-color.js (shared with the ring, tooltip and taskbar).
const pctColor = (p) => usageColor(p, false);

// A ring arc in one solid colour: the colour for its current percent (see usageColor). Starts
// at 12 o'clock and runs clockwise up to `pct`.
function gradArc(cx, r, width, pct, light) {
  const c = document.createElementNS("http://www.w3.org/2000/svg", "circle");
  const L = 2 * Math.PI * r, end = Math.max(0, Math.min(100, pct));
  c.setAttribute("cx", cx); c.setAttribute("cy", cx); c.setAttribute("r", r);
  c.setAttribute("fill", "none");
  c.setAttribute("stroke", usageColor(end, light));
  c.setAttribute("stroke-width", width);
  c.setAttribute("stroke-linecap", "round");
  c.setAttribute("stroke-dasharray", `${(L * end) / 100} ${L}`);
  c.setAttribute("transform", `rotate(-90 ${cx} ${cx})`);
  if (end <= 0) c.setAttribute("stroke-opacity", "0");
  return c;
}

// Sounds: one per alert kind, from sounds.js (window.chime).

// ---------- the shape ----------
// Springs work in edge terms: `a` along the screen edge, `d` away from it (depth), `r` outer
// corner radius, `e` ear radius. The drawing maps them to screen x/y for the current edge.
const shape = $("shape");
const views = { tab: $("v-tab"), compact: $("v-compact"), alert: $("v-alert"), open: $("v-open") };
const springs = { a: new Spring(180, PRESET.open), d: new Spring(34, PRESET.open), r: new Spring(16, PRESET.open), e: new Spring(8, PRESET.open) };
const bg = $("bg"), bgp = $("bgp"), viewsBox = $("views");
let edge = "top";
let offset = 0; // shift along the edge inside the window (near the screen ends)
const vertical = () => edge === "left" || edge === "right";

// Outline in edge terms (u along, v depth, edge at v = 0): concave ear flowing into each straight
// side, round outer corners. Cubic curves with k = 0.62 read smoother than circular arcs.
function outline(a, d, r, e) {
  r = Math.min(r, d / 2, a / 2); e = Math.min(e, Math.max(0, d - r));
  const k = 0.62, A = a + 2 * e;
  const P = (u, v) => {
    const x = edge === "top" || edge === "bottom" ? u : edge === "left" ? v : d - v;
    const y = edge === "top" ? v : edge === "bottom" ? d - v : u;
    return x.toFixed(2) + " " + y.toFixed(2);
  };
  return `M${P(0, 0)}L${P(A, 0)}` +
    `C${P(A - e * k, 0)} ${P(A - e, e * (1 - k))} ${P(A - e, e)}` +
    `L${P(A - e, d - r)}C${P(A - e, d - r * (1 - k))} ${P(A - e - r * (1 - k), d)} ${P(A - e - r, d)}` +
    `L${P(e + r, d)}C${P(e + r * (1 - k), d)} ${P(e, d - r * (1 - k))} ${P(e, d - r)}` +
    `L${P(e, e)}C${P(e, e * (1 - k))} ${P(e * k, 0)} ${P(0, 0)}Z`;
}
const CORNERS = { top: (r) => `0 0 ${r}px ${r}px`, bottom: (r) => `${r}px ${r}px 0 0`, left: (r) => `0 ${r}px ${r}px 0`, right: (r) => `${r}px 0 0 ${r}px` };

// Shift actually used: the wanted offset, limited so the whole shape stays inside the window.
function shift(A) {
  const win = vertical() ? innerHeight : innerWidth;
  const room = Math.max(0, (win - A) / 2);
  return Math.max(-room, Math.min(room, offset));
}
let lastHot = "";
function reportHot() {
  // While moving, the hot area covers both where the shape is and where it is going.
  const a = Math.max(springs.a.x, springs.a.target), d = Math.max(springs.d.x, springs.d.target);
  const e = Math.max(springs.e.x, springs.e.target), A = a + 2 * e;
  let x, y, w, h;
  const o = shift(A);
  if (vertical()) { w = d; h = A; y = (innerHeight - h) / 2 + o; x = edge === "left" ? 0 : innerWidth - w; }
  else { w = A; h = d; x = (innerWidth - w) / 2 + o; y = edge === "top" ? 0 : innerHeight - h; }
  const k = devicePixelRatio, key = [x, y, w, h].map(Math.round).join();
  if (key === lastHot) return;
  lastHot = key;
  invoke("report_island_hot", { rect: [x * k, y * k, w * k, h * k] }).catch(() => {});
}
const anim = new Animator(springs, (v) => {
  const d = Math.max(1, v.d), A = v.a + 2 * v.e;
  if (vertical()) {
    shape.style.width = d + "px"; shape.style.height = v.a + "px";
    bg.setAttribute("width", d); bg.setAttribute("height", A);
    bg.style.left = "0px"; bg.style.top = -v.e + "px";
  } else {
    shape.style.width = v.a + "px"; shape.style.height = d + "px";
    bg.setAttribute("width", A); bg.setAttribute("height", d);
    bg.style.top = "0px"; bg.style.left = -v.e + "px";
  }
  bgp.setAttribute("d", outline(v.a, d, v.r, v.e));
  const o = shift(A);
  shape.style.translate = vertical() ? `0 ${o}px` : `${o}px 0`;
  viewsBox.style.borderRadius = CORNERS[edge](v.r);
  reportHot();
});

let current = "compact";
// [along, depth, corner radius, ear radius] for a view.
function targetSize(name) {
  const v = views[name], V = vertical();
  const along = V ? v.offsetHeight : v.offsetWidth, depth = V ? v.offsetWidth : v.offsetHeight;
  if (name === "tab") return [56, 6, 3, 4];
  if (name === "compact") return [Math.max(V ? 60 : 120, along), Math.max(V ? 46 : 38, depth), V ? 18 : 16, 8];
  if (name === "alert") return [Math.max(260, along), 56, 22, 14];
  const maxA = (V ? innerHeight : innerWidth) - 48, maxD = (V ? innerWidth : innerHeight) - 24;
  return [Math.min(maxA, along), Math.min(maxD, depth), 32, 20];
}
function morph() {
  const [a, d, r, e] = targetSize(current);
  const grow = a * d > springs.a.target * springs.d.target + 1;
  const p = grow ? PRESET.open : PRESET.close;
  springs.a.to(a, p); springs.d.to(d, p); springs.r.to(r, p); springs.e.to(e, p);
  anim.kick();
}
function show(name) {
  if (name === current) { morph(); return; }
  views[current].classList.remove("on");
  current = name;
  views[name].classList.add("on");
  shape.className = name;
  morph();
}
function snapTo(name) {
  views[current].classList.remove("on");
  current = name; views[name].classList.add("on"); shape.className = name;
  const [a, d, r, e] = targetSize(name);
  springs.a.jump(a); springs.d.jump(d); springs.r.jump(r); springs.e.jump(e);
  lastHot = ""; anim.draw(anim.values());
}
// The open view grows and shrinks with its content (tab switch, a request answered).
new ResizeObserver(() => { if (current === "open") morph(); }).observe(views.open);
new ResizeObserver(() => { if (current === "compact") morph(); }).observe(views.compact);
listen("island_edge", (e) => setEdge(e.payload));
// The page can also miss the first offset event; ask again with every state update (cheap).
function fetchOffset() {
  invoke("island_offset").then((o) => { o = Number(o) || 0; if (o !== offset) { offset = o; lastHot = ""; anim.draw(anim.values()); } }).catch(() => {});
}
listen("island_offset", (e) => { offset = Number(e.payload) || 0; lastHot = ""; anim.draw(anim.values()); });

// ---------- what to show ----------
let snap = null;
let hovering = false, hoverTimer = 0;
let requestsUp = false, foldTimer = 0; // a new request opens the island for a while
let alertUp = false;
let tab = "sessions";
let folded = false, foldTimer2 = 0; // "fold until hovered": thin tab after a short grace
let dragging = false;
let clicked = false; // fold mode: hover shows the pill, a click opens it

function decide() {
  const typing = document.activeElement && document.activeElement.classList.contains("other");
  if (dragging) { show("compact"); return; }
  const hoverOpens = hovering && (clicked || !(snap && snap.settings.notch_on_hover));
  if (hoverOpens || requestsUp || typing) { folded = false; clearTimeout(foldTimer2); show("open"); return; }
  if (alertUp) { show("alert"); return; }
  const canFold = snap && snap.settings.notch_on_hover && !pending().length;
  if (!canFold) { folded = false; clearTimeout(foldTimer2); show("compact"); return; }
  if (hovering) { folded = false; clearTimeout(foldTimer2); show("compact"); return; }
  if (folded) { show("tab"); return; }
  show("compact");
  clearTimeout(foldTimer2);
  foldTimer2 = setTimeout(() => { if (!hovering && !dragging) { folded = true; decide(); } }, 450);
}

listen("island_pointer", (e) => {
  clearTimeout(hoverTimer);
  if (e.payload === true) {
    // A short pause so a cursor passing over the top of the screen does not open it.
    hoverTimer = setTimeout(() => { if (dragging) return; hovering = true; clearTimeout(foldTimer); if (pending().length && !requestsUp) setTab("requests"); decide(); }, 40);
  } else {
    hoverTimer = setTimeout(() => { hovering = false; clicked = false; requestsUp = false; decide(); }, 140);
  }
});

// ---------- alerts: queue, one at a time, 0.3 s apart ----------
const queue = [];
let alertBusy = false;
function nextAlert() {
  if (!queue.length) { alertBusy = false; alertUp = false; decide(); return; }
  alertBusy = true;
  const a = queue.shift();
  const [type] = KIND[a.kind] || ["info"];
  const ic = $("a-icon");
  ic.className = "a-icon " + type;
  ic.innerHTML = ICONS[type];
  $("a-title").textContent = `${NAME[a.provider] || "Claude"} · ${a.title}`;
  $("a-body").textContent = (a.body || "").replace(/\s+/g, " ").slice(0, 160);
  alertUp = true;
  decide();
  setTimeout(() => { alertUp = false; decide(); setTimeout(nextAlert, 300 + 450); }, 4200);
}
listen("pill", (e) => {
  if (edge !== "top") return; // other edges: the app sends alerts to the toast instead
  const a = e.payload;
  if (a.sound) chime(a.kind);
  // Requests show as cards in the open island; the alert is only for ones it cannot answer.
  if (a.kind === "permission" || a.kind === "question") {
    setTimeout(() => {
      if (!pending().some((p) => p.session_key === a.session_key)) { queue.push(a); if (!alertBusy) nextAlert(); }
    }, 400);
    return;
  }
  queue.push(a);
  if (!alertBusy) nextAlert();
});
listen("show_requests", () => { setTab("requests"); requestsUp = true; armFold(); decide(); });

// ---------- tabs ----------
const tabs = $("tabs");
const moveHl = slider($("tab-hl"));
function setTab(t) {
  if (t === tab && $("pane").dataset.tab === t) return;
  tab = t;
  tabs.querySelectorAll("button").forEach((b) => b.classList.toggle("on", b.dataset.tab === t));
  moveHl(tabs.querySelector(`[data-tab="${t}"]`));
  renderPane(true);
}
tabs.addEventListener("click", (e) => { const b = e.target.closest("button[data-tab]"); if (b) setTab(b.dataset.tab); });
$("open-panel").addEventListener("click", () => invoke("show_panel"));
$("dnd").addEventListener("click", () => invoke("set_dnd", { on: !(snap && snap.dnd_manual) }));
pressable(document.body);

// Drag along the edge: press on the pill (or the empty part of the open bar) and move. The shape
// folds to the pill while it travels; the place is saved on release.
let press = null;
shape.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  const onPill = e.target.closest("#v-compact, #v-tab");
  const onBar = e.target.closest(".bar") && !e.target.closest("button");
  if (onPill || onBar) press = { x: e.screenX, y: e.screenY };
});
addEventListener("pointermove", (e) => {
  if (!press) return;
  if (!dragging && Math.hypot(e.screenX - press.x, e.screenY - press.y) < 5) return;
  if (!dragging) { dragging = true; hovering = false; shape.classList.add("dragging"); decide(); }
  const k = devicePixelRatio;
  invoke("notch_drag", { x: e.screenX * k, y: e.screenY * k }).catch(() => {});
});
addEventListener("pointerup", () => {
  if (press && !dragging && !clicked) { clicked = true; decide(); }
  if (dragging) { invoke("notch_drag_end").catch(() => {}); shape.classList.remove("dragging"); setTimeout(() => { dragging = false; decide(); }, 0); }
  press = null;
});

// ---------- content ----------
const wanted = (p) => { const w = snap.settings.providers; return w === "both" || w === p.provider; };
const pending = () => (snap ? (snap.pending || []).filter(wanted) : []);
const mode = () => (snap && snap.settings.questions_mode) || "system";

// Provider mark inside its rings: session usage (thick), week (thin, outside), and activity
// inside (spinning while working, pulsing amber when it needs you).
const SVGNS = "http://www.w3.org/2000/svg";
function circ(r, stroke, width, frac, cls) {
  const c = document.createElementNS(SVGNS, "circle");
  c.setAttribute("cx", 15); c.setAttribute("cy", 15); c.setAttribute("r", r);
  c.setAttribute("stroke", stroke); c.setAttribute("stroke-width", width);
  if (frac != null) { const L = 2 * Math.PI * r; c.setAttribute("stroke-dasharray", `${L * Math.max(0, Math.min(1, frac))} ${L}`); c.setAttribute("stroke-linecap", "round"); }
  if (cls) c.setAttribute("class", cls);
  return c;
}
function provRing(provider) {
  const u = snap.usage.find((x) => x.provider === provider) || { status: "loading", windows: [] };
  const h = u.windows.find((x) => x.label === "5h") || u.windows[0];
  const wk = u.windows.find((x) => x.label === "7d");
  const mine = snap.sessions.filter((s) => s.provider === provider);
  const act = pending().some((p) => p.provider === provider) || mine.some((s) => s.attention) ? "attention"
    : mine.some((s) => s.state === "working") ? "working" : "";
  const box = el("span", "pr p-" + provider + (u.status !== "ok" && h ? " stale" : ""));
  const wrap = el("span", "prw");
  const svg = document.createElementNS(SVGNS, "svg");
  svg.setAttribute("class", "rr"); svg.setAttribute("viewBox", "0 0 30 30");
  svg.append(circ(11, "rgba(255,255,255,.12)", 2.6));
  if (h) svg.append(gradArc(15, 11, 2.6, h.used_percent, true));
  if (wk && wk !== h) { svg.append(circ(14.4, "rgba(255,255,255,.08)", 1.6)); svg.append(gradArc(15, 14.4, 1.6, wk.used_percent, false)); }
  if (act === "working") { const g = document.createElementNS(SVGNS, "g"); g.setAttribute("class", "spin"); g.append(circ(7.6, "rgba(255,255,255,.85)", 1.4, 0.28)); svg.append(g); }
  if (act === "attention") svg.append(circ(7.6, "var(--warn)", 1.4, null, "pulse"));
  const gl = glyph(provider);
  if (h && h.used_percent >= 100) gl.classList.add("dim");
  wrap.append(svg, gl);
  const pct = h ? Math.round(h.used_percent) : -1;
  box.append(wrap, el("span", "pct", pct >= 0 ? pct + "%" : u.status === "loading" ? "…" : "—"));
  box.title = `${NAME[provider]}: ${pct >= 0 ? pct + "% of session" : u.message || u.status}${wk ? ", " + Math.round(wk.used_percent) + "% of week" : ""}`;
  return { el: box, sig: [pct, wk ? Math.round(wk.used_percent) : -1, act, u.status].join() };
}
function renderCompact() {
  const left = $("c-left"), right = $("c-right");
  const p = pending();
  const working = snap.sessions.filter((s) => s.state === "working");
  const want = snap.settings.providers;
  const provs = want === "both" ? ["claude", "codex"] : [want];
  const L = [], R = [];
  const rings = provs.map(provRing);
  rings.forEach((r) => L.push(r.el));
  if (p.length) {
    L.push(el("span", "c-txt", p[0].kind === "question" ? "Question" : "Needs approval"));
    R.push(el("span", "badge", String(p.length)));
  } else if (working.length) {
    // One session: its folder and working bars. Several: a count only.
    if (working.length === 1) {
      const eq = el("span", "eq p-" + working[0].provider); eq.append(el("i"), el("i"), el("i"));
      L.push(el("span", "c-txt", working[0].name || working[0].folder));
      R.push(eq);
    } else {
      L.push(el("span", "c-txt", `${working.length} working`));
    }
  }
  const sig = JSON.stringify([p.map((x) => x.id), working.map((s) => s.key), rings.map((r) => r.sig), provs, edge]);
  if (left.dataset.sig === sig) return;
  left.dataset.sig = sig;
  left.replaceChildren(...L);
  right.replaceChildren(...R);
}

function renderSessions(pane) {
  if (!snap.sessions.length) { pane.append(el("div", "empty", "No sessions yet. Start Claude Code or Codex with hooks on.")); return; }
  const list = [...snap.sessions].sort((a, b) => b.last_event_ms - a.last_event_ms).slice(0, 8);
  list.forEach((x, i) => {
    const r = el("button", "srow");
    r.type = "button";
    r.title = "Jump to terminal";
    r.style.setProperty("--i", i);
    r.addEventListener("click", () => invoke("jump_session", { key: x.key }).catch(() => {}));
    const s = el("span", "s");
    const extra = x.subagents ? ` · ${x.subagents} agents` : "";
    if (x.state === "working") { const eq = el("span", "eq p-" + x.provider); eq.append(el("i"), el("i"), el("i")); s.append(eq, document.createTextNode("Working" + extra)); }
    else s.append(el("span", "dot " + x.state), document.createTextNode((x.attention ? "Needs you" : x.state === "done" ? "Done" : x.state[0].toUpperCase() + x.state.slice(1)) + extra));
    const ctx = el("span", "ctx", x.context_tokens ? Math.round(x.context_tokens / 1000) + "k" : "");
    ctx.title = "Context size";
    const t = el("span", "t", ago(x.last_event_ms));
    t.dataset.ago = x.last_event_ms;
    const go = document.createElementNS(SVGNS, "svg");
    go.setAttribute("class", "go"); go.setAttribute("viewBox", "0 0 24 24");
    const path = document.createElementNS(SVGNS, "path");
    path.setAttribute("d", "M9 6l6 6-6 6"); path.setAttribute("fill", "none"); path.setAttribute("stroke", "currentColor");
    path.setAttribute("stroke-width", "2"); path.setAttribute("stroke-linecap", "round"); path.setAttribute("stroke-linejoin", "round");
    go.append(path);
    r.append(glyph(x.provider), el("span", "f", x.name || x.folder), s, ctx, t, go);
    pane.append(r);
  });
}

function renderUsage(pane) {
  const want = snap.settings.providers;
  const provs = want === "both" ? ["claude", "codex"] : [want];
  const g = el("div", "meters");
  provs.forEach((pv, i) => {
    const u = snap.usage.find((x) => x.provider === pv);
    const m = el("div", "meter");
    m.style.setProperty("--i", i);
    const h = el("div", "mh");
    h.append(glyph(pv), el("span", null, NAME[pv]));
    m.append(h);
    if (!u || !u.windows.length) {
      const why = !u ? "No usage data yet" : u.status === "needs_auth" ? "Sign in again: the saved Claude token has expired" : (u.message || u.status);
      m.append(el("div", "mnone", why));
    } else {
      if (u.status !== "ok") h.append(el("span", "st", u.message || u.status));
      const ws = el("div", "mw");
      for (const w of u.windows.slice(0, 2)) {
        const pct = Math.round(w.used_percent);
        const it = el("div", "mi");
        const txt = el("div");
        txt.append(el("b", null, pct + "%"), el("span", null, LABEL[w.label] || w.label), el("small", null, resetIn(w.resets_at)));
        const big = document.createElementNS(SVGNS, "svg");
        big.setAttribute("class", "big-ring"); big.setAttribute("viewBox", "0 0 44 44");
        const tr = document.createElementNS(SVGNS, "circle");
        tr.setAttribute("cx", 22); tr.setAttribute("cy", 22); tr.setAttribute("r", 20); tr.setAttribute("class", "tr");
        big.append(tr, gradArc(22, 20, 4, pct, !w.label.startsWith("7d")));
        it.append(big, txt);
        ws.append(it);
      }
      m.append(ws);
    }
    g.append(m);
  });
  pane.append(g);
}

// Requests: one card per request, built once (never re-rendered while it waits, so it never blinks).
const cards = new Map();
const picks = {};
function sendAnswer(id, answers) { invoke("answer_question", { id, answers }).catch(() => {}); delete picks[id]; }
function makeReq(a) {
  const c = el("div", "req p-" + a.provider);
  const h = el("div", "rh");
  const left = el("span", "left");
  h.append(glyph(a.provider), el("b", null, a.kind === "question" ? "Question" : `Allow ${a.tool_name || "tool"}?`), el("span", "w", a.name || a.folder || "session"), left);
  c.append(h);
  if (a.kind === "question") {
    const full = mode() === "full";
    const sel = picks[a.id] || (picks[a.id] = {});
    const qs = a.questions || [];
    const instant = !full && qs.length === 1 && !qs[0].multiSelect;
    const answers = () => { const o = {}; for (const q of qs) { const s = sel[q.question]; if (!s || !s.size) return null; o[q.question] = [...s].join(", "); } return o; };
    for (const q of qs) {
      const chosen = sel[q.question] || (sel[q.question] = new Set());
      const qt = el("div", "q");
      if (q.header) qt.append(el("span", "h", q.header));
      qt.append(document.createTextNode(q.question));
      const list = el("div", "opts");
      (q.options || []).forEach((o, i) => {
        const row = el("button", "opt");
        row.type = "button";
        const lab = el("span", "l");
        lab.append(el("span", null, o.label));
        if (o.description) lab.append(el("small", null, o.description));
        row.append(el("span", "n", String(i + 1)), lab);
        row.addEventListener("click", () => {
          if (q.multiSelect) chosen.has(o.label) ? chosen.delete(o.label) : chosen.add(o.label);
          else { chosen.clear(); chosen.add(o.label); }
          list.querySelectorAll(".opt").forEach((r, j) => r.classList.toggle("on", chosen.has(q.options[j].label)));
          if (instant) sendAnswer(a.id, answers());
        });
        list.append(row);
      });
      c.append(qt, list);
      if (full) {
        const other = el("input", "other");
        other.placeholder = "Other answer";
        other.addEventListener("input", () => {
          for (const l of [...chosen]) if (!(q.options || []).some((o) => o.label === l)) chosen.delete(l);
          if (other.value.trim()) { if (!q.multiSelect) chosen.clear(); chosen.add(other.value.trim()); }
        });
        other.addEventListener("blur", () => setTimeout(decide, 50));
        c.append(other);
      }
    }
    if (!instant) {
      const acts = el("div", "acts");
      acts.append(btn("Send", "btn primary", () => { const x = answers(); if (x) sendAnswer(a.id, x); }));
      if (full) acts.append(el("span", "grow"), btn("Answer in terminal", "link", () => sendAnswer(a.id, {})), btn("Jump", "link", () => invoke("jump_session", { key: a.session_key })));
      c.append(acts);
    }
  } else if (mode() !== "full") {
    // System prompts: one row. Mark, "Allow: command?", Allow and Deny beside it.
    c.classList.add("slim");
    h.remove();
    const what = summary(a);
    const t = el("span", "st", `Allow: ${what || a.tool_name || "tool"}?`);
    t.title = what;
    const answer = (d) => invoke("answer_permission", { id: a.id, decision: d }).catch(() => {});
    c.append(glyph(a.provider), t, el("span", "w", a.name || a.folder || "session"), left, btn("Allow", "btn primary", () => answer("allow")), btn("Deny", "btn quiet", () => answer("deny")));
  } else {
    const what = summary(a);
    if (what) c.append(el("div", "cmd", what));
    const answer = (d) => invoke("answer_permission", { id: a.id, decision: d }).catch(() => {});
    const acts = el("div", "acts");
    acts.append(btn("Allow", "btn primary", () => answer("allow")), btn("Deny", "btn quiet", () => answer("deny")));
    if (mode() === "full") acts.append(el("span", "grow"), btn("Jump", "link", () => invoke("jump_session", { key: a.session_key })));
    c.append(acts);
  }
  // Time left before the terminal takes over: a thin bar draining along the bottom edge.
  const bar = el("div", "timebar"), fill = el("i");
  bar.append(fill); c.append(bar);
  const total = Math.max(1, a.expires_ms - a.created_ms), rest = Math.max(0, a.expires_ms - Date.now());
  fill.style.transform = `scaleX(${rest / total})`;
  requestAnimationFrame(() => requestAnimationFrame(() => { fill.style.transition = `transform ${rest}ms linear`; fill.style.transform = "scaleX(0)"; }));
  return { el: c, left, a };
}
function renderRequests(pane, fresh) {
  const live = pending();
  const ids = new Set(live.map((a) => a.id));
  for (const [id, x] of cards) {
    if (ids.has(id)) continue;
    cards.delete(id);
    if (x.el.isConnected && !fresh) { x.el.style.animation = "none"; leave(x.el).then(() => x.el.remove()); }
  }
  for (const a of live) if (!cards.has(a.id)) cards.set(a.id, makeReq(a));
  if (fresh) {
    if (!live.length) { pane.append(el("div", "empty", "Nothing is waiting for you.")); return; }
    live.forEach((a, i) => { const x = cards.get(a.id); x.el.style.setProperty("--i", i); x.el.style.animation = ""; x.el.style.transform = x.el.style.opacity = x.el.style.filter = ""; pane.append(x.el); });
    return;
  }
  // In place: add new cards at the end, drop the empty note.
  pane.querySelector(".empty")?.remove();
  for (const a of live) { const x = cards.get(a.id); if (!x.el.isConnected) pane.append(x.el); }
  if (!live.length && !pane.querySelector(".empty")) setTimeout(() => { if (!pending().length && tab === "requests") renderPane(true); }, 320);
}
function tickCountdowns() {
  for (const x of cards.values()) x.left.textContent = Math.max(0, Math.round((x.a.expires_ms - Date.now()) / 1000)) + "s";
  document.querySelectorAll("[data-ago]").forEach((t) => (t.textContent = ago(Number(t.dataset.ago))));
}
setInterval(tickCountdowns, 1000);

let paneSig = "";
function renderPane(fresh) {
  const pane = $("pane");
  if (tab === "requests" && !fresh && pane.dataset.tab === "requests") { renderRequests(pane, false); tickCountdowns(); return; }
  const sig = tab === "sessions" ? JSON.stringify(snap.sessions.map((s) => [s.key, s.state, s.attention, s.name, s.folder, s.subagents, s.context_tokens]))
    : tab === "usage" ? JSON.stringify([snap.settings.providers, snap.usage.map((u) => [u.provider, u.status, u.windows.map((w) => Math.round(w.used_percent))])]) : "";
  if (!fresh && pane.dataset.tab === tab && sig === paneSig) return;
  paneSig = sig;
  pane.dataset.tab = tab;
  pane.replaceChildren();
  if (tab === "requests") renderRequests(pane, true);
  else if (tab === "sessions") renderSessions(pane);
  else renderUsage(pane);
  tickCountdowns();
}

function armFold() {
  clearTimeout(foldTimer);
  const secs = snap ? snap.settings.request_popup_secs : 8;
  if (!secs) return;
  foldTimer = setTimeout(() => { if (!hovering) { requestsUp = false; decide(); } }, secs * 1000);
}

let seen = new Set();
function setEdge(e) {
  if (!e || e === edge) return;
  edge = e;
  document.body.dataset.edge = edge;
  lastHot = "";
  requestAnimationFrame(() => snapTo(current));
}
function onState(s) {
  snap = s;
  // The page can load before the app has read its settings; every update carries the edge.
  setEdge(s.settings.edge);
  fetchOffset();
  const p = pending();
  const ids = new Set(p.map((a) => a.id));
  const fresh = p.some((a) => !seen.has(a.id));
  seen = ids;
  $("tn-requests").textContent = p.length || "";
  $("tn-requests").classList.toggle("hot", p.length > 0);
  $("tn-sessions").textContent = s.sessions.length || "";
  $("dnd").classList.toggle("on", !!s.dnd);
  moveHl(tabs.querySelector(`[data-tab="${tab}"]`)); // counts change tab widths
  renderCompact();
  // Top edge: a new request opens the island. Other edges: the toast shows it instead.
  if (fresh && !s.dnd && edge === "top") { setTab("requests"); requestsUp = true; armFold(); }
  if (!p.length && requestsUp) requestsUp = false;
  renderPane(false);
  decide();
}
listen("state", (e) => onState(e.payload));

// First paint: closed pill, no animation.
views.compact.classList.add("on");
shape.className = "compact";
invoke("get_state").then((s) => {
  onState(s);
  tabs.querySelectorAll("button").forEach((b) => b.classList.toggle("on", b.dataset.tab === tab));
  moveHl(tabs.querySelector(`[data-tab="${tab}"]`));
  snapTo("compact");
  decide();
}).catch((e) => invoke("log_js", { msg: "ui/island.js: get_state failed: " + e }));
