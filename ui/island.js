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
// Seconds left as 45s or 9:56.
function clock(ms) {
  const s = Math.ceil(ms / 1000);
  return s < 60 ? s + "s" : Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0");
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
  if (name === "alert") return [along, 56, 18, 12];
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
let dt = null, dtUi = null; // DeskTime snapshot and the live pane parts
let folded = false, foldTimer2 = 0; // "fold until hovered": thin tab after a short grace
let dragging = false;
let clicked = false; // fold mode: hover shows the pill, a click opens it
let sessionMenu = null;

function decide() {
  const typing = document.activeElement && document.activeElement.classList.contains("other");
  if (dragging) { show("compact"); return; }
  const hoverOpens = hovering && (clicked || !(snap && snap.settings.notch_on_hover));
  if (hoverOpens || requestsUp || typing || sessionMenu) { folded = false; clearTimeout(foldTimer2); show("open"); return; }
  if (alertUp) { show("alert"); return; }
  const canFold = snap && snap.settings.notch_on_hover && !pending().length && !missed().length;
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
  closeSessionMenu(false);
  tab = t;
  tabs.querySelectorAll("button").forEach((b) => b.classList.toggle("on", b.dataset.tab === t));
  moveHl(tabs.querySelector(`[data-tab="${t}"]`));
  dtChrome();
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
// Requests whose takeover time ran out: they wait in the terminal until dismissed or answered there.
const missed = () => (snap ? (snap.missed || []).filter(wanted) : []);
const waiting = () => [...pending(), ...missed()];
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
  svg.append(circ(11, "var(--notch-ink-12)", 2.6));
  if (h) svg.append(gradArc(15, 11, 2.6, h.used_percent, true));
  if (wk && wk !== h) { svg.append(circ(14.4, "var(--notch-ink-8)", 1.6)); svg.append(gradArc(15, 14.4, 1.6, wk.used_percent, false)); }
  if (act === "working") { const g = document.createElementNS(SVGNS, "g"); g.setAttribute("class", "spin"); g.append(circ(7.6, "var(--notch-activity)", 1.4, 0.28)); svg.append(g); }
  if (act === "attention") svg.append(circ(7.6, "var(--warn)", 1.4, null, "pulse"));
  const gl = glyph(provider);
  if (h && h.used_percent >= 100) gl.classList.add("dim");
  wrap.append(svg, gl);
  const pct = h ? Math.round(h.used_percent) : -1;
  box.append(wrap, el("span", "pct", pct >= 0 ? pct + "%" : u.status === "loading" ? "…" : "—"));
  // aria-label, not title: hovering opens the card, and the native tooltip then hung over its tabs.
  box.setAttribute("aria-label", `${NAME[provider]}: ${pct >= 0 ? pct + "% of session" : u.message || u.status}${wk ? ", " + Math.round(wk.used_percent) + "% of week" : ""}`);
  return { el: box, sig: [pct, wk ? Math.round(wk.used_percent) : -1, act, u.status].join() };
}
function renderCompact() {
  const left = $("c-left"), right = $("c-right");
  const p = pending();
  const m = missed();
  const working = snap.sessions.filter((s) => s.state === "working");
  const want = snap.settings.providers;
  const provs = want === "both" ? ["claude", "codex"] : [want];
  const L = [], R = [];
  const rings = provs.map(provRing);
  rings.forEach((r) => L.push(r.el));
  if (p.length) {
    L.push(el("span", "c-txt", p[0].kind === "question" ? "Question" : "Needs your OK"));
    R.push(el("span", "badge", String(p.length)));
  } else if (m.length) {
    // Number only (owner, 2026-11-04): no "in terminal" text. Click the number to open the terminal.
    const b = el("span", "badge", String(m.length));
    b.title = "Waiting in terminal. Click to open it";
    b.style.cursor = "pointer";
    b.addEventListener("click", (e) => { e.stopPropagation(); invoke("jump_session", { key: m[0].session_key }).catch(() => {}); });
    b.addEventListener("pointerdown", (e) => e.stopPropagation());
    const x = btn("×", "dismiss", () => invoke("dismiss_missed", { id: null }).catch(() => {}));
    x.title = "Dismiss";
    x.addEventListener("pointerdown", (e) => e.stopPropagation());
    R.push(b, x);
  }
  // DeskTime glance (Settings > DeskTime): only while nothing is waiting.
  const glance = !p.length && !m.length && dt && dt.glance ? dt.glance : "";
  if (glance) {
    const g = el("span", "c-dt", glance);
    g.setAttribute("aria-label", "DeskTime today: " + glance);
    R.push(g);
  }
  const sig = JSON.stringify([p.map((x) => x.id), m.map((x) => x.id), working.map((s) => s.key), rings.map((r) => r.sig), provs, edge, glance]);
  if (left.dataset.sig === sig) return;
  left.dataset.sig = sig;
  left.replaceChildren(...L);
  right.replaceChildren(...R);
}

function closeSessionMenu(restore = true) {
  if (!sessionMenu) return;
  const { box, row } = sessionMenu;
  sessionMenu = null;
  box.remove();
  views.open.classList.remove("has-session-menu");
  if (restore && row.isConnected) row.focus();
  decide();
}

function showSessionMenu(event, x, row) {
  event.preventDefault();
  event.stopPropagation();
  closeSessionMenu(false);
  const box = el("div", "session-menu");
  box.setAttribute("role", "menu");
  box.setAttribute("aria-label", `Actions for ${x.name || x.folder || NAME[x.provider]}`);
  const status = el("div", "session-menu-status");
  status.setAttribute("role", "status");
  sessionMenu = { box, row, key: x.key };
  views.open.classList.add("has-session-menu");
  views.open.append(box);

  function position() {
    const bounds = views.open.getBoundingClientRect();
    const keyboard = !Number.isFinite(event.clientX) || (event.clientX === 0 && event.clientY === 0);
    const rect = row.getBoundingClientRect();
    const px = keyboard ? rect.left + 12 : event.clientX;
    const py = keyboard ? rect.bottom : event.clientY;
    box.style.left = Math.max(8, Math.min(px - bounds.left, bounds.width - box.offsetWidth - 8)) + "px";
    box.style.top = Math.max(8, Math.min(py - bounds.top, bounds.height - box.offsetHeight - 8)) + "px";
  }
  async function run(command, args = {}) {
    const buttons = [...box.querySelectorAll("button")];
    buttons.forEach((b) => { b.disabled = true; });
    try {
      const result = await invoke(command, { key: x.key, ...args });
      if (command === "jump_session" && !result) throw new Error("Terminal window not found");
      closeSessionMenu();
    } catch (error) {
      if (!box.isConnected) return;
      status.textContent = String(error.message || error);
      buttons.forEach((b) => { b.disabled = false; });
      buttons[0]?.focus();
      position();
    }
  }
  function confirm(kind) {
    box.setAttribute("role", "alertdialog");
    box.setAttribute("aria-labelledby", "session-action-title");
    box.setAttribute("aria-describedby", "session-action-detail");
    const title = el("b", null, kind === "close" ? "Close this session?" : "Compact context?");
    title.id = "session-action-title";
    const detail = el("p", null, kind === "close"
      ? "Stops this agent and its running commands. The terminal window stays open."
      : "Sends /compact when this session is idle. Keep the terminal prompt empty until it runs.");
    detail.id = "session-action-detail";
    const actions = el("div", "session-menu-actions");
    const cancel = btn("Cancel", "", () => closeSessionMenu());
    actions.append(cancel, btn(kind === "close" ? "Close session" : "Queue compact", kind === "close" ? "danger" : "", () => run(kind === "close" ? "close_session" : "queue_session_compact", { queued: true })));
    box.replaceChildren(title, detail, status, actions);
    position();
    cancel.focus();
  }
  function item(label, fn, cls = "") {
    const b = btn(label, cls, fn);
    b.setAttribute("role", "menuitem");
    box.append(b);
    return b;
  }
  item("Jump", () => run("jump_session"));
  item("New terminal", () => run("new_session_terminal"));
  item(x.compact_queued ? "Cancel queued compact" : "Compact context", () => x.compact_queued ? run("queue_session_compact", { queued: false }) : confirm("compact"));
  item("Close", () => confirm("close"), "danger");
  box.append(status);
  box.addEventListener("keydown", (e) => {
    const buttons = [...box.querySelectorAll("button:not(:disabled)")];
    const index = buttons.indexOf(document.activeElement);
    let next;
    if (e.key === "ArrowDown") next = (index + 1) % buttons.length;
    else if (e.key === "ArrowUp") next = (index + buttons.length - 1) % buttons.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = buttons.length - 1;
    else if (e.key === "Tab" && box.getAttribute("role") === "alertdialog") next = (index + (e.shiftKey ? buttons.length - 1 : 1)) % buttons.length;
    else if (e.key === "Tab") { closeSessionMenu(); return; }
    if (next !== undefined && buttons.length) { e.preventDefault(); buttons[next].focus(); }
  });
  decide();
  position();
  requestAnimationFrame(() => { if (box.isConnected) position(); });
  box.querySelector("button").focus();
}

// The notch is an app surface; WebView navigation/print actions do not belong here.
document.addEventListener("contextmenu", (e) => e.preventDefault());
document.addEventListener("pointerdown", (e) => {
  if (sessionMenu && !sessionMenu.box.contains(e.target)) closeSessionMenu(false);
}, true);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && sessionMenu) { e.preventDefault(); closeSessionMenu(); }
});
window.addEventListener("blur", () => closeSessionMenu(false));
window.addEventListener("resize", () => closeSessionMenu());

