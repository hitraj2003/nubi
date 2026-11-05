window.addEventListener("error", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/panel.js: ${e.message} @${e.lineno}` }));
window.addEventListener("unhandledrejection", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/panel.js: ${e.reason}` }));
// Panel. All dynamic text is set with textContent: tool input and questions are untrusted.
// Motion uses the same spring engine and presets as the notch (motion.js).
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { PRESET, Spring, Animator, enter, tween, slider, vslider, pressable } = window.Motion;
const $ = (id) => document.getElementById(id);
const NAME = { claude: "Claude", codex: "Codex" };

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}
function btn(text, fn, cls) { const b = el("button", "btn " + (cls || ""), text); b.type = "button"; b.addEventListener("click", fn); return b; }
function ago(ms) { const s = Math.max(0, Math.round((Date.now() - ms) / 1000)); return s < 60 ? s + "s ago" : s < 3600 ? Math.round(s / 60) + "m ago" : Math.round(s / 3600) + "h ago"; }
function resetIn(sec) {
  if (!sec) return "";
  const m = Math.round((sec * 1000 - Date.now()) / 60000);
  if (m <= 0) return "resetting";
  return "resets in " + (m < 60 ? m + "m" : m < 1440 ? Math.floor(m / 60) + "h " + (m % 60) + "m" : Math.round(m / 1440) + "d");
}
const tile = (p) => el("span", "ico " + p, p === "codex" ? "X" : "C");
const LABEL = { "5h": "Last 5 hours", "7d": "This week", "7d Opus": "This week, Opus", "7d Sonnet": "This week, Sonnet" };
const RING_LABEL = { "5h": "5-hour limit", "7d": "weekly limit" };
const STATE = { working: "Working", waiting: "Waiting", done: "Done", idle: "Idle" };

// Children come in one after another on the notch's content spring (blur + rise).
function stagger(nodes, step = 35) {
  [...nodes].forEach((n, i) => { n.style.opacity = "0"; setTimeout(() => enter(n, PRESET.content), i * step); });
}
pressable(document.body);

// ---- navigation: the rail tile slides between items ----
const railHl = vslider(document.querySelector(".rail-hl"));
function go(view) {
  document.querySelectorAll(".rail button[data-view]").forEach((x) => { const on = x.dataset.view === view; x.classList.toggle("on", on); on ? x.setAttribute("aria-current", "page") : x.removeAttribute("aria-current"); });
  railHl(document.querySelector(".rail button.on"));
  document.querySelectorAll(".view").forEach((v) => {
    const on = v.id === "v-" + view;
    if (on && !v.classList.contains("on")) {
      v.classList.add("on");
      enter(v, PRESET.content);
      stagger(v.querySelectorAll(".pane.on .card, .ses, .al, .meter, .item"));
    } else if (!on) v.classList.remove("on");
  });
  if (view === "settings") requestAnimationFrame(() => { sideHl(document.querySelector(".side button.on")); syncThumbs(); });
}
let leavingTo = null;
document.querySelectorAll(".rail button[data-view]").forEach((b) =>
  b.addEventListener("click", () => {
    if (formDirty && b.dataset.view !== "settings") { leavingTo = b.dataset.view; openLeave(); return; }
    go(b.dataset.view);
  }));
// Dialogs: focus moves in, Tab stays inside, Escape closes, focus goes back to the opener.
const openers = {};
function showModal(id, focus) { openers[id] = document.activeElement; $(id).hidden = false; (focus || $(id).querySelector("button, textarea, input")).focus(); }
function hideModal(id) { $(id).hidden = true; const o = openers[id]; openers[id] = null; if (o && o.isConnected) o.focus(); }
addEventListener("keydown", (e) => {
  const m = document.querySelector(".modal:not([hidden])");
  if (!m) return;
  if (e.key === "Escape") { e.preventDefault(); m.id === "leave" ? closeLeave() : hideModal(m.id); return; }
  if (e.key !== "Tab") return;
  const f = [...m.querySelectorAll("button, textarea, input, [tabindex]")].filter((x) => !x.disabled && x.offsetParent);
  if (!f.length) return;
  const i = f.indexOf(document.activeElement);
  if (e.shiftKey && i <= 0) { e.preventDefault(); f[f.length - 1].focus(); }
  else if (!e.shiftKey && (i === -1 || i === f.length - 1)) { e.preventDefault(); f[0].focus(); }
});
function openLeave() { showModal("leave", $("leave-save")); enter($("leave").querySelector(".box"), PRESET.open); }
const closeLeave = () => { hideModal("leave"); leavingTo = null; };
$("leave").addEventListener("click", (e) => { if (e.target === $("leave")) closeLeave(); });
requestAnimationFrame(() => railHl(document.querySelector(".rail button.on")));
// The settings button sits at the bottom of the rail, so it moves when the window resizes.
new ResizeObserver(() => railHl(document.querySelector(".rail button.on"))).observe(document.querySelector(".rail"));

// ---- settings sections: highlight slides, pane blurs in ----
const sideHl = vslider(document.querySelector(".side-hl"));
document.querySelectorAll(".side button[data-pane]").forEach((b) => b.addEventListener("click", () => {
  document.querySelectorAll(".side button[data-pane]").forEach((x) => { x.classList.toggle("on", x === b); x === b ? x.setAttribute("aria-current", "page") : x.removeAttribute("aria-current"); });
  sideHl(b);
  document.querySelectorAll(".pane").forEach((p) => {
    const on = p.dataset.pane === b.dataset.pane;
    p.classList.toggle("on", on);
    if (on) { enter(p, PRESET.content); stagger(p.querySelectorAll(".card")); }
  });
  requestAnimationFrame(syncThumbs);
}));

// ---- segmented controls: the thumb slides on the snappy spring ----
const thumbs = [];
document.querySelectorAll(".seg").forEach((seg) => {
  const t = el("i", "thumb"); seg.prepend(t);
  thumbs.push({ seg, t, move: slider(t) });
});
function syncThumbs() {
  for (const { seg, t, move } of thumbs) {
    const span = seg.querySelector("input:checked + span");
    if (!span || !span.offsetWidth) { t.style.opacity = "0"; continue; }
    t.style.opacity = "1";
    move(span.parentElement);
  }
}
addEventListener("resize", syncThumbs);

