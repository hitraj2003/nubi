window.addEventListener("error", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/panel.js: ${e.message} @${e.lineno}` }));
window.addEventListener("unhandledrejection", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/panel.js: ${e.reason}` }));
// Panel. All dynamic text is set with textContent: tool input and questions are untrusted.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);
const NAME = { claude: "Claude", codex: "Codex" };

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}
function btn(text, fn, cls) { const b = el("button", cls, text); b.type = "button"; b.addEventListener("click", fn); return b; }
function ago(ms) { const s = Math.max(0, Math.round((Date.now() - ms) / 1000)); return s < 60 ? s + "s ago" : s < 3600 ? Math.round(s / 60) + "m ago" : Math.round(s / 3600) + "h ago"; }
function resetIn(sec) {
  if (!sec) return "";
  const m = Math.round((sec * 1000 - Date.now()) / 60000);
  if (m <= 0) return "Resetting";
  return "Resets in " + (m < 60 ? m + "m" : m < 1440 ? Math.floor(m / 60) + "h " + (m % 60) + "m" : Math.round(m / 1440) + "d");
}
const prov = (p) => el("span", "prov " + p, NAME[p]);
const LABEL = { "5h": "Session", "7d": "Week", "7d Opus": "Week, Opus", "7d Sonnet": "Week, Sonnet" };

// ---- navigation ----
document.querySelectorAll("nav button[data-view]").forEach((b) =>
  b.addEventListener("click", () => {
    document.querySelectorAll("nav button[data-view]").forEach((x) => x.classList.toggle("on", x === b));
    document.querySelectorAll(".view").forEach((v) => v.classList.toggle("on", v.id === "v-" + b.dataset.view));
  }));

let snap = null, formDirty = false;
const picks = {};
let inboxIds = "";

// ---- inbox: built once per set of requests, countdowns update in place ----
function renderInbox(s) {
  $("inbox-meta").textContent = s.pending.length ? `${s.pending.length} waiting` : "";
  const ids = s.pending.map((p) => p.id).join(",");
  if (ids === inboxIds) {
    for (const p of s.pending) { const c = document.querySelector(`[data-left="${p.id}"]`); if (c) c.textContent = `Terminal takes over in ${Math.max(0, Math.round((p.expires_ms - Date.now()) / 1000))}s`; }
    return;
  }
  inboxIds = ids;
  const box = $("inbox");
  box.replaceChildren();
  if (!s.pending.length) { box.append(el("div", "empty", "Nothing waiting. Approvals and Claude's questions appear here.")); return; }
  for (const p of s.pending) {
    const it = el("div", "item " + p.provider);
    const h = el("div", "ih");
    h.append(prov(p.provider), el("span", "t", p.kind === "question" ? "Question" : `Allow ${p.tool_name || "tool"}?`), el("span", "sub", p.name || p.folder || "session"));
    const left = el("span", "left"); left.dataset.left = p.id; h.append(left);
    it.append(h);
    if (p.kind === "question") renderQuestion(it, p);
    else {
      if (p.tool_input) it.append(el("pre", null, p.tool_input));
      const a = el("div", "acts");
      a.append(btn("Allow", () => invoke("answer_permission", { id: p.id, decision: "allow" }), "primary"),
        btn("Deny", () => invoke("answer_permission", { id: p.id, decision: "deny" })), el("span", "grow"),
        btn("Jump to terminal", () => invoke("jump_session", { key: p.session_key }), "link"));
      it.append(a);
    }
    box.append(it);
  }
  renderInbox({ ...s, pending: s.pending }); // fill countdowns
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
        opts.querySelectorAll(".opt").forEach((r, j) => r.classList.toggle("on", chosen.has(q.options[j].label)));
      });
      opts.append(b);
    });
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
    btn("Jump to terminal", () => invoke("jump_session", { key: p.session_key }), "link"));
  it.append(a);
}

// ---- sessions ----
function renderSessions(s) {
  $("sessions-meta").textContent = s.sessions.length ? `${s.sessions.length} running` : "";
  const box = $("sessions");
  box.replaceChildren();
  box.style.display = s.sessions.length ? "" : "none";
  if (!s.sessions.length) { if (!$("sessions-empty")) { const e = el("div", "empty", "No sessions yet. Turn on hooks in Settings, then start Claude Code or Codex."); e.id = "sessions-empty"; box.after(e); } return; }
  if ($("sessions-empty")) $("sessions-empty").remove();
  for (const x of s.sessions) {
    const r = el("div", "r");
    const state = x.state + (x.attention && x.state !== "waiting" ? ", needs you" : "") + (x.subagents ? ` · ${x.subagents} agents` : "");
    r.append(el("span", "dot " + x.state), el("span", null, x.name || x.folder), prov(x.provider), el("span", "sub", state),
      el("span", "mono sub num", x.context_tokens ? Math.round(x.context_tokens / 1000) + "k" : "—"),
      el("span", "sub", ago(x.last_event_ms)),
      btn("Jump", () => invoke("jump_session", { key: x.key }), "link"));
    box.append(r);
  }
}