function renderSessions(pane) {
  if (!snap.sessions.length) { pane.append(el("div", "empty", "No sessions yet. Start Claude Code or Codex.")); return; }
  const list = [...snap.sessions].sort((a, b) => b.last_event_ms - a.last_event_ms).slice(0, 8);
  list.forEach((x, i) => {
    const r = el("button", "srow");
    r.type = "button";
    r.dataset.sessionKey = x.key;
    r.setAttribute("aria-haspopup", "menu");
    r.title = "Open terminal";
    r.style.setProperty("--i", i);
    r.addEventListener("click", () => invoke("jump_session", { key: x.key }).catch(() => {}));
    r.addEventListener("contextmenu", (e) => showSessionMenu(e, x, r));
    r.addEventListener("keydown", (e) => {
      if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) showSessionMenu(e, x, r);
    });
    const s = el("span", "s");
    const extra = (x.subagents ? ` · ${x.subagents} agents` : "") + (x.compact_queued ? " · Compact queued" : "");
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
    if (x.action_error) {
      const error = el("div", "session-action-error", x.action_error);
      error.setAttribute("role", "status");
      pane.append(error);
    }
  });
}

const usageMeters = new Map();
const usageValues = new Map();
let usageRefresh = null;
const newestUsage = () => Math.max(0, ...(snap?.usage || []).map((u) => Number(u.fetched_ms) || 0));
function updateUsageLine() {
  document.querySelectorAll("[data-usage-provider]").forEach((node) => {
    const u = snap.usage.find((x) => x.provider === node.dataset.usageProvider);
    node.textContent = UsageStatus.stale(u) ? UsageStatus.text(u) : "";
  });
}
function stopUsageRefresh(failed) {
  const loading = usageRefresh;
  if (!loading || loading.stopping) return;
  loading.stopping = true;
  clearTimeout(loading.timeout);
  updateUsageLine();
  const finish = () => {
    if (usageRefresh !== loading) return;
    loading.animation?.cancel();
    $("refresh-usage").disabled = false;
    $("refresh-usage").classList.remove("loading");
    $("refresh-usage").setAttribute("aria-busy", "false");
    usageRefresh = null;
  };
  if (failed || !loading.animation || Motion.reduced.matches) { finish(); return; }
  const elapsed = Number(loading.animation.currentTime) || 0;
  const end = (Math.floor(elapsed / 700) + 1) * 700;
  loading.animation.effect.updateTiming({ iterations: end / 700 });
  loading.animation.finished.then(finish, finish);
}
$("refresh-usage").addEventListener("click", () => {
  if (tab === "desktime") { invoke("desktime_refresh").catch(() => {}); return; }
  if (usageRefresh) return;
  const button = $("refresh-usage");
  usageRefresh = { stamp: newestUsage() };
  button.disabled = true;
  button.classList.add("loading");
  button.setAttribute("aria-busy", "true");
  if (!Motion.reduced.matches) usageRefresh.animation = button.querySelector("svg").animate(
    [{ transform: "rotate(0deg)" }, { transform: "rotate(360deg)" }],
    { duration: 700, iterations: Infinity, easing: "linear" });
  usageRefresh.timeout = setTimeout(() => stopUsageRefresh(true), 15000);
  updateUsageLine();
  invoke("refresh_usage").catch(() => stopUsageRefresh(true));
});
function updateUsageMeters() {
  for (const u of snap.usage) for (const w of u.windows.slice(0, 2)) {
    const meter = usageMeters.get(u.provider + ":" + w.label);
    if (!meter) continue;
    meter.spring.to(Math.max(0, Math.min(100, w.used_percent)), PRESET.content);
    meter.anim.kick();
    meter.reset.textContent = resetIn(w.resets_at);
  }
  updateUsageLine();
}

