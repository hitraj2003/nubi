window.addEventListener("error", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/overlay.js: ${e.message} @${e.lineno}` }));
window.addEventListener("unhandledrejection", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/overlay.js: ${e.reason}` }));
// Notification island. Alerts: Navi's pill (Apache-2.0, e8e0c40) with its queue, icons and sounds.
// Requests (approvals, Claude questions): cards in the same material under the island, as a deck
// ("stack") or side by side ("list"). Cards fold away after a few seconds; the request stays in the
// panel Inbox and comes back with show_requests. All dynamic text goes through textContent.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const ICONS = {
  check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>',
  warning: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>',
  info: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>',
};
const KIND = {
  done: { type: "success", icon: "check", sound: "success" },
  permission: { type: "warning", icon: "warning", sound: "error" },
  question: { type: "info", icon: "info", sound: "success" },
  idle: { type: "info", icon: "info", sound: "success" },
  info: { type: "info", icon: "info", sound: "success" },
};
const NAME = { claude: "Claude", codex: "Codex" };

// Sounds: one per alert kind, from sounds.js (window.chime).

// ---------- helpers ----------
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
const secsLeft = (a) => Math.max(0, Math.round((a.expires_ms - Date.now()) / 1000));

let snap = null;
const stage = document.getElementById("stage");
const island = document.getElementById("island");
const deck = document.getElementById("deck");

// ---------- alert pill (Navi queue) ----------
const queue = [];
let pillShowing = false;
// Pill shape on springs: width, height, scale, opacity. Opacity settles faster, like a crossfade.
const { Spring, Animator, PRESET } = Motion;
const FADE = { response: 0.3, damping: 1 };
const islandContent = island.querySelector(".island-content");
const pill = { w: new Spring(120), h: new Spring(0), s: new Spring(0.8), o: new Spring(0) };
const pillAnim = new Animator(pill, (v) => {
  island.style.width = v.w + "px";
  island.style.height = Math.max(0, v.h) + "px";
  island.style.transform = `scale(${v.s})`;
  island.style.opacity = String(Math.max(0, Math.min(1, v.o)));
  island.style.marginBottom = -8 * (1 - Math.max(0, Math.min(1, v.h / 56))) + "px";
});
function pillWidth() {
  // Natural width of the new content, read in one synchronous pass (nothing paints in between).
  const old = island.style.width;
  island.style.width = "max-content";
  const w = island.offsetWidth;
  island.style.width = old;
  return Math.min(480, w);
}
function pillOpen() {
  island.classList.add("expanded");
  pill.w.to(pillWidth(), PRESET.open); pill.h.to(56, PRESET.open); pill.s.to(1, PRESET.open); pill.o.to(1, FADE);
  pillAnim.kick();
  islandContent.style.opacity = "0";
  setTimeout(() => Motion.enter(islandContent), 100);
}
function pillClose() {
  island.classList.remove("expanded");
  Motion.leave(islandContent);
  pill.w.to(120, PRESET.close); pill.h.to(0, PRESET.close); pill.s.to(0.8, PRESET.close); pill.o.to(0, FADE);
  pillAnim.kick();
}
function showNext() {
  if (!queue.length) { pillShowing = false; settle(); return; }
  pillShowing = true;
  const a = queue.shift();
  const k = KIND[a.kind] || KIND.info;
  document.getElementById("icon").innerHTML = ICONS[k.icon];
  document.getElementById("title").textContent = `${NAME[a.provider] || "Claude"} · ${a.title}`;
  document.getElementById("message").textContent = (a.body || "").slice(0, 200);
  island.className = "island type-" + k.type;
  pillOpen();
  reportHot();
  setTimeout(() => {
    pillClose();
    setTimeout(showNext, 600);
  }, 4500);
}
listen("pill", (e) => {
  const a = e.payload;
  const k = KIND[a.kind] || KIND.info;
  if (a.kind === "permission" || a.kind === "question") {
    // A card shows this one; the pill is only for requests the overlay cannot answer.
    setTimeout(() => {
      const hasCard = snap && (snap.pending || []).some((p) => p.session_key === a.session_key);
      if (a.sound) chime(a.kind);
      if (!hasCard) { queue.push(a); if (!pillShowing) showNext(); }
    }, 400);
    return;
  }
  if (a.sound) chime(a.kind);
  queue.push(a);
  if (!pillShowing) showNext();
});
island.addEventListener("click", () => invoke("show_panel"));

// ---------- request cards ----------
const cards = new Map(); // id -> { el, data }
const dismissed = new Set();
const picks = {}; // id -> { question: Set(labels) }
let hovering = false, foldTimer = null;

function mode() { return (snap && snap.settings.questions_mode) || "system"; }
function wanted(p) { const w = snap.settings.providers; return w === "both" || w === p.provider; }

