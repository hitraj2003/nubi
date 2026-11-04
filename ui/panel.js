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
const STATE = { working: "Working", waiting: "Waiting", done: "Done", idle: "Idle" };

// Children come in one after another on the notch's content spring (blur + rise).
function stagger(nodes, step = 35) {
  [...nodes].forEach((n, i) => { n.style.opacity = "0"; setTimeout(() => enter(n, PRESET.content), i * step); });
}
pressable(document.body);

// ---- navigation: the rail tile slides between items ----
const railHl = vslider(document.querySelector(".rail-hl"));
function go(view) {
  document.querySelectorAll(".rail button[data-view]").forEach((x) => x.classList.toggle("on", x.dataset.view === view));
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
function openLeave() { $("leave").hidden = false; enter($("leave").querySelector(".box"), PRESET.open); $("leave-save").focus(); }
const closeLeave = () => { $("leave").hidden = true; leavingTo = null; };
$("leave").addEventListener("click", (e) => { if (e.target === $("leave")) closeLeave(); });
addEventListener("keydown", (e) => { if (e.key === "Escape" && !$("leave").hidden) closeLeave(); });
requestAnimationFrame(() => railHl(document.querySelector(".rail button.on")));
// The settings button sits at the bottom of the rail, so it moves when the window resizes.
new ResizeObserver(() => railHl(document.querySelector(".rail button.on"))).observe(document.querySelector(".rail"));

// ---- settings sections: highlight slides, pane blurs in ----
const sideHl = vslider(document.querySelector(".side-hl"));
document.querySelectorAll(".side button[data-pane]").forEach((b) => b.addEventListener("click", () => {
  document.querySelectorAll(".side button[data-pane]").forEach((x) => x.classList.toggle("on", x === b));
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
  const ul = el("ul"); ul.setAttribute("role", "listbox");
  dd.append(b, ul);
  sel.after(dd);
  let act = -1;
  const items = () => [...ul.children];
  const mark = () => items().forEach((li, i) => li.classList.toggle("act", i === act));
  const close = () => { dd.classList.remove("open"); act = -1; mark(); };
  const pick = (v) => { sel.value = v; sync(); close(); sel.dispatchEvent(new Event("input", { bubbles: true })); b.focus(); };
  function sync() {
    ul.replaceChildren();
    for (const o of sel.options) {
      const li = el("li", o.value === sel.value ? "sel" : "", o.textContent);
      li.setAttribute("role", "option");
      li.addEventListener("click", () => pick(o.value));
      ul.append(li);
    }
    label.textContent = sel.selectedOptions[0] ? sel.selectedOptions[0].textContent : "";
  }
  b.addEventListener("click", () => {
    const open = !dd.classList.contains("open");
    dropdowns.forEach((d) => d.close());
    if (open) { dd.classList.add("open"); act = sel.selectedIndex; mark(); stagger(ul.children, 20); }
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
document.querySelectorAll("#settings select").forEach(enhanceSelect);
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
  $("sessions-meta").textContent = s.sessions.length ? `${s.sessions.length} running` : "";
  renderRequests($("session-requests"), s);
  const box = $("sessions");
  const keys = list.map((x) => x.key).join(",");
  const changed = keys !== sessionKeys; sessionKeys = keys;
  box.replaceChildren();
  if (!list.length) { box.append(el("div", "empty", s.sessions.length ? "No sessions for this filter." : "No sessions yet. Turn on Connections in Settings, then start Claude Code or Codex.")); return; }
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
      inner.append(b, el("small", null, "used, " + (LABEL[w0.label] || w0.label).toLowerCase()));
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
    const status = el("span", "st", UsageStatus.text(u));
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
  if (!s.alerts.length) { box.append(el("div", "empty", "No alerts yet")); return; }
  for (const a of s.alerts) {
    const r = el("div", "al");
    const tx = el("div", "tx", `${a.title} · ${a.body}`);
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
  syncDropdowns();
  modeFields();
  window.nubiTheme.apply({ theme: cfg.theme, accent: cfg.accent, reduce_motion: cfg.reduce_motion });
  requestAnimationFrame(syncThumbs);
}
function modeFields() {
  const m = $("settings").elements.display_mode.value;
  document.querySelectorAll(".mode-notch").forEach((e) => (e.style.display = m === "island" ? "" : "none"));
  document.querySelectorAll(".mode-taskbar").forEach((e) => (e.style.display = m === "taskbar" ? "" : "none"));
}
// Changes are a draft until Save. Theme, accent and motion preview live; Discard reverts them.
function setDirty(d) {
  const was = formDirty;
  formDirty = d; window.nubiTheme.hold = d;
  $("draftbar").hidden = !d;
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
  };
  try {
    await invoke("save_settings", { settings });
    setDirty(false);
    $("saved").textContent = "Saved"; clearTimeout(savedTimer); savedTimer = setTimeout(() => ($("saved").textContent = ""), 1500);
    return true;
  } catch (err) { $("saved").textContent = String(err); return false; }
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
  $("dnd-wrap").title = s.dnd ? (s.dnd_manual ? "Do not disturb is on" : "Do not disturb, on from quiet hours") : "Do not disturb (" + (s.settings.dnd_hotkey || "off") + ")";
  $("pipe").textContent = s.pipe_error ? "Pipe error: " + s.pipe_error : "";
  const ni = $("n-inbox");
  ni.textContent = s.pending.length || ""; ni.hidden = !s.pending.length;
  if (s.pending.length > lastPending) tween(ni, { scale: 0.4, opacity: 0 }, {}, PRESET.press);
  lastPending = s.pending.length;
  $("inbox-meta").textContent = s.pending.length ? `${s.pending.length} waiting` : "";
  renderRequests($("inbox"), s); renderSessions(s); renderUsage(s); renderAlerts(s); renderHooks(s);
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
  $("rep").hidden = false; $("rep-status").textContent = ""; $("rep-text").focus();
  repCtx = await invoke("report_context").catch(() => null);
  if (repCtx && !repF("setup").value) repF("setup").value = repCtx.setup || "";
});
$("rep-cancel").addEventListener("click", () => ($("rep").hidden = true));
$("rep-manual").addEventListener("click", () => { if (!repF("what-happened").value) repF("what-happened").value = $("rep-text").value; repShowFields(); });
document.querySelectorAll("#rep [data-agent]").forEach((b) => b.addEventListener("click", async () => {
  const text = $("rep-text").value.trim();
  if (!text) { $("rep-status").textContent = "Write a sentence first."; $("rep-text").focus(); return; }
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
  $("rep").hidden = true;
});

// ---- version history ----
async function loadReleases() {
  const box = $("releases");
  try {
    const list = await invoke("list_releases");
    if (!list.length) { box.replaceChildren(el("div", "rd", "No releases yet.")); return; }
    box.replaceChildren(...list.map((r) => {
      const row = el("div", "rel");
      const info = el("div");
      info.append(el("div", "rl", "Version " + r.version + (r.current ? " (installed)" : "") + (r.prerelease ? " · pre-release" : "")), el("div", "rd", r.date));
      const notes = el("button", "btn ghost", "Notes"); notes.type = "button";
      notes.addEventListener("click", () => invoke("open_release_link", { url: r.notes }));
      const dl = el("button", "btn", "Download"); dl.type = "button";
      if (r.download) dl.addEventListener("click", () => invoke("open_release_link", { url: r.download }));
      else { dl.disabled = true; dl.title = "No Windows installer in this release"; }
      const acts = el("span", "hook"); acts.append(notes, dl);
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
    node.textContent = UsageStatus.text(u);
  });
  const known = snap.usage.filter((u) => u.windows.length && u.fetched_ms);
  const oldest = known.reduce((a, u) => !a || u.fetched_ms < a.fetched_ms ? u : a, null);
  $("usage-meta").textContent = oldest ? UsageStatus.age(oldest) : "";
}, 30000);