function renderUsage(pane) {
  for (const meter of usageMeters.values()) cancelAnimationFrame(meter.anim.raf);
  usageMeters.clear();
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
      // Owner (2026-11-05): no "Updated just now"; only stale data gets an amber note.
      const status = el("span", "st warn", UsageStatus.stale(u) ? UsageStatus.text(u) : "");
      status.dataset.usageProvider = pv;
      h.append(status);
      const ws = el("div", "mw");
      for (const w of u.windows.slice(0, 2)) {
        const key = pv + ":" + w.label;
        const pct = usageValues.get(key) ?? Math.round(w.used_percent);
        const it = el("div", "mi");
        const label = LABEL[w.label] || w.label;
        const txt = el("div");
        const value = el("b", null, Math.round(pct) + "%");
        const pie = document.createElementNS(SVGNS, "svg");
        pie.setAttribute("class", "usage-pie");
        pie.setAttribute("viewBox", "0 0 44 44");
        const remainder = document.createElementNS(SVGNS, "circle");
        remainder.setAttribute("cx", "22");
        remainder.setAttribute("cy", "22");
        remainder.setAttribute("r", "21.5");
        remainder.setAttribute("fill", "var(--notch-ink-10)");
        const wedge = document.createElementNS(SVGNS, "path");
        pie.append(remainder, wedge);
        pie.setAttribute("role", "meter");
        pie.setAttribute("aria-label", NAME[pv] + " " + label + " usage");
        pie.setAttribute("aria-valuemin", "0");
        pie.setAttribute("aria-valuemax", "100");
        const reset = el("small", null, resetIn(w.resets_at));
        txt.append(value, el("span", null, label), reset);
        const spring = new Spring(pct, PRESET.content);
        const paint = (percent) => {
          const angle = percent / 100 * 2 * Math.PI;
          const x = 22 + 21.5 * Math.sin(angle), y = 22 - 21.5 * Math.cos(angle);
          const path = percent <= 0 ? "" : percent >= 100
            ? "M22 .5 A21.5 21.5 0 1 1 22 43.5 A21.5 21.5 0 1 1 22 .5 Z"
            : `M22 22 L22 .5 A21.5 21.5 0 ${percent > 50 ? 1 : 0} 1 ${x} ${y} Z`;
          wedge.setAttribute("d", path);
          wedge.setAttribute("fill", usageColor(percent, !w.label.startsWith("7d")));
          pie.setAttribute("aria-valuenow", String(Math.round(percent)));
          value.textContent = Math.round(percent) + "%";
        };
        paint(Math.max(0, Math.min(100, pct)));
        const meterAnim = new Animator({ pct: spring }, (values) => {
          const percent = Math.max(0, Math.min(100, values.pct));
          usageValues.set(key, percent);
          paint(percent);
        });
        usageMeters.set(key, { spring, anim: meterAnim, reset });
        it.append(pie, txt);
        ws.append(it);
      }
      m.append(ws);
    }
    g.append(m);
  });
  pane.append(g);
  updateUsageMeters();
}