// ---- custom dropdowns: each <select> keeps the form value, a styled menu drives it ----
const CHEVRON = '<svg viewBox="0 0 24 24"><path d="m6 9 6 6 6-6"/></svg>';
const dropdowns = [];
function enhanceSelect(sel) {
  const dd = el("div", "dd");
  const b = el("button"); b.type = "button";
  const label = el("span");
  b.append(label); b.insertAdjacentHTML("beforeend", CHEVRON);
  const ul = el("ul"); ul.setAttribute("role", "listbox"); ul.id = "lb-" + sel.name + "-" + dropdowns.length;
  label.id = ul.id + "-v";
  b.setAttribute("aria-haspopup", "listbox"); b.setAttribute("aria-expanded", "false"); b.setAttribute("aria-controls", ul.id);
  if (sel.dataset.name) { const n = el("span", "sr", sel.dataset.name); n.id = ul.id + "-n"; b.prepend(n); }
  dd.append(b, ul);
  sel.after(dd);
  let act = -1;
  const items = () => [...ul.children];
  const mark = () => {
    items().forEach((li, i) => li.classList.toggle("act", i === act));
    act >= 0 && items()[act] ? b.setAttribute("aria-activedescendant", items()[act].id) : b.removeAttribute("aria-activedescendant");
  };
  const close = () => { dd.classList.remove("open"); b.setAttribute("aria-expanded", "false"); act = -1; mark(); };
  const pick = (v) => { sel.value = v; sync(); close(); sel.dispatchEvent(new Event("input", { bubbles: true })); b.focus(); };
  function sync() {
    ul.replaceChildren();
    for (const o of sel.options) {
      const li = el("li", o.value === sel.value ? "sel" : "", o.textContent);
      li.setAttribute("role", "option"); li.id = ul.id + "-" + ul.children.length;
      li.setAttribute("aria-selected", String(o.value === sel.value));
      li.addEventListener("click", () => pick(o.value));
      ul.append(li);
    }
    label.textContent = sel.selectedOptions[0] ? sel.selectedOptions[0].textContent : "";
    b.disabled = sel.disabled;
  }
  b.addEventListener("click", () => {
    const open = !dd.classList.contains("open");
    dropdowns.forEach((d) => d.close());
    if (open) { dd.classList.add("open"); b.setAttribute("aria-expanded", "true"); act = sel.selectedIndex; mark(); stagger(ul.children, 20); }
  });
  b.addEventListener("keydown", (e) => {
    if (!dd.classList.contains("open")) { if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); b.click(); } return; }
    const n = sel.options.length;
    if (e.key === "ArrowDown") { e.preventDefault(); act = (act + 1) % n; mark(); }
    else if (e.key === "ArrowUp") { e.preventDefault(); act = (act - 1 + n) % n; mark(); }
    else if (e.key === "Enter" || e.key === " ") { e.preventDefault(); if (act >= 0) pick(sel.options[act].value); }
    else if (e.key === "Escape" || e.key === "Tab") close();
  });
  dropdowns.push({ sync, close, dd });
  sync();
}
// Quiet hours: Off or a whole hour (the backend stores hours).
document.querySelectorAll("select.hours").forEach((s) => {
  s.append(new Option("Off", ""));
  for (let h = 0; h < 24; h++) s.append(new Option(String(h).padStart(2, "0") + ":00", String(h)));
});
document.querySelectorAll("select").forEach(enhanceSelect);
// Own number stepper: the browser's spin arrows never show. The input becomes plain text that
// accepts digits only; - and + step within min..max, and leaving the field clamps the value.
function enhanceNumber(input) {
  const min = Number(input.min), max = Number(input.max);
  input.type = "text"; input.inputMode = "numeric"; input.autocomplete = "off";
  const clamp = (n) => Math.min(max, Math.max(min, n));
  const step = (d) => {
    const n = Number(input.value);
    input.value = String(clamp((Number.isFinite(n) && input.value !== "" ? n : min) + d));
    input.dispatchEvent(new Event("input", { bubbles: true }));
  };
  const box = el("span", "stepper");
  const less = btn("\u2212", () => step(-1), "ghost"), more = btn("+", () => step(1), "ghost");
  // Named after the row label so screen readers say "Less: Move to terminal after".
  const row = input.closest(".row"), name = row && row.querySelector(".rl") ? row.querySelector(".rl").textContent : "";
  less.setAttribute("aria-label", name ? "Less: " + name : "Less"); more.setAttribute("aria-label", name ? "More: " + name : "More");
  input.before(box);
  box.append(less, input, more);
  input.addEventListener("input", () => { const v = input.value.replace(/\D/g, "").slice(0, 3); if (v !== input.value) input.value = v; });
  input.addEventListener("blur", () => { if (input.value !== "") input.value = String(clamp(Number(input.value))); });
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") { e.preventDefault(); step(e.key === "ArrowUp" ? 1 : -1); }
  });
}
document.querySelectorAll('input[type="number"]').forEach(enhanceNumber);
// Own time and date pickers (owner, 2026-11-05: Nubi uses its own controls, never the browser's).
// The real <input> stays in the form, hidden, and holds the value; the picker writes to it.
function popover(input, cls, build, text) {
  const dd = el("div", "dd " + cls);
  const b = el("button"); b.type = "button";
  const label = el("span");
  b.append(label); b.insertAdjacentHTML("beforeend", CHEVRON);
  const pop = el("div", "pop");
  pop.setAttribute("role", "dialog");
  b.setAttribute("aria-haspopup", "dialog"); b.setAttribute("aria-expanded", "false");
  dd.append(b, pop);
  input.hidden = true;
  input.after(dd);
  const close = () => { dd.classList.remove("open", "up"); b.setAttribute("aria-expanded", "false"); };
  // Keyboard: focus moves into the picker on the chosen (or first) value.
  const focusIn = () => (pop.querySelector('[tabindex="0"]') || pop.querySelector("button"))?.focus({ preventScroll: true });
  // `keep` leaves the picker open (time: the hour is picked first, then the minutes).
  const set = (v, keep) => {
    input.value = v; sync(); input.dispatchEvent(new Event("input", { bubbles: true }));
    if (keep) { build(pop, input.value, set); focusIn(); } else { close(); b.focus(); }
  };
  function sync() { label.textContent = text(input.value); b.disabled = input.disabled; }
  b.addEventListener("click", () => {
    const open = !dd.classList.contains("open");
    dropdowns.forEach((d) => d.close());
    if (!open) return;
    pop.setAttribute("aria-label", (dd.closest(".row")?.querySelector(".rl")?.textContent || "Date") + " picker");
    build(pop, input.value, set);
    // Open upward when the picker would run past the bottom of the window.
    const r = b.getBoundingClientRect();
    dd.classList.toggle("up", r.bottom + pop.offsetHeight + 12 > innerHeight && r.top > pop.offsetHeight + 12);
    dd.classList.add("open"); b.setAttribute("aria-expanded", "true");
    requestAnimationFrame(focusIn);
  });
  dd.addEventListener("keydown", (e) => { if (e.key === "Escape") { e.stopPropagation(); close(); b.focus(); } });
  dropdowns.push({ sync, close, dd });
  sync();
}
// Roving focus: one Tab stop per list; arrow keys move inside it.
function rove(items, current) {
  items.forEach((o, i) => (o.tabIndex = i === current ? 0 : -1));
}
const two = (n) => String(n).padStart(2, "0");
function timeBuild(input) {
  return (pop, value, set) => {
    const [h, m] = value ? value.split(":").map(Number) : [null, null];
    pop.replaceChildren();
    const cols = el("div", "tp-cols");
    const col = (title, values, sel, pick) => {
      const wrap = el("div", "tp-wrap"), c = el("div", "tp-col");
      c.setAttribute("role", "listbox"); c.setAttribute("aria-label", title);
      const items = values.map((i) => {
        const o = btn(two(i), () => pick(i), i === sel ? "on" : "");
        o.setAttribute("role", "option"); o.setAttribute("aria-selected", String(i === sel));
        c.append(o);
        return o;
      });
      rove(items, Math.max(0, values.indexOf(sel)));
      c.addEventListener("keydown", (e) => {
        const i = items.indexOf(document.activeElement);
        const to = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: items.length - 1 }[e.key];
        if (to === undefined) return;
        e.preventDefault();
        const n = Math.max(0, Math.min(items.length - 1, to));
        rove(items, n); items[n].focus();
      });
      wrap.append(el("span", "tp-h", title), c);
      return wrap;
    };
    let hour = h ?? 9, minute = m ?? 0;
    // Five-minute steps, plus a saved minute off that grid so it can still be seen and kept.
    const minutes = [...Array(12).keys()].map((i) => i * 5);
    if (m != null && !minutes.includes(m)) { minutes.push(m); minutes.sort((a, b) => a - b); }
    const hours = col("Hour", [...Array(24).keys()], h, (i) => { hour = i; set(`${two(hour)}:${two(minute)}`, true); });
    const mins = col("Min", minutes, m, (i) => { minute = i; set(`${two(hour)}:${two(minute)}`); });
    cols.append(hours, mins);
    pop.append(cols);
    if (!input.required) pop.append(btn("Clear", () => set(""), "ghost tp-clear"));
    // Show the chosen values without scrolling the page.
    requestAnimationFrame(() => pop.querySelectorAll(".tp-col .on").forEach((o) => { o.parentElement.scrollTop = o.offsetTop - o.parentElement.offsetTop - 60; }));
  };
}
document.querySelectorAll('#settings input[type="time"]').forEach((i) =>
  popover(i, "tp", timeBuild(i), (v) => v || "Not set"));