// ---- usage ----
function renderUsage(s) {
  const box = $("usage");
  box.replaceChildren();
  const ok = s.usage.find((u) => u.fetched_ms);
  $("usage-meta").textContent = ok ? "Updated " + ago(ok.fetched_ms) : "";
  if (!s.usage.length) box.append(el("div", "empty", "Loading"));
  for (const u of s.usage) {
    const c = el("div", "meter " + u.provider);
    const top = el("div", "top");
    top.append(prov(u.provider), el("span", "st" + (u.status === "ok" ? "" : " bad"), u.status === "ok" ? "" : (u.message || u.status)));
    c.append(top);
    for (const w of u.windows) {
      const d = el("div", "win");
      const wh = el("div", "wh");
      const big = el("span", "big", String(Math.round(w.used_percent)));
      big.append(el("small", null, "%"));
      wh.append(big, el("span", "sub", resetIn(w.resets_at)));
      const bar = el("div", "bar"), i = el("i");
      i.style.width = Math.min(100, w.used_percent) + "%";
      if (w.used_percent >= 70) i.className = "bad"; else if (w.used_percent >= 50) i.className = "warn";
      bar.append(i);
      d.append(wh, bar, el("div", "wl", LABEL[w.label] || w.label));
      c.append(d);
    }
    box.append(c);
  }
}

// ---- alerts ----
function renderAlerts(s) {
  const box = $("alerts");
  box.replaceChildren();
  box.style.display = s.alerts.length ? "" : "none";
  if (!s.alerts.length) { if (!$("alerts-empty")) { const e = el("div", "empty", "No alerts yet"); e.id = "alerts-empty"; box.after(e); } return; }
  if ($("alerts-empty")) $("alerts-empty").remove();
  for (const a of s.alerts) {
    const r = el("div", "r");
    const body = el("span", null, `${a.title} · ${a.body}`);
    if (a.queued) body.append(el("span", "sub", "  during do not disturb"));
    r.append(el("span", "mono sub", new Date(a.ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })), prov(a.provider), body);
    box.append(r);
  }
}

// ---- settings ----
function renderHooks(s) {
  const box = $("hooks");
  box.replaceChildren();
  for (const p of ["claude", "codex"]) {
    const on = !!s.hooks[p];
    const r = el("div", "row");
    const left = el("div");
    left.append(el("div", "rl", NAME[p]), el("div", "rd", p === "codex" && on ? "Run /hooks once in Codex to trust them" : p === "codex" ? "$CODEX_HOME/hooks.json" : "~/.claude/settings.json"));
    const right = el("span", "hook");
    right.append(el("span", "on-off" + (on ? " on" : ""), on ? "On" : "Off"),
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
  modeFields();
}
function modeFields() {
  const m = $("settings").elements.display_mode.value;
  document.querySelectorAll(".mode-notch").forEach((e) => (e.style.display = m === "island" ? "" : "none"));
  document.querySelectorAll(".mode-taskbar").forEach((e) => (e.style.display = m === "taskbar" ? "" : "none"));
}
// Settings save themselves. Switches, radios and selects save at once; typed fields (numbers,
// hotkey, quiet hours) save 700 ms after the last keystroke or when the field loses focus.
let autoTimer = 0;
const autoSave = () => { clearTimeout(autoTimer); $("settings").requestSubmit(); };
$("settings").addEventListener("input", (e) => {
  formDirty = true; modeFields();
  const t = e.target;
  const typed = t.tagName === "INPUT" && !["checkbox", "radio"].includes(t.type);
  clearTimeout(autoTimer);
  if (typed) autoTimer = setTimeout(autoSave, 700); else autoSave();
});
$("settings").addEventListener("change", (e) => { if (formDirty && e.target.tagName === "INPUT" && !["checkbox", "radio"].includes(e.target.type)) autoSave(); });
$("settings").addEventListener("submit", async (e) => {
  e.preventDefault();
  const f = e.target.elements;
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
  };
  try { await invoke("save_settings", { settings }); formDirty = false; $("saved").textContent = "Saved"; clearTimeout(savedTimer); savedTimer = setTimeout(() => ($("saved").textContent = ""), 1500); }
  catch (err) { $("saved").textContent = String(err); }
});

let savedTimer = 0;

// ---- shell ----
function render(s) {
  snap = s;
  $("dnd").checked = s.dnd;
  $("dnd-hint").textContent = s.dnd && !s.dnd_manual ? "On from quiet hours" : (s.settings.dnd_hotkey || "");
  $("pipe").textContent = s.pipe_error ? "Pipe error: " + s.pipe_error : "";
  const ni = $("n-inbox"); ni.textContent = s.pending.length || ""; ni.classList.toggle("hot", s.pending.length > 0);
  $("n-sessions").textContent = s.sessions.length || "";
  renderInbox(s); renderSessions(s); renderUsage(s); renderAlerts(s); renderHooks(s);
  if (!formDirty) fillForm(s.settings);
}
$("dnd").addEventListener("change", (e) => invoke("set_dnd", { on: e.target.checked }));
$("refresh").addEventListener("click", () => invoke("refresh_usage"));
$("test").addEventListener("click", () => invoke("test_alert"));
invoke("monitors").then((list) => list.forEach((label, i) => $("monitor").append(new Option(label.replace(/^\d+: /, "") || "Monitor " + (i + 1), String(i)))));
listen("state", (e) => render(e.payload));
invoke("get_state").then(render);
setInterval(() => { if (snap) { renderInbox(snap); renderSessions(snap); } }, 1000);