// ---------- DeskTime ----------
// The snapshot comes from the "desktime" event. The pane is built once per mode and then updated in
// place, so a refresh never blinks: old numbers stay, changed ones are swapped and flash.
const DT_READ = { day: "Reading today", week: "Reading this week", month: "Reading this month" };
const DT_STEP_VIEW = [null, "day", "week", "month"]; // backend step number to view
const clockText = (ms) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
function setNum(node, text, flash) {
  if (node.textContent === text) return;
  node.textContent = text;
  if (!flash || Motion.reduced.matches) return;
  node.classList.remove("changed");
  void node.offsetWidth;
  node.classList.add("changed");
}
function dtChrome() {
  // The tab is always shown, so the owner can log in from the notch.
  const show = !dt || dt.show !== false;
  $("tab-desktime").hidden = !show;
  if (!show && tab === "desktime") { setTab("usage"); return; }
  const button = $("refresh-usage"), here = tab === "desktime";
  button.classList.toggle("dt-spin", here && !!(dt && dt.loading));
  const label = here ? "Refresh DeskTime now" : "Refresh usage now";
  button.title = label;
  button.setAttribute("aria-label", label);
  moveHl(tabs.querySelector(`[data-tab="${tab}"]`)); // the tab appears or disappears
}
function onDesktime(s) {
  dt = s;
  dtChrome();
  const pane = $("pane");
  if (tab === "desktime" && pane.dataset.tab === "desktime") paintDesktime(pane);
  if (snap) renderCompact(); // the glance chip
}
// actions: [label, fn] pairs; the first one is the main button.
function dtEmpty(pane, title, text, ...actions) {
  const box = el("div", "dt-empty"), row = el("div", "dt-actions");
  row.append(...actions.map(([label, fn], i) => btn(label, "dt-btn" + (i ? " ghost" : ""), fn)));
  box.append(el("b", null, title), el("p", null, text), row);
  pane.replaceChildren(box);
}
function dtSkeleton(pane, d) {
  const shown = new Set(d.views && d.views.length ? d.views : ["day", "week", "month"]);
  const fetch = d.fetch && d.fetch.length ? d.fetch : ["day", "week", "month"];
  const line = (w, h, m) => { const x = el("div", "sk"); x.style.cssText = `width:${w};height:${h}px;${m || ""}`; return x; };
  const root = el("div", "dt");
  root.setAttribute("aria-busy", "true");
  const day = el("div", "dt-day");
  const rows = el("div", "dt-rows");
  for (let i = 0; i < 4; i++) { const c = el("div"); c.append(line("48px", 10, "margin-bottom:6px"), line("64px", 14)); rows.append(c); }
  day.append(line("96px", 28), line("140px", 12), el("div", "dt-bar"), rows);
  const n = ["week", "month"].filter((v) => shown.has(v)).length;
  const period = el("div", "dt-period" + (n === 1 ? " one" : ""));
  for (let i = 0; i < n; i++) {
    const p = el("div", "dt-p");
    p.append(line("60px", 12, "margin-bottom:12px"));
    for (let j = 0; j < 4; j++) p.append(line("100%", 10, "margin:8px 0"));
    period.append(p);
  }
  const steps = el("div", "dt-steps");
  const pips = el("span", "pips");
  const labels = ["Opening DeskTime", ...fetch.map((v) => DT_READ[v])];
  for (let i = 0; i < labels.length; i++) pips.append(el("i"));
  steps.append(pips, el("span", "msg"));
  if (shown.has("day")) root.append(day);
  if (n) root.append(period);
  root.append(steps);
  pane.replaceChildren(root);
  return { root, steps, pips, labels, fetch, msg: steps.querySelector(".msg"), shown: -1, sig: fetch.join() + "|" + (d.views || []).join() };
}
// Rows per card and the grey notes are the owner's choice (Settings > DeskTime > Notch tab).
// Late arrival hides while it is None.
const dtRows = (view) => (snap && snap.settings["dt_rows_" + view]) || (view === "day" ? ["done", "finish"] : ["balance", "late"]);
const dtNotes = () => !!(snap && snap.settings.dt_notes);
function dtBuild(pane, d) {
  const root = el("div", "dt dt-live" + (dtNotes() ? " notes" : ""));
  root.append(el("div", "dt-line"));
  const ui = { root, rows: new Map(), views: new Map(), periods: new Map() };
  const ds = d.sections.find((s) => s.view === "day");
  if (ds) {
    const card = el("div", "dt-day");
    const big = el("div", "dt-big"), bigNum = el("span", "num");
    big.append(bigNum);
    const cap = el("div", "dt-cap");
    const bar = el("div", "dt-bar"), fill = el("i");
    bar.append(fill);
    bar.setAttribute("role", "meter");
    bar.setAttribute("aria-label", "Productive time against target");
    bar.setAttribute("aria-valuemin", "0");
    bar.setAttribute("aria-valuemax", "100");
    const rows = el("div", "dt-rows");
    for (const r of ds.rows.filter((r) => dtRows("day").includes(r.key))) {
      const cell = el("div", "dt-r"), v = el("b"), n = el("small");
      const num = el("span", "num");
      v.append(num);
      cell.append(el("span", null, r.label), v, n);
      rows.append(cell);
      ui.rows.set("day." + r.key, { num, note: n });
    }
    card.append(big, cap, bar, rows);
    root.append(card);
    ui.day = { big, bigNum, cap, bar, fill };
  }
  const periods = d.sections.filter((s) => s.view !== "day");
  if (periods.length) {
    // A lone week or month card spans the full width with its rows in two columns.
    const wide = periods.length === 1;
    const wrap = el("div", "dt-period" + (wide ? " one" : ""));
    for (const s of periods) {
      const card = el("div", "dt-p" + (wide ? " wide" : ""));
      const list = el("div", "dt-list");
      const head = el("div", "mh"), note = el("span", "st");
      head.append(el("span", null, s.title), note);
      // Same top block as v5: remaining (or overtime), then progress with a pace marker.
      const big = el("div", "dt-big"), bigNum = el("span", "num"), cap = el("span", "dt-cap");
      big.append(bigNum, document.createTextNode(" "), cap);
      const bar = el("div", "dt-bar"), fill = el("i"), pace = el("u");
      bar.append(fill, pace);
      bar.setAttribute("role", "meter");
      bar.setAttribute("aria-label", s.title + ": productive time against target");
      bar.setAttribute("aria-valuemin", "0");
      bar.setAttribute("aria-valuemax", "100");
      card.append(head, big, bar);
      ui.periods.set(s.view, { big, bigNum, cap, bar, fill, pace });
      for (const r of s.rows.filter((r) => dtRows(s.view).includes(r.key))) {
        const kv = el("div", "dt-kv"), v = el("b"), num = el("span", "num"), n = el("small");
        v.append(num, document.createTextNode(" "), n);
        kv.append(el("span", null, r.label), v);
        list.append(kv);
        ui.rows.set(s.view + "." + r.key, { num, note: n, b: v, kv });
      }
      card.append(list);
      wrap.append(card);
      ui.views.set(s.view, note);
    }
    root.append(wrap);
  }
  ui.foot = el("div", "dt-foot");
  ui.foot.setAttribute("role", "status");
  root.append(ui.foot);
  pane.replaceChildren(root);
  return ui;
}
function dtFill(ui, d, first) {
  ui.root.classList.toggle("busy", !!d.loading);
  for (const s of d.sections) {
    if (s.view === "day" && ui.day) {
      const x = ui.day;
      setNum(x.bigNum, s.big, !first);
      x.big.classList.toggle("over", s.over);
      x.cap.textContent = s.caption;
      x.bar.setAttribute("aria-valuenow", String(Math.round(s.percent)));
      // Two frames so the bar animates from its old width (or from empty on first paint).
      requestAnimationFrame(() => requestAnimationFrame(() => x.fill.style.setProperty("--p", s.percent + "%")));
    } else if (ui.views.has(s.view)) {
      ui.views.get(s.view).textContent = s.note;
      const x = ui.periods.get(s.view);
      setNum(x.bigNum, s.big, !first);
      x.big.classList.toggle("over", s.over);
      x.cap.textContent = s.caption;
      x.bar.setAttribute("aria-valuenow", String(Math.round(s.percent)));
      x.pace.hidden = s.pace == null;
      if (s.pace != null) { x.pace.style.left = s.pace + "%"; }
      requestAnimationFrame(() => requestAnimationFrame(() => x.fill.style.setProperty("--p", s.percent + "%")));
    }
    for (const r of s.rows) {
      const cell = ui.rows.get(s.view + "." + r.key);
      if (!cell) continue;
      setNum(cell.num, r.value, !first);
      cell.note.textContent = r.note;
      // Late arrival shows only when there is an allowance to use.
      if (r.key === "late") (cell.kv || cell.num.closest(".dt-r")).hidden = r.value === "None";
      if (cell.b) cell.b.className = r.tone;
    }
  }
  // Owner (2026-11-05): no footer while all is well; Log out and status live in Settings > DeskTime.
  // A failed refresh shows one readable line, so keyboard and screen-reader users get it too.
  ui.foot.hidden = !d.error;
  ui.foot.replaceChildren(...(d.error ? [el("span", "dt-state warn", `Couldn't refresh. Showing data from ${clockText(d.updated_ms)}.`)] : []));
}
function paintDesktime(pane) {
  const d = dt;
  if (!d) return;
  const mode = !d.enabled ? "off" : d.logged_out ? "out" : d.sections.length ? "data" : d.error && !d.loading ? "error" : "load";
  // Two ways in: DeskTime's login page in a Nubi window, or the browser plus the Nubi extension.
  const signIn = ["Log in with email", () => invoke("desktime_login").catch(() => {})];
  const getExt = ["Get browser extension", () => invoke("desktime_open", { target: "extension_download" }).catch(() => {})];
  const how = "Log in with your DeskTime email, or install the Nubi extension, log in to DeskTime in your browser and click Allow access.";
  if (mode === "off") {
    if (pane.dataset.dtmode !== "off") dtEmpty(pane, "DeskTime is inactive", how, signIn, getExt);
  } else if (mode === "out") {
    if (pane.dataset.dtmode !== "out") dtEmpty(pane, "Signed out of DeskTime", how, signIn, getExt);
  } else if (mode === "error") {
    if (pane.dataset.dtmode !== "error") dtEmpty(pane, "Couldn't read DeskTime", d.error, ["Try again", () => invoke("desktime_refresh").catch(() => {})]);
  } else if (mode === "load") {
    const skSig = (d.fetch || []).join() + "|" + (d.views || []).join();
    if (pane.dataset.dtmode !== "load" || !dtUi?.steps?.isConnected || dtUi.sig !== skSig) dtUi = dtSkeleton(pane, d);
    const view = DT_STEP_VIEW[d.step || 0];
    const i = view ? Math.max(0, dtUi.fetch.indexOf(view) + 1) : 0;
    if (dtUi.shown !== i) {
      dtUi.shown = i;
      dtUi.pips.querySelectorAll("i").forEach((p, j) => p.classList.toggle("on", j <= i));
      dtUi.msg.textContent = dtUi.labels[i] + "…";
      dtUi.msg.style.animation = "none"; void dtUi.msg.offsetWidth; dtUi.msg.style.animation = "";
    }
  } else {
    const sig = d.sections.map((s) => s.view + ":" + s.rows.map((r) => r.key) + ":" + dtRows(s.view)).join("|") + "|" + dtNotes();
    const fresh = pane.dataset.dtmode !== "data" || !dtUi?.root?.isConnected || dtUi.sig !== sig;
    if (fresh) { dtUi = dtBuild(pane, d); dtUi.sig = sig; }
    dtFill(dtUi, d, fresh);
  }
  pane.dataset.dtmode = mode;
}