const MONTH = (y, mo) => new Date(y, mo, 1).toLocaleDateString([], { month: "long", year: "numeric" });
function dateBuild(pop, value, set) {
  const today = new Date();
  const iso = (d) => `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
  let cur = value ? new Date(value + "T00:00") : new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const focusCur = () => pop.querySelector('.cal-d[tabindex="0"]')?.focus();
  function draw() {
    const y = cur.getFullYear(), mo = cur.getMonth();
    pop.replaceChildren();
    const head = el("div", "cal-head");
    const month = (d) => { cur = new Date(y, mo + d, Math.min(cur.getDate(), new Date(y, mo + d + 1, 0).getDate())); draw(); };
    const prev = btn("‹", () => month(-1), "ghost cal-nav");
    const next = btn("›", () => month(1), "ghost cal-nav");
    prev.setAttribute("aria-label", "Previous month"); next.setAttribute("aria-label", "Next month");
    const title = el("b", null, MONTH(y, mo)); title.setAttribute("aria-live", "polite");
    head.append(prev, title, next);
    const grid = el("div", "cal-grid");
    for (const w of ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]) grid.append(el("span", "cal-w", w));
    const lead = (new Date(y, mo, 1).getDay() + 6) % 7; // Monday first
    for (let i = 0; i < lead; i++) grid.append(el("span"));
    const days = new Date(y, mo + 1, 0).getDate();
    const items = [];
    for (let d = 1; d <= days; d++) {
      const v = iso(new Date(y, mo, d));
      const o = btn(String(d), () => set(v), "cal-d" + (v === value ? " on" : "") + (v === iso(today) ? " now" : ""));
      o.setAttribute("aria-label", dayLabel(v) + (v === value ? ", selected" : "") + (v === iso(today) ? ", today" : ""));
      grid.append(o); items.push(o);
    }
    rove(items, cur.getDate() - 1);
    // Arrows move by day and week, PageUp/PageDown by month, Home/End to the ends of the week.
    grid.addEventListener("keydown", (e) => {
      if (e.key === "PageUp" || e.key === "PageDown") { e.preventDefault(); month(e.key === "PageUp" ? -1 : 1); focusCur(); return; }
      const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[e.key];
      const dow = (cur.getDay() + 6) % 7;
      const n = step ? new Date(y, mo, cur.getDate() + step)
        : e.key === "Home" ? new Date(y, mo, cur.getDate() - dow)
        : e.key === "End" ? new Date(y, mo, cur.getDate() + 6 - dow) : null;
      if (!n) return;
      e.preventDefault();
      cur = n;
      if (n.getMonth() === mo && n.getFullYear() === y) { rove(items, n.getDate() - 1); items[n.getDate() - 1].focus(); }
      else { draw(); focusCur(); }
    });
    const foot = el("div", "cal-foot");
    foot.append(btn("Today", () => set(iso(today)), "ghost"));
    pop.append(head, grid, foot);
  }
  draw();
}
document.querySelectorAll('.dt-dates input[type="date"]').forEach((i) =>
  popover(i, "dp", dateBuild, () => "Add a date"));
addEventListener("pointerdown", (e) => dropdowns.forEach((d) => { if (!d.dd.contains(e.target)) d.close(); }));
const syncDropdowns = () => dropdowns.forEach((d) => d.sync());

let snap = null, formDirty = false;
const picks = {};

// ---- requests: shown in the Inbox and on top of Sessions ----
const built = {};
function countdowns(box, s) {
  for (const p of s.pending) for (const c of box.querySelectorAll(`[data-left="${p.id}"]`)) c.textContent = `Terminal in ${Math.max(0, Math.round((p.expires_ms - Date.now()) / 1000))}s`;
}
function renderRequests(box, s) {
  const ids = s.pending.map((p) => p.id).join(",");
  if (built[box.id] === ids) { countdowns(box, s); return; }
  const before = (built[box.id] || "").split(",");
  built[box.id] = ids;
  box.replaceChildren();
  if (box.id === "inbox" && !s.pending.length) { box.append(el("div", "empty", "Nothing waiting. Approvals and Claude's questions appear here.")); return; }
  for (const p of s.pending) {
    const it = el("div", "item " + p.provider);
    const h = el("div", "ih");
    const tl = el("div");
    tl.append(el("div", "t", p.kind === "question" ? "Question" : `Allow ${p.tool_name || "tool"}?`), el("div", "sm", p.name || p.folder || "session"));
    const left = el("span", "pill waiting left"); left.dataset.left = p.id;
    h.append(tile(p.provider), tl, left);
    it.append(h);
    if (p.kind === "question") renderQuestion(it, p);
    else {
      if (p.tool_input) it.append(el("pre", null, p.tool_input));
      const a = el("div", "acts");
      a.append(btn("Allow", () => invoke("answer_permission", { id: p.id, decision: "allow" }), "primary"),
        btn("Deny", () => invoke("answer_permission", { id: p.id, decision: "deny" })), el("span", "grow"),
        btn("Open terminal", () => invoke("jump_session", { key: p.session_key }), "link"));
      it.append(a);
    }
    box.append(it);
    // A new request drops in like a notch card.
    if (!before.includes(String(p.id)) && box.offsetParent) tween(it, { y: -10, scale: 0.97, opacity: 0, blur: 6 }, {}, PRESET.open);
  }
  countdowns(box, s);
}

function renderQuestion(it, p) {
  const sel = picks[p.id] || (picks[p.id] = {});
  for (const q of p.questions || []) {
    const chosen = sel[q.question] || (sel[q.question] = new Set());
    const qt = el("div", "q");
    if (q.header) qt.append(el("span", "h", q.header));
    qt.append(document.createTextNode(q.question + (q.multiSelect ? " (pick any)" : "")));
    it.append(qt);
    const opts = el("div", "opts");
    const key = p.id + "\u0000" + q.question;
    (q.options || []).forEach((o, i) => {
      const b = el("button", "opt" + (chosen.has(o.label) ? " on" : ""));
      b.type = "button";
      const l = el("span", "l");
      l.append(el("span", null, o.label));
      if (o.description) l.append(el("small", null, o.description));
      b.append(el("span", "k", String(i + 1)), l);
      b.addEventListener("click", () => {
        if (q.multiSelect) chosen.has(o.label) ? chosen.delete(o.label) : chosen.add(o.label);
        else { chosen.clear(); chosen.add(o.label); }
        // The same question shows in Inbox and Sessions: update both copies.
        document.querySelectorAll(".opts").forEach((os) => {
          if (os.dataset.q === key) os.querySelectorAll(".opt").forEach((r, j) => r.classList.toggle("on", chosen.has(q.options[j].label)));
        });
      });
      opts.append(b);
    });
    opts.dataset.q = key;
    it.append(opts);
    const other = el("input", "other");
    other.placeholder = "Other answer";
    const known = (l) => (q.options || []).some((o) => o.label === l);
    other.addEventListener("input", () => {
      for (const l of [...chosen]) if (!known(l)) chosen.delete(l);
      if (other.value.trim()) { if (!q.multiSelect) chosen.clear(); chosen.add(other.value.trim()); }
    });
    it.append(other);
  }
  const a = el("div", "acts");
  a.append(
    btn("Send", () => {
      const answers = {};
      for (const q of p.questions || []) { const c = sel[q.question]; if (!c || !c.size) return; answers[q.question] = [...c].join(", "); }
      invoke("answer_question", { id: p.id, answers });
      delete picks[p.id];
    }, "primary"),
    el("span", "grow"),
    btn("Answer in terminal", () => { invoke("answer_question", { id: p.id, answers: {} }); delete picks[p.id]; }, "link"),
    btn("Open terminal", () => invoke("jump_session", { key: p.session_key }), "link"));
  it.append(a);
}

// ---- sessions ----
let sfilter = "all", sessionKeys = "";
document.querySelectorAll("input[name=sfilter]").forEach((r) => r.addEventListener("change", () => {
  sfilter = r.value; syncThumbs(); if (snap) renderSessions(snap, true);
}));
function renderSessions(s, animate) {
  const list = s.sessions.filter((x) => sfilter === "all" || x.provider === sfilter);
  // Idle and finished sessions are listed too, so "running" overstated it.
  const n = s.sessions.length;
  $("sessions-meta").textContent = n ? `${n} session${n === 1 ? "" : "s"}` : "";
  renderRequests($("session-requests"), s);
  const box = $("sessions");
  const keys = list.map((x) => x.key).join(",");
  const changed = keys !== sessionKeys; sessionKeys = keys;
  box.replaceChildren();
  if (!list.length) { box.append(el("div", "empty", s.sessions.length ? "No sessions for this filter" : "No sessions yet. Turn on Connections in Settings, then start Claude Code or Codex.")); return; }
  for (const x of list) {
    const r = el("div", "ses");
    const tl = el("div");
    const parts = [x.folder, x.context_tokens ? Math.round(x.context_tokens / 1000) + "k context" : "", x.subagents ? `${x.subagents} agents` : ""].filter(Boolean);
    tl.append(el("div", "n", x.name || x.folder), el("div", "sm", parts.join(" · ")));
    const st = x.attention && x.state !== "waiting" ? "waiting" : x.state;
    r.append(tile(x.provider), tl, el("span", "grow"), el("span", "pill " + st, STATE[st] || st), el("span", "ago", ago(x.last_event_ms)),
      btn("Open terminal", () => invoke("jump_session", { key: x.key }), "ghost"));
    box.append(r);
  }
  if ((animate || changed) && box.offsetParent) stagger(box.children);
}

// ---- usage: ring for the first window, bars for every window ----
const ringVals = {}, barVals = {};
function barColor(p, used) { return used >= 70 ? "var(--bad)" : used >= 50 ? "var(--warn)" : p === "codex" ? "var(--codex)" : "var(--claude)"; }
function renderUsage(s) {
  const box = $("usage");
  box.replaceChildren();
  const known = s.usage.filter((u) => u.windows.length && u.fetched_ms);
  const oldest = known.reduce((a, u) => !a || u.fetched_ms < a.fetched_ms ? u : a, null);
  $("usage-meta").textContent = oldest ? UsageStatus.age(oldest) : "";
  if (!s.usage.length) box.append(el("div", "empty", "Loading usage..."));
  for (const u of s.usage) {
    const c = el("div", "meter " + u.provider);
    const w0 = u.windows[0];
    if (w0) {
      const ring = el("div", "ring");
      const pct = Math.min(100, w0.used_percent);
      ring.innerHTML = `<svg viewBox="0 0 36 36"><circle cx="18" cy="18" r="15.5" stroke="var(--card-2)" stroke-width="3.2"/><circle class="arc" cx="18" cy="18" r="15.5" stroke="${barColor(u.provider, pct)}" stroke-width="3.2" stroke-dasharray="0 97.4"/></svg>`;
      const v = el("div", "v"), inner = el("div"), b = el("b", null, "0%");
      // Short label: "used, last 5 hours" ran into the ring stroke.
      inner.append(b, el("small", null, RING_LABEL[w0.label] || "used"));
      v.append(inner); ring.append(v); c.append(ring);
      // Counts up on the content spring from the last shown value.
      const sp = new Spring(ringVals[u.provider] ?? 0, PRESET.content).to(pct);
      ringVals[u.provider] = pct;
      const arc = ring.querySelector(".arc");
      new Animator({ sp }, (o) => {
        const x = Math.max(0, Math.min(100, o.sp));
        b.textContent = Math.round(x) + "%";
        arc.setAttribute("stroke-dasharray", `${(x / 100) * 97.4} 97.4`);
      }).kick();
    }
    const body = el("div", "body");
    const top = el("div", "top");
    top.append(tile(u.provider), el("div", "ct", NAME[u.provider]));
    // Only stale or missing data gets a (warning) note; the page header shows the age.
    const status = el("span", "st", UsageStatus.stale(u) ? UsageStatus.text(u) : "");
    status.dataset.usageProvider = u.provider;
    top.append(status);
    body.append(top);
    for (const w of u.windows) {
      const kv = el("div", "kv");
      kv.append(el("span", null, LABEL[w.label] || w.label), el("span", null, `${Math.round(w.used_percent)}% · ${resetIn(w.resets_at)}`));
      const bar = el("div", "bar"), i = el("i");
      const k = u.provider + w.label, to = Math.min(100, w.used_percent);
      i.style.width = (barVals[k] ?? 0) + "%"; i.style.background = barColor(u.provider, w.used_percent);
      barVals[k] = to;
      bar.append(i);
      requestAnimationFrame(() => requestAnimationFrame(() => (i.style.width = to + "%")));
      body.append(kv, bar);
    }
    c.append(body);
    box.append(c);
  }
}

// ---- alerts ----
let alertCount = -1;
function renderAlerts(s) {
  const box = $("alerts");
  const grew = alertCount >= 0 && s.alerts.length > alertCount; alertCount = s.alerts.length;
  box.replaceChildren();
  const m = s.alerts.length;
  $("alerts-meta").textContent = !m ? "" : m >= 50 ? "Latest 50" : `${m} alert${m === 1 ? "" : "s"}`;
  if (!m) { box.append(el("div", "empty", "No alerts yet")); return; }
  // Repeats in a row (same title and text) collapse into one row with a count.
  const groups = [];
  for (const a of s.alerts) {
    const g = groups[groups.length - 1];
    if (g && !a.queued && !g.a.queued && g.a.title === a.title && g.a.body === a.body) g.n++;
    else groups.push({ a, n: 1 });
  }
  for (const { a, n } of groups) {
    const r = el("div", "al");
    const tx = el("div", "tx", `${a.title} · ${a.body}`);
    if (n > 1) tx.append(el("span", "cnt", ` ×${n}`));
    if (a.queued) tx.append(el("small", null, "Received while Do not disturb was on"));
    r.append(el("span", "tm", new Date(a.ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })), tile(a.provider), tx);
    box.append(r);
  }
  if (grew && box.offsetParent) tween(box.firstChild, { y: -10, opacity: 0, blur: 6 }, {}, PRESET.open);
}

// ---- settings ----
function renderHooks(s) {
  const box = $("hooks");
  box.replaceChildren();
  for (const p of ["claude", "codex"]) {
    const on = !!s.hooks[p];
    const r = el("div", "row");
    const left = el("div", "hook");
    const t = el("div");
    t.append(el("div", "rl", NAME[p]), el("div", "rd", p === "codex" && on ? "In Codex, run /hooks once and allow Nubi" : p === "codex" ? "$CODEX_HOME/hooks.json" : "~/.claude/settings.json"));
    left.append(tile(p), t);
    const right = el("span", "hook");
    right.append(el("span", "pill " + (on ? "working" : "idle"), on ? "On" : "Off"),
      btn(on ? "Turn off" : "Turn on", () => invoke("set_hooks", { provider: p, enable: !on }).catch((e) => alert(e))));
    r.append(left, right);
    box.append(r);
  }
  // DeskTime: turning it on here shows its section in the sidebar and the tab in the notch.
  const on = !!s.settings.desktime_tab;
  const r = el("div", "row"), left = el("div", "hook"), t = el("div");
  t.append(el("div", "rl", "DeskTime"), el("div", "rd", "Your DeskTime hours in the notch"));
  left.append(el("span", "ico dt", "D"), t);
  const right = el("span", "hook");
  right.append(el("span", "pill " + (on ? "working" : "idle"), on ? "On" : "Off"));
  if (on) right.append(btn("Settings", () => document.querySelector('.side button[data-pane="desktime"]').click()));
  right.append(btn(on ? "Turn off" : "Turn on", () =>
    invoke("save_settings", { settings: { ...snap.settings, desktime_tab: !on } }).catch((e) => alert(e))));
  r.append(left, right);
  box.append(r);
}
// The DeskTime section is in the sidebar only while DeskTime is on in Connections.
function desktimeNav(cfg) {
  const b = document.querySelector('.side button[data-pane="desktime"]');
  b.hidden = !cfg.desktime_tab;
  if (b.hidden && b.classList.contains("on")) document.querySelector('.side button[data-pane="hooks"]').click();
}
function fillForm(cfg) {
  const f = $("settings");
  for (const [k, v] of Object.entries(cfg)) {
    const i = f.elements[k];
    if (!i) continue;
    if (i instanceof RadioNodeList) { for (const r of i) r.checked = r.value === String(v); continue; }
    if (i.type === "checkbox") i.checked = !!v;
    else i.value = v == null ? "" : String(v);
  }
  fillDesktime(cfg);
  syncDropdowns();
  modeFields();
  f.elements.launch_hotkey.disabled = !f.elements.launcher_enabled.checked;
  paintAllCaps();
  window.nubiTheme.apply({ theme: cfg.theme, accent: cfg.accent, reduce_motion: cfg.reduce_motion });
  requestAnimationFrame(syncThumbs);
}
// DeskTime fields that are not plain inputs: work-day checkboxes and the date lists.
function fillDesktime(cfg) {
  const f = $("settings").elements;
  document.querySelectorAll("#dt-days input").forEach((c) => (c.checked = (cfg.work_days || []).includes(Number(c.value))));
  document.querySelectorAll("#dt-views input").forEach((c) => (c.checked = (cfg.dt_views || []).includes(c.value)));
  document.querySelectorAll(".dt-rows-pick input").forEach((c) => (c.checked = (cfg["dt_rows_" + c.closest(".dt-rows-pick").dataset.view] || []).includes(c.value)));
  f.dt_glance_kind.disabled = !cfg.dt_glance;
  dtDates.holidays = [...(cfg.holidays || [])].sort();
  dtDates.half_days = [...(cfg.half_days || [])].sort();
  paintDates();
  paintPreview();
  desktimeNav(cfg);
}
// Holidays and half days: a date picker adds a day, a chip's x removes it. Saved with the form.
const dtDates = { holidays: [], half_days: [] };
const dayLabel = (d) => new Date(d + "T00:00").toLocaleDateString([], { weekday: "short", day: "numeric", month: "short", year: "numeric" });
function paintDates() {
  for (const row of document.querySelectorAll(".dt-dates")) {
    const list = dtDates[row.dataset.list], chips = row.querySelector(".dt-chips");
    chips.replaceChildren(...list.map((d) => {
      const c = el("span", "dt-chip", dayLabel(d));
      const x = btn("\u00d7", () => { list.splice(list.indexOf(d), 1); paintDates(); setDirty(true); });
      x.className = "x"; x.title = "Remove " + dayLabel(d); x.setAttribute("aria-label", "Remove " + dayLabel(d));
      c.append(x);
      return c;
    }));
    if (!list.length) chips.append(el("span", "hint", "None"));
  }
}
for (const row of document.querySelectorAll(".dt-dates")) {
  const input = row.querySelector("input[type=date]");
  // A picked date is added at once (no separate Add click, so nothing is lost on Save).
  const note = row.querySelector(".dt-note");
  input.addEventListener("input", () => {
    const list = dtDates[row.dataset.list], v = input.value;
    input.value = "";
    if (!v) return;
    note.textContent = list.includes(v) ? dayLabel(v) + " is already in the list" : list.length >= 366 ? "The list is full (366 days)" : "";
    if (!note.textContent) { list.push(v); list.sort(); paintDates(); setDirty(true); }
    syncDropdowns();
  });
}
// Live sketch of the notch tab for the ticked sections.
function paintPreview() {
  const views = [...document.querySelectorAll("#dt-views input:checked")].map((c) => c.value);
  const box = $("dt-preview");
  const block = (cls, title) => { const b = el("div", "pv " + cls); b.append(el("b", null, title), el("i"), el("i")); return b; };
  box.replaceChildren();
  if (views.includes("day")) { const d = block("day", "Today"); d.append(el("span", "bar")); box.append(d); }
  const periods = [["week", "This week"], ["month", "This month"]].filter(([v]) => views.includes(v));
  if (periods.length) {
    const row = el("div", "pv-row" + (periods.length === 1 ? " one" : ""));
    for (const [v, title] of periods) row.append(block(v, title));
    box.append(row);
  }
}
$("dt-views").addEventListener("change", paintPreview);
// Two ways in: Log in with email opens DeskTime's own login page in a Nubi window, or the owner
// logs in in their browser and the Nubi extension's Allow access hands the session over.
function renderDesktime(s) {
  const on = s.enabled && !s.logged_out;
  $("dt-status").textContent = !s.enabled ? "Log in with your DeskTime email and password, or install the Nubi extension (Browser extension below), log in in your browser and click Allow access."
    : s.logged_out ? "Signed out. Log in with email again, or click Allow access in the Nubi extension again."
    : "Nubi reads your dashboard every 5 minutes";
  const pill = $("dt-pill");
  pill.className = "pill " + (!on ? "idle" : s.error ? "waiting" : "working");
  pill.textContent = !s.enabled ? "Not connected" : s.logged_out ? "Signed out" : s.error ? "Read failed" : "Connected";
  $("dt-login").hidden = on;
  $("dt-signin").hidden = on;
  $("dt-dash").hidden = !on;
  $("dt-logout").hidden = !on; // signed out already: nothing to log out of
  $("dt-diag").hidden = !s.enabled;
  const when = (ms) => (ms ? new Date(ms).toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit", hour12: false }) : "");
  $("dt-read").textContent = s.loading ? "Reading now\u2026" : when(s.read_ms) || "Never";
  $("dt-sync").textContent = when(s.synced_ms) || "Not received since Nubi started";
  $("dt-err").textContent = s.error || "None";
  $("dt-err").classList.toggle("bad", !!s.error);
  $("dt-refresh").disabled = !on || s.loading;
  $("dt-ext-path").textContent = s.extension_dir || "";
  // Install steps matter only while not connected; once connected they were just noise.
  $("dt-ext-steps").hidden = on;
  $("dt-ext-path").hidden = on;
  $("dt-ext-note").textContent = on ? "Installed. Needed again only if you log out or switch browsers" : "Install it in Chrome, Brave or Edge:";
}
const dtFail = (e) => ($("saved").textContent = String(e));
$("dt-signin").addEventListener("click", () => invoke("desktime_login").catch(dtFail));
$("dt-login").addEventListener("click", () => invoke("desktime_open", { target: "site" }).catch(dtFail));
$("dt-ext-get").addEventListener("click", () => invoke("desktime_open", { target: "extension_download" }).catch(dtFail));
$("dt-ext-open").addEventListener("click", () => invoke("desktime_open", { target: "extension" }).catch(dtFail));
$("dt-logout").addEventListener("click", () => invoke("desktime_logout").catch(dtFail));
$("dt-dash").addEventListener("click", () => invoke("desktime_open", { target: "dashboard" }).catch(dtFail));
$("dt-refresh").addEventListener("click", () => invoke("desktime_refresh").catch(dtFail));
listen("desktime", (e) => renderDesktime(e.payload));
invoke("desktime_state").then(renderDesktime).catch(() => {});
function modeFields() {
  const m = $("settings").elements.display_mode.value;
  document.querySelectorAll(".mode-notch").forEach((e) => (e.style.display = m === "island" ? "" : "none"));
  document.querySelectorAll(".mode-taskbar").forEach((e) => (e.style.display = m === "taskbar" ? "" : "none"));
}
// Changes are a draft until Save. Theme, accent and motion preview live; Discard reverts them.
function setDirty(d) {
  const was = formDirty;
  formDirty = d; window.nubiTheme.hold = d;
  $("draftbar").hidden = !d; $("draft-err").textContent = "";
  if (d && !was) tween($("draftbar"), { y: 16, opacity: 0, scale: 0.98 }, {}, PRESET.open);
}
$("settings").addEventListener("input", () => {
  setDirty(true); modeFields(); syncThumbs();
  const f = $("settings").elements;
  window.nubiTheme.apply({ theme: f.theme.value, accent: f.accent.value, reduce_motion: f.reduce_motion.checked });
});
function discard() { setDirty(false); if (snap) fillForm(snap.settings); }
$("discard").addEventListener("click", discard);
$("leave-discard").addEventListener("click", () => { const to = leavingTo; discard(); closeLeave(); if (to) go(to); });
$("leave-save").addEventListener("click", async () => { const to = leavingTo; if (await saveSettings()) { closeLeave(); if (to) go(to); } });
$("settings").addEventListener("submit", (e) => { e.preventDefault(); saveSettings(); });
// Enter on a checkbox or radio (day chips, segmented controls, switches, swatches) toggles it
// like Space instead of submitting the whole form (owner, 2026-11-05).
$("settings").addEventListener("keydown", (e) => {
  const t = e.target;
  if (e.key !== "Enter" || !(t instanceof HTMLInputElement) || !["checkbox", "radio"].includes(t.type)) return;
  e.preventDefault();
  t.click();
});
let savedTimer = 0;
async function saveSettings() {
  const f = $("settings").elements;
  const num = (v) => (v === "" ? null : Number(v));
  const settings = {
    ...snap.settings,
    providers: f.providers.value,
    questions_mode: f.questions_mode.value,
    request_layout: f.request_layout.value,
    request_popup_secs: Number(f.request_popup_secs.value || 0),
    takeover_secs: Math.min(95, Math.max(3, Number(f.takeover_secs.value || 10))),
    display_mode: f.display_mode.value,
    monitor: Number(f.monitor.value || 0),
    edge: f.edge.value,
    notch_on_hover: f.notch_on_hover.checked,
    taskbar_side: f.taskbar_side.value,
    dnd_hotkey: f.dnd_hotkey.value,
    launch_hotkey: f.launch_hotkey.value,
    launcher_enabled: f.launcher_enabled.checked,
    quiet_start: num(f.quiet_start.value),
    quiet_end: num(f.quiet_end.value),
    follow_quiet_time: f.follow_quiet_time.checked,
    sounds: f.sounds.checked,
    toast_when_fullscreen: f.toast_when_fullscreen.checked,
    auto_update: f.auto_update.checked,
    auto_start: f.auto_start.checked,
    theme: f.theme.value,
    accent: f.accent.value,
    notch_theme: f.notch_theme.value,
    notch_accent: f.notch_accent.value,
    reduce_motion: f.reduce_motion.checked,
    desktime_tab: snap.settings.desktime_tab,
    daily_target: f.daily_target.value || snap.settings.daily_target,
    end_of_day: f.end_of_day.value || snap.settings.end_of_day,
    start_of_day: f.start_of_day.value,
    work_days: [...document.querySelectorAll("#dt-days input:checked")].map((c) => Number(c.value)),
    holidays: [...dtDates.holidays],
    half_days: [...dtDates.half_days],
    dt_views: [...document.querySelectorAll("#dt-views input:checked")].map((c) => c.value),
    dt_glance: f.dt_glance.checked,
    dt_glance_kind: f.dt_glance_kind.value,
    dt_alert_target: f.dt_alert_target.checked,
    dt_notes: f.dt_notes.checked,
    ...Object.fromEntries([...document.querySelectorAll(".dt-rows-pick")].map((p) => ["dt_rows_" + p.dataset.view, [...p.querySelectorAll("input:checked")].map((c) => c.value)])),
  };
  try {
    await invoke("save_settings", { settings });
    setDirty(false);
    $("saved").textContent = "Saved"; clearTimeout(savedTimer); savedTimer = setTimeout(() => ($("saved").textContent = ""), 3000);
    return true;
  } catch (err) { $("draft-err").textContent = "Not saved: " + String(err); return false; }
}

// ---- about ----
invoke("app_version").then((v) => ($("version").textContent = "Version " + v)).catch(() => {});
$("whats-new").addEventListener("click", () => invoke("open_release_notes"));
$("check-update").addEventListener("click", async () => {
  const b = $("check-update"); b.textContent = "Checking…"; b.disabled = true;
  try { b.textContent = await invoke("check_update"); } catch (e) { b.textContent = "Check failed"; $("saved").textContent = String(e); }
  setTimeout(() => { b.textContent = "Check for updates"; b.disabled = false; }, 3000);
});

// ---- shell ----
let lastPending = 0;
function render(s) {
  snap = s;
  $("dnd").checked = s.dnd;
  $("dnd-wrap").title = s.dnd ? (s.dnd_manual ? "Do not disturb is on" : "Do not disturb, on from quiet hours") : "Do not disturb (" + (s.settings.dnd_hotkey ? s.settings.dnd_hotkey.split("+").map(capLabel).join("+") : "off") + ")";
  $("pipe").textContent = s.pipe_error ? "Pipe error: " + s.pipe_error : "";
  const ni = $("n-inbox");
  ni.textContent = s.pending.length || ""; ni.hidden = !s.pending.length;
  if (s.pending.length > lastPending) tween(ni, { scale: 0.4, opacity: 0 }, {}, PRESET.press);
  lastPending = s.pending.length;
  $("inbox-meta").textContent = s.pending.length ? `${s.pending.length} waiting` : "";
  renderRequests($("inbox"), s); renderSessions(s); renderUsage(s); renderAlerts(s); renderHooks(s); nameRows($("hooks"));
  if (!formDirty) fillForm(s.settings);
}
$("dnd").addEventListener("change", (e) => invoke("set_dnd", { on: e.target.checked }));
$("refresh").addEventListener("click", () => invoke("refresh_usage"));
$("test").addEventListener("click", () => invoke("test_alert"));
// ---- report an issue: optional draft by the local Claude/Codex CLI, then the prefilled GitHub form ----
let repCtx = null;
const repF = (k) => document.querySelector(`#rep [data-f="${k}"]`);
document.querySelectorAll("#rep select").forEach(enhanceSelect);
function repShowFields() { $("rep-fields").hidden = false; $("rep-open").disabled = false; syncDropdowns(); }
$("report").addEventListener("click", async () => {
  $("rep-status").textContent = ""; showModal("rep", $("rep-text"));
  repCtx = await invoke("report_context").catch(() => null);
  if (repCtx && !repF("setup").value) repF("setup").value = repCtx.setup || "";
});
$("rep-cancel").addEventListener("click", () => hideModal("rep"));
$("rep-manual").addEventListener("click", () => { if (!repF("what-happened").value) repF("what-happened").value = $("rep-text").value; repShowFields(); });
document.querySelectorAll("#rep [data-agent]").forEach((b) => b.addEventListener("click", async () => {
  const text = $("rep-text").value.trim();
  if (!text) { $("rep-status").textContent = "Write a sentence first"; $("rep-text").focus(); return; }
  const agent = b.dataset.agent, btns = document.querySelectorAll("#rep [data-agent]");
  btns.forEach((x) => (x.disabled = true));
  $("rep-status").textContent = (agent === "claude" ? "Claude" : "Codex") + " is drafting…";
  try {
    const f = await invoke("draft_report", { agent, text, context: repCtx || {} });
    for (const [k, v] of Object.entries(f)) {
      const i = repF(k); if (!i) continue;
      if (i.tagName === "SELECT") { if ([...i.options].some((o) => o.value === v)) i.value = v; } else i.value = v;
    }
    $("rep-status").textContent = "Check the draft, then open it on GitHub.";
    repShowFields();
  } catch (e) { $("rep-status").textContent = String(e); }
  btns.forEach((x) => (x.disabled = false));
}));
$("rep-open").addEventListener("click", () => {
  const fields = { ...(repCtx || {}) };
  document.querySelectorAll("#rep [data-f]").forEach((i) => (fields[i.dataset.f] = i.value));
  invoke("open_report", { fields });
  hideModal("rep");
});

// ---- version history ----
async function loadReleases() {
  const box = $("releases");
  try {
    const list = await invoke("list_releases");
    if (!list.length) { box.replaceChildren(el("div", "rd", "No releases yet")); return; }
    box.replaceChildren(...list.map((r) => {
      const row = el("div", "rel");
      const info = el("div");
      info.append(el("div", "rl", "Version " + r.version + (r.current ? " (installed)" : "") + (r.prerelease ? " · pre-release" : "")), el("div", "rd", r.date));
      const notes = el("button", "btn ghost", "Notes"); notes.type = "button";
      notes.addEventListener("click", () => invoke("open_release_link", { url: r.notes }));
      const dl = el("button", "btn", "Download"); dl.type = "button";
      if (r.download) dl.addEventListener("click", () => invoke("open_release_link", { url: r.download }));
      else { dl.disabled = true; dl.title = "No Windows installer in this release"; }
      const acts = el("span", "hook"); acts.append(notes);
      if (!r.current) acts.append(dl);
      row.append(info, acts);
      return row;
    }));
  } catch (e) { box.replaceChildren(el("div", "rd", "Could not load versions. " + e)); }
}
loadReleases();
invoke("monitors").then((list) => {
  list.forEach((label, i) => $("monitor").append(new Option(label || "Display " + (i + 1), String(i))));
  if (snap) $("monitor").value = String(snap.settings.monitor);
  syncDropdowns();
});
listen("state", (e) => render(e.payload));
invoke("get_state").then((s) => {
  render(s);
  stagger(document.querySelectorAll("#v-inbox .item, #v-inbox .empty"));
  // Theme crossfades only after the first paint, so startup does not flash.
  requestAnimationFrame(() => document.documentElement.classList.add("ready"));
});
// Countdowns tick every second without rebuilding the cards.
setInterval(() => { if (snap) { countdowns($("inbox"), snap); countdowns($("session-requests"), snap); } }, 1000);
// Refresh the age without restarting meter animations.
setInterval(() => {
  if (!snap) return;
  document.querySelectorAll("[data-usage-provider]").forEach((node) => {
    const u = snap.usage.find((u) => u.provider === node.dataset.usageProvider);
    node.textContent = UsageStatus.stale(u) ? UsageStatus.text(u) : "";
  });
  const known = snap.usage.filter((u) => u.windows.length && u.fetched_ms);
  const oldest = known.reduce((a, u) => !a || u.fetched_ms < a.fetched_ms ? u : a, null);
  $("usage-meta").textContent = oldest ? UsageStatus.age(oldest) : "";
}, 30000);

// Shortcut boxes record the pressed combination instead of free text.
const KEY_NAMES = { " ": "space", arrowup: "up", arrowdown: "down", arrowleft: "left", arrowright: "right", escape: "esc" };
for (const input of document.querySelectorAll("input[name$=_hotkey]")) {
  input.addEventListener("keydown", (e) => {
    if (e.key === "Tab") return;
    e.preventDefault();
    const k = e.key.toLowerCase();
    if (k === "backspace" || k === "delete") { input.value = ""; input.dispatchEvent(new Event("input", { bubbles: true })); return; }
    if (["control", "shift", "alt", "meta"].includes(k)) return;
    const mods = [e.ctrlKey && "ctrl", e.shiftKey && "shift", e.altKey && "alt", e.metaKey && "super"].filter(Boolean);
    if (!mods.length) return; // a bare key would fire in every app
    const code = e.code.startsWith("Key") ? e.code.slice(3).toLowerCase() : e.code.startsWith("Digit") ? e.code.slice(5) : KEY_NAMES[k] || k;
    input.value = [...mods, code].join("+");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
// Shortcut boxes show the stored text ("ctrl+shift+t") as key caps (Ctrl Shift T). The input
// stays on top, transparent, so clicking and typing work as before; the caps are only a picture.
const CAP_NAMES = { ctrl: "Ctrl", shift: "Shift", alt: "Alt", super: "Win", space: "Space", esc: "Esc", up: "↑", down: "↓", left: "←", right: "→" };
const capLabel = (k) => CAP_NAMES[k] || (k.length === 1 ? k.toUpperCase() : k[0].toUpperCase() + k.slice(1));
function paintCaps(input) {
  const caps = input.previousElementSibling;
  const parts = input.value ? input.value.split("+") : [];
  caps.replaceChildren(...(parts.length ? parts.map((k) => el("kbd", null, capLabel(k))) : [el("span", "hk-none", input.placeholder ? "Not set" : "")]));
}
for (const input of document.querySelectorAll("input[name$=_hotkey]")) {
  const wrap = el("span", "hk"), caps = el("span", "hk-caps");
  caps.setAttribute("aria-hidden", "true");
  input.replaceWith(wrap);
  wrap.append(caps, input);
  input.addEventListener("input", () => paintCaps(input));
  paintCaps(input);
}
function paintAllCaps() { document.querySelectorAll("input[name$=_hotkey]").forEach(paintCaps); }
$("settings").elements.launcher_enabled.addEventListener("change", (e) => { $("settings").elements.launch_hotkey.disabled = !e.target.checked; });
$("settings").elements.dt_glance.addEventListener("change", (e) => { $("settings").elements.dt_glance_kind.disabled = !e.target.checked; syncDropdowns(); });
// The notch tab needs at least one section: the last checked box cannot be cleared.
$("dt-views").addEventListener("change", (e) => {
  if (!document.querySelector("#dt-views input:checked")) { e.target.checked = true; paintPreview(); }
});
// Accessible names: each control in a settings row is named by the row label (.rl)
// and described by the row text (.rd), so screen readers say "Start with Windows, switch, on".
let rowIds = 0;
function nameRows(root) {
  for (const row of root.querySelectorAll(".row")) {
    const rl = row.querySelector(".rl"), rd = row.querySelector(".rd");
    if (!rl) continue;
    if (!rl.id) rl.id = "rl-" + ++rowIds;
    if (rd && !rd.id) rd.id = "rd-" + rowIds;
    const by = (n, extra) => { n.setAttribute("aria-labelledby", [rl.id, extra].filter(Boolean).join(" ")); if (rd) n.setAttribute("aria-describedby", rd.id); };
    row.querySelectorAll(".switch input").forEach((i) => { i.setAttribute("role", "switch"); by(i); });
    row.querySelectorAll("input.key, input.num").forEach((i) => by(i));
    row.querySelectorAll(".seg, .days").forEach((g) => { g.setAttribute("role", g.classList.contains("seg") ? "radiogroup" : "group"); by(g); });
    row.querySelectorAll(".dd > button").forEach((b) => {
      const n = b.querySelector(".sr"), v = b.querySelector("span:not(.sr)");
      if (v && !v.id) v.id = "dv-" + ++rowIds;
      by(b, [n && n.id, v && v.id].filter(Boolean).join(" "));
    });
    row.querySelectorAll(".hook .btn, .dt-add .btn").forEach((b) => by(b, (b.id || (b.id = "rb-" + ++rowIds))));
  }
}
nameRows(document);
document.querySelectorAll(".swatches label").forEach((l) => l.querySelector("input").setAttribute("aria-label", l.title));
document.querySelectorAll(".rail button.on, .side button.on").forEach((b) => b.setAttribute("aria-current", "page"));
// No browser context menu (Back, Reload, Inspect) outside text fields.
document.addEventListener("contextmenu", (e) => { if (!e.target.closest("input, textarea, .dt-path")) e.preventDefault(); });