function header(a) {
  const h = el("div", "head");
  const k = KIND[a.kind] || KIND.info;
  const ic = el("div", "icon");
  ic.innerHTML = ICONS[k.icon];
  h.classList.add("type-" + k.type);
  const title = a.kind === "question" ? "Question" : `Allow ${a.tool_name || "tool"}?`;
  const t = el("div", "t", title);
  const m = el("div", "m", `${NAME[a.provider]} · ${a.name || a.folder || "session"}`);
  h.append(ic, t, m);
  return h;
}

function send(id, answers) {
  invoke("answer_question", { id, answers }).catch(() => {});
  delete picks[id];
}

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
      send(a.id, answers);
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
    acts.append(btn("Answer in terminal", "link", () => send(a.id, {})),
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
    if (n) n.textContent = `Terminal takes over in ${Math.ceil(state.left / 1000)}s`;
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

function permissionBody(a, body) {
  if (a.tool_input) body.append(el("div", "cmd", a.tool_input));
  const acts = el("div", "acts");
  const answer = (d) => invoke("answer_permission", { id: a.id, decision: d }).catch(() => {});
  acts.append(btn("Allow", "btn primary", () => answer("allow")), btn("Deny", "btn quiet", () => answer("deny")));
  if (mode() === "full") acts.append(el("div", "grow"), btn("Open terminal", "link", () => invoke("jump_session", { key: a.session_key })));
  body.append(acts);
}

const glyphs = {};
for (const p of ["claude", "codex"]) {
  fetch("glyphs/" + p + ".svg").then((r) => r.text()).then((t) => { glyphs[p] = t.replace(/<metadata>[\s\S]*?<\/metadata>/, ""); }).catch(() => {});
}
// Short text for a tool call: the command, path or first value of its input.
function summary(a) {
  try {
    const v = JSON.parse(a.tool_input || "{}");
    const s = v.command || v.file_path || v.path || v.url || v.pattern || Object.values(v).find((x) => typeof x === "string") || "";
    return String(s).replace(/\s+/g, " ").trim();
  } catch (_) { return ""; }
}
// System-prompt approval: one slim pill. Dot, "Allow: npm install stripe?", Allow, Deny, provider mark.
function makeSlim(a) {
  const c = el("div", "card slim");
  c.dataset.id = a.id;
  const what = summary(a);
  const t = el("span", "st", `Allow: ${what || a.tool_name || "tool"}?`);
  t.title = what;
  const answer = (d) => invoke("answer_permission", { id: a.id, decision: d }).catch(() => {});
  const mark = el("span", "pm");
  mark.innerHTML = glyphs[a.provider] || "";
  mark.title = `${NAME[a.provider]} · ${a.name || a.folder || "session"}`;
  c.append(el("span", "sdot"), t, el("span", "sw", a.name || a.folder || "session"), btn("Allow", "sbtn ok", () => answer("allow")), btn("Deny", "sbtn no", () => answer("deny")), mark);
  return c;
}

function makeCard(a) {
  if (a.kind !== "question" && mode() !== "full") return makeSlim(a);
  const c = el("div", "card in");
  c.dataset.id = a.id;
  c.append(header(a));
  const body = el("div", "body");
  if (a.kind === "question") questionBody(a, body); else permissionBody(a, body);
  const foot = el("div", "acts");
  foot.append(el("span", "count"));
  body.append(foot);
  c.append(body);
  return c;
}

function updateCountdown(c, a) {
  const x = cards.get(a.id);
  if (x?.clock) { x.clock.a = a; x.clock.tick(); }
}
function addTimeBar(c) {
  const bar = el("div", "timebar");
  bar.append(el("i")); c.append(bar);
}

// Each card rides its own springs (y, scale, opacity, blur). Layout only sets targets.
function cardSprings(c, id) {
  const sp = { y: new Spring(-6, PRESET.open), s: new Spring(0.8, PRESET.open), o: new Spring(0, FADE), b: new Spring(6, FADE) };
  const x = { el: c, sp };
  x.anim = new Animator(sp, (v) => {
    c.style.transform = `translateY(${v.y}px) scale(${v.s})`;
    c.style.opacity = String(Math.max(0, Math.min(1, v.o)));
    c.style.filter = v.b > 0.05 ? `blur(${v.b}px)` : "";
    if (x.leaving && !x.gone && Object.values(sp).every((p) => p.done)) {
      x.gone = true; c.remove(); cards.delete(id); layout(); settle();
    }
  });
  return x;
}
function aim(x, y, s, o) {
  if (x.leaving) return;
  x.sp.y.to(y, PRESET.open); x.sp.s.to(s, PRESET.open); x.sp.o.to(o, FADE); x.sp.b.to(0, FADE);
  x.anim.kick();
}
// Deck box in stack mode: height, width and top margin spring with the front card.
let deckStack = false;
const dk = { h: new Spring(0, PRESET.open), w: new Spring(0, PRESET.open), m: new Spring(0, PRESET.open) };
const deckAnim = new Animator(dk, (v) => {
  if (!deckStack) return;
  deck.style.height = Math.max(0, v.h) + "px"; deck.style.width = Math.max(0, v.w) + "px"; deck.style.marginTop = v.m + "px";
});

function layout() {
  const list = [...cards.values()].filter((x) => !x.leaving);
  const stack = (snap && snap.settings.request_layout) !== "list";
  deck.className = stack ? "stack" : "list";
  if (stack) {
    // Front = oldest. Up to three behind it, each a little higher and smaller.
    list.forEach((x, i) => {
      const depth = Math.min(i, 3);
      x.el.classList.toggle("back", i > 0);
      x.el.style.zIndex = String(100 - i);
      aim(x, -10 * depth, 1 - 0.05 * depth, i > 3 ? 0 : 1 - 0.2 * depth);
    });
    const front = list[0];
    // Hold the deck's size while a card leaves, so the leaving card does not drift sideways.
    if ([...cards.values()].some((x) => x.leaving)) return;
    if (!deckStack) { dk.h.jump(deck.offsetHeight); dk.w.jump(deck.offsetWidth); dk.m.jump(0); deckStack = true; }
    dk.h.to(front ? front.el.offsetHeight : 0); dk.w.to(front ? front.el.offsetWidth : 0);
    dk.m.to(list.length > 1 ? 10 * Math.min(list.length - 1, 3) : 0);
    deckAnim.kick();
  } else {
    if ([...cards.values()].some((x) => x.leaving)) return;
    list.forEach((x) => { x.el.classList.remove("back"); x.el.style.zIndex = ""; aim(x, 0, 1, 1); });
    deckStack = false;
    deck.style.height = deck.style.width = deck.style.marginTop = "";
  }
}

function renderCards() {
  if (!snap) return;
  const live = (snap.pending || []).filter(wanted);
  const liveIds = new Set(live.map((a) => a.id));
  for (const id of [...dismissed]) if (!liveIds.has(id)) dismissed.delete(id);
  const show = live.filter((a) => !dismissed.has(a.id));
  const showIds = new Set(show.map((a) => a.id));
  let added = false;
  // Leave
  for (const [id, x] of cards) {
    if (!showIds.has(id) && !x.leaving) {
      x.clock.release();
      x.leaving = true;
      x.el.classList.add("leave");
      // Leaves straight up, shrinking toward the island; nothing moves sideways.
      x.sp.y.to(-24, PRESET.close); x.sp.s.to(0.6, PRESET.close); x.sp.o.to(0, FADE); x.sp.b.to(4, FADE);
      x.anim.kick();
    }
  }
  // Enter (DOM is built once per request, so cards never flash on updates)
  for (const a of show) {
    const x = cards.get(a.id);
    if (x) { x.data = a; updateCountdown(x.el, a); continue; }
    const c = makeCard(a);
    c.classList.add("p-" + a.provider); // provider colours: Claude terracotta, Codex blue
    addTimeBar(c, a);
    deck.append(c);
    const nx = cardSprings(c, a.id);
    nx.data = a;
    nx.clock = requestClock(c, a);
    cards.set(a.id, nx);
    updateCountdown(c, a);
    added = true;
    setTimeout(() => c.classList.remove("in"), 700);
  }
  layout();
  if (added) armFold();
  settle();
}

function armFold() {
  clearTimeout(foldTimer);
  const secs = snap ? snap.settings.request_popup_secs : 8;
  if (!secs || hovering) return;
  foldTimer = setTimeout(() => {
    if (hovering) return;
    for (const id of cards.keys()) dismissed.add(id);
    renderCards();
  }, secs * 1000);
}

listen("overlay_pointer", (e) => {
  hovering = e.payload === true;
  if (hovering) clearTimeout(foldTimer); else armFold();
});
listen("show_requests", () => { dismissed.clear(); renderCards(); armFold(); });

// ---------- window bookkeeping ----------
function reportHot() {
  const k = window.devicePixelRatio || 1;
  const rects = [];
  if (island.classList.contains("expanded")) rects.push(island.getBoundingClientRect());
  for (const x of cards.values()) if (!x.leaving) rects.push(x.el.getBoundingClientRect());
  if (!rects.length) { invoke("report_overlay_hot", { rect: null }); return; }
  const r = rects.reduce((u, b) => ({ left: Math.min(u.left, b.left), top: Math.min(u.top, b.top), right: Math.max(u.right, b.right), bottom: Math.max(u.bottom, b.bottom) }),
    { left: Infinity, top: Infinity, right: -Infinity, bottom: -Infinity });
  invoke("report_overlay_hot", { rect: [r.left * k, r.top * k, (r.right - r.left) * k, (r.bottom - r.top) * k] });
}
let hideTimer = null;
function settle() {
  reportHot();
  clearTimeout(hideTimer);
  if (!pillShowing && !cards.size) hideTimer = setTimeout(() => { if (!pillShowing && !cards.size) invoke("hide_overlay"); }, 200);
}
new ResizeObserver(() => { layout(); reportHot(); }).observe(stage);

listen("state", (e) => { snap = e.payload; renderCards(); });
invoke("get_state").then((s) => { snap = s; renderCards(); });
setInterval(() => { if (!snap) return; for (const x of cards.values()) updateCountdown(x.el, x.data); }, 1000);