// Requests: one card per request, built once (never re-rendered while it waits, so it never blinks).
const cards = new Map();
const picks = {};
function sendAnswer(id, answers) { invoke("answer_question", { id, answers }).catch(() => {}); delete picks[id]; }
function questionBody(a, body) {
  const sel = picks[a.id] || (picks[a.id] = {});
  const qs = a.questions || [];
  let index = 0, submitted = false;
  const step = el("div", "question-step");
  body.append(step);
  function draw() {
    step.replaceChildren();
    const q = qs[index];
    if (!q) return;
    const chosen = sel[q.question] || (sel[q.question] = new Set());
    const advance = () => {
      if (!chosen.size || submitted) return;
      if (index + 1 < qs.length) { index++; draw(); return; }
      const answers = {};
      for (const question of qs) answers[question.question] = [...sel[question.question]].join(", ");
      submitted = true;
      step.querySelectorAll("button,input").forEach((node) => { node.disabled = true; });
      sendAnswer(a.id, answers);
    };
    const qt = el("div", "q");
    if (qs.length > 1) qt.append(el("span", "h", `${index + 1} of ${qs.length}`));
    if (q.header) qt.append(el("span", "h", q.header));
    qt.append(document.createTextNode(q.question));
    const list = el("div", "opts");
    (q.options || []).forEach((o, i) => {
      const row = btn("", "opt", () => {
        if (q.multiSelect) chosen.has(o.label) ? chosen.delete(o.label) : chosen.add(o.label);
        else { chosen.clear(); chosen.add(o.label); }
        list.querySelectorAll(".opt").forEach((node, j) => {
          node.classList.toggle("on", chosen.has(q.options[j].label));
          node.setAttribute("aria-pressed", String(chosen.has(q.options[j].label)));
        });
        if (!q.multiSelect) advance();
      });
      row.setAttribute("aria-pressed", "false");
      const label = el("span", "l");
      label.append(el("b", null, o.label));
      if (o.description) label.append(el("small", null, o.description));
      row.append(el("span", "n", String(i + 1)), label);
      list.append(row);
    });
    const other = el("input", "other");
    other.placeholder = "Other answer";
    other.setAttribute("aria-label", "Your answer: " + q.question);
    const updateTyped = () => {
      for (const label of [...chosen]) if (!(q.options || []).some((o) => o.label === label)) chosen.delete(label);
      if (other.value.trim()) { if (!q.multiSelect) chosen.clear(); chosen.add(other.value.trim()); }
    };
    other.addEventListener("input", updateTyped);
    other.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" || event.isComposing || !other.value.trim()) return;
      event.preventDefault(); updateTyped(); advance();
    });
    other.addEventListener("blur", () => setTimeout(decide, 50));
    step.append(qt, list, other);
    if (q.multiSelect) {
      const acts = el("div", "acts");
      acts.append(btn("Done", "btn quiet", advance));
      step.append(acts);
    }
    if (index > 0) step.querySelector("button,input")?.focus({ preventScroll: true });
  }
  draw();
  if (mode() === "full") {
    const acts = el("div", "acts");
    acts.append(btn("Answer in terminal", "link", () => sendAnswer(a.id, {})),
      btn("Open terminal", "link", () => invoke("jump_session", { key: a.session_key })));
    body.append(acts);
  }
}
// Hold locally immediately; snapshots also carry holds from the other surface.
function requestClock(c, a) {
  const state = { a, total: Math.max(1, a.expires_ms - a.created_ms), over: false, held: false, left: Math.max(0, a.expires_ms - Date.now()) };
  const fill = c.querySelector(".timebar i");
  function tick() {
    const held = state.over || state.a.held;
    if (!state.held) state.left = Math.max(0, state.a.expires_ms - Date.now());
    state.held = held;
    const n = c.querySelector(".count, .left");
    if (n) n.textContent = clock(state.left);
    if (fill) {
      fill.style.transition = "none";
      fill.style.transform = `scaleX(${Math.min(1, state.left / state.total)})`;
    }
  }
  function hold(on) {
    if (state.over === on) return;
    if (on) tick();
    state.over = on;
    invoke("hold_pending", { id: state.a.id, on }).catch(() => {});
    tick();
  }
  c.addEventListener("pointerenter", () => hold(true));
  c.addEventListener("pointerleave", () => hold(false));
  state.tick = tick;
  state.release = () => hold(false);
  tick();
  return state;
}
function makeReq(a) {
  const c = el("div", "req p-" + a.provider);
  const h = el("div", "rh");
  const left = el("span", "left");
  h.append(glyph(a.provider), el("b", null, a.kind === "question" ? "Question" : `Allow ${a.tool_name || "tool"}?`), el("span", "w", a.name || a.folder || "session"), left);
  c.append(h);
  if (a.kind === "question") {
    questionBody(a, c);
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
    if (mode() === "full") acts.append(el("span", "grow"), btn("Open terminal", "link", () => invoke("jump_session", { key: a.session_key })));
    c.append(acts);
  }
  // Time left before the terminal takes over: a thin bar draining along the bottom edge.
  const bar = el("div", "timebar"), fill = el("i");
  bar.append(fill); c.append(bar);
  return { el: c, left, a, clock: requestClock(c, a) };
}
function renderTerminalRequests(pane) {
  const items = missed();
  let list = pane.querySelector(".terminal-requests");
  if (!items.length) { list?.remove(); return; }
  if (!list) { list = el("div", "terminal-requests"); pane.append(list); }
  const sig = JSON.stringify(items);
  if (list.dataset.sig === sig) return;
  list.dataset.sig = sig;
  list.replaceChildren(...items.map((a) => {
    const row = el("div", "req slim");
    row.append(glyph(a.provider), el("span", "st", a.kind === "question" ? "Question waiting in terminal" : "Permission waiting in terminal"),
      el("span", "w", a.name || a.folder || "session"),
      btn("Open terminal", "btn primary", () => invoke("jump_session", { key: a.session_key }).catch(() => {})),
      btn("Dismiss", "btn quiet", () => invoke("dismiss_missed", { id: a.id }).catch(() => {})));
    return row;
  }));
}
function renderRequests(pane, fresh) {
  const live = pending();
  const ids = new Set(live.map((a) => a.id));
  for (const [id, x] of cards) {
    if (ids.has(id)) continue;
    x.clock.release();
    cards.delete(id);
    if (x.el.isConnected && !fresh) { x.el.style.animation = "none"; leave(x.el).then(() => x.el.remove()); }
  }
  for (const a of live) {
    if (!cards.has(a.id)) cards.set(a.id, makeReq(a));
    cards.get(a.id).a = a; cards.get(a.id).clock.a = a;
  }
  if (fresh) {
    if (!waiting().length) { pane.append(el("div", "empty", "Nothing is waiting for you")); return; }
    live.forEach((a, i) => { const x = cards.get(a.id); x.el.style.setProperty("--i", i); x.el.style.animation = ""; x.el.style.transform = x.el.style.opacity = x.el.style.filter = ""; pane.append(x.el); });
    renderTerminalRequests(pane);
    return;
  }
  // In place: add new cards at the end, drop the empty note.
  pane.querySelector(".empty")?.remove();
  for (const a of live) { const x = cards.get(a.id); if (!x.el.isConnected) pane.append(x.el); }
  renderTerminalRequests(pane);
  if (!waiting().length && !pane.querySelector(".empty")) setTimeout(() => { if (!waiting().length && tab === "requests") renderPane(true); }, 320);
}
function tickCountdowns() {
  for (const x of cards.values()) x.clock.tick();
  if (tab === "usage" && snap) { updateUsageLine(); for (const u of snap.usage) for (const w of u.windows.slice(0, 2)) { const meter = usageMeters.get(u.provider + ":" + w.label); if (meter) meter.reset.textContent = resetIn(w.resets_at); } }
  document.querySelectorAll("[data-ago]").forEach((t) => (t.textContent = ago(Number(t.dataset.ago))));
}
setInterval(tickCountdowns, 1000);

let paneSig = "";
function renderPane(fresh) {
  const pane = $("pane");
  if (tab === "requests" && !fresh && pane.dataset.tab === "requests") { renderRequests(pane, false); tickCountdowns(); return; }
  if (tab === "desktime") {
    if (fresh || pane.dataset.tab !== "desktime") {
      for (const x of cards.values()) x.clock.release();
      pane.replaceChildren();
      pane.dataset.tab = "desktime"; pane.dataset.dtmode = ""; paneSig = ""; dtUi = null;
    }
    paintDesktime(pane);
    return;
  }
  const sig = tab === "sessions" ? JSON.stringify(snap.sessions.map((s) => [s.key, s.state, s.attention, s.name, s.folder, s.subagents, s.context_tokens, s.compact_queued, s.action_error]))
    : tab === "usage" ? JSON.stringify([snap.settings.providers, snap.usage.map((u) => [u.provider, u.status, u.message, u.windows.slice(0, 2).map((w) => w.label)])]) : "";
  if (!fresh && pane.dataset.tab === tab && sig === paneSig) { if (tab === "usage") updateUsageMeters(); return; }
  paneSig = sig;
  pane.dataset.tab = tab;
  for (const x of cards.values()) x.clock.release();
  pane.replaceChildren();
  if (tab === "requests") renderRequests(pane, true);
  else if (tab === "sessions") renderSessions(pane);
  else renderUsage(pane);
  if (sessionMenu) {
    const row = [...pane.querySelectorAll(".srow")].find((r) => r.dataset.sessionKey === sessionMenu.key);
    if (row) sessionMenu.row = row;
    else closeSessionMenu(false);
  }
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
  if (usageRefresh && newestUsage() > usageRefresh.stamp) stopUsageRefresh(false);
  const liveIds = new Set((s.pending || []).map((a) => a.id));
  for (const [id, card] of cards) {
    const data = (s.pending || []).find((a) => a.id === id);
    if (data) card.clock.a = card.a = data;
    if (!liveIds.has(id)) card.clock.release();
  }
  // The page can load before the app has read its settings; every update carries the edge.
  setEdge(s.settings.edge);
  fetchOffset();
  const p = waiting();
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
listen("desktime", (e) => onDesktime(e.payload));
invoke("desktime_state").then(onDesktime).catch(() => {});

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
