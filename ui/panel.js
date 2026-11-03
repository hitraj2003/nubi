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
function until(sec) {
  if (!sec) return "";
  const m = Math.round((sec * 1000 - Date.now()) / 60000);
  if (m <= 0) return "resetting";
  return "resets in " + (m < 60 ? m + "m" : m < 1440 ? Math.round(m / 60) + "h" : Math.round(m / 1440) + "d");
}
const tone = (p) => (p >= 70 ? "var(--crit)" : p >= 50 ? "var(--watch)" : "var(--ample)");
const tag = (p) => el("span", "tag " + p, NAME[p]);

// ---- navigation ----
document.querySelectorAll("#nav button[data-view]").forEach((b) =>
  b.addEventListener("click", () => {
    document.querySelectorAll("#nav button[data-view]").forEach((x) => x.classList.toggle("on", x === b));
    document.querySelectorAll(".view").forEach((v) => v.classList.toggle("on", v.id === "v-" + b.dataset.view));
  }));

let snap = null, formDirty = false;
const picks = {}; // question id -> { question: Set(labels) }

// ---- inbox ----
function renderInbox(s) {
  const box = $("inbox");
  // Keep focus in an "Other" box while typing: skip re-render if one is focused.
  if (box.contains(document.activeElement) && document.activeElement.tagName === "INPUT") return;
  box.replaceChildren();
  if (!s.pending.length) { box.append(el("div", "empty", "Nothing waiting. Approvals and Claude's questions show up here.")); return; }
  for (const p of s.pending) {
    const it = el("div", "item " + p.kind);
    const h = el("div", "ih");
    h.append(tag(p.provider), el("span", "t", p.kind === "question" ? "Question" : "Permission: " + (p.tool_name || "tool")),
      el("span", "tag", p.folder || "session"),
      el("span", "exp", "terminal takes over in " + Math.max(0, Math.round((p.expires_ms - Date.now()) / 1000)) + "s"));
    it.append(h);
    if (p.kind === "question") renderQuestion(it, p);
    else {
      if (p.tool_input) it.append(el("pre", null, p.tool_input));
      const a = el("div", "acts");
      a.append(btn("Allow", () => answer(p.id, "allow"), "primary"), btn("Deny", () => answer(p.id, "deny"), "danger"),
        btn("Jump to terminal", () => invoke("jump_session", { key: p.session_key }), "ghost"));
      it.append(a);
    }
    box.append(it);
  }
}
function answer(id, decision) { invoke("answer_permission", { id, decision }).catch((e) => alert(e)); }

function renderQuestion(it, p) {
  const sel = picks[p.id] || (picks[p.id] = {});
  (p.questions || []).forEach((q) => {
    const chosen = sel[q.question] || (sel[q.question] = new Set());
    const wrap = el("div", "q");
    const qt = el("div", "qt");
    if (q.header) qt.append(el("span", "qh", q.header));
    qt.append(document.createTextNode(q.question + (q.multiSelect ? " (pick any)" : "")));
    wrap.append(qt);
    const opts = el("div", "opts");
    for (const o of q.options || []) {
      const b = el("button", "opt" + (chosen.has(o.label) ? " on" : ""));
      b.type = "button";
      b.append(el("span", null, o.label));
      if (o.description) b.append(el("small", null, o.description));
      b.addEventListener("click", () => {
        if (q.multiSelect) chosen.has(o.label) ? chosen.delete(o.label) : chosen.add(o.label);
        else { chosen.clear(); chosen.add(o.label); }
        renderInbox(snap);
      });
      opts.append(b);
    }
    wrap.append(opts);
    const other = el("input", "other");
    other.placeholder = "Other answer…";
    const known = (l) => (q.options || []).some((o) => o.label === l);
    other.value = [...chosen].find((l) => !known(l)) || "";
    other.addEventListener("input", () => {
      for (const l of [...chosen]) if (!known(l)) chosen.delete(l);
      if (other.value.trim()) { if (!q.multiSelect) chosen.clear(); chosen.add(other.value.trim()); }
    });
    wrap.append(other);
    it.append(wrap);
  });
  const a = el("div", "acts");
  a.append(
    btn("Send answer", () => {
      const answers = {};
      for (const q of p.questions || []) {
        const c = sel[q.question];
        if (!c || !c.size) { alert("Answer every question first."); return; }
        answers[q.question] = [...c].join(", ");
      }
      invoke("answer_question", { id: p.id, answers }).catch((e) => alert(e));
      delete picks[p.id];
    }, "primary"),
    btn("Answer in terminal", () => { invoke("answer_question", { id: p.id, answers: {} }); delete picks[p.id]; }, "ghost"),
    btn("Jump to terminal", () => invoke("jump_session", { key: p.session_key }), "ghost"));
  it.append(a);
}

// ---- sessions ----
function renderSessions(s) {
  const tb = $("sessions");
  tb.replaceChildren();
  $("sessions-empty").style.display = s.sessions.length ? "none" : "block";
  for (const x of s.sessions) {
    const tr = el("tr", "state-" + x.state);
    const td = (c) => { const t = el("td"); if (c instanceof Node) t.append(c); else t.textContent = c; return t; };
    tr.append(td(el("span", "sdot")), td(x.folder), td(tag(x.provider)),
      td(x.state + (x.attention ? " · needs you" : "") + (x.subagents ? ` · ${x.subagents} sub-agent(s)` : "")),
      td(x.context_tokens ? Math.round(x.context_tokens / 1000) + "k tokens" : "—"),
      td(`${x.last_event} · ${ago(x.last_event_ms)}`),
      td(btn("Jump", () => invoke("jump_session", { key: x.key }).then((ok) => { if (!ok) alert("Terminal window not found."); }), "ghost")));
    tb.append(tr);
  }
}

// ---- usage ----
function renderUsage(s) {
  const box = $("usage");
  box.replaceChildren();
  if (!s.usage.length) box.append(el("div", "empty", "Loading…"));
  for (const u of s.usage) {
    const c = el("div", "ucard");
    const h = el("h3"); h.append(tag(u.provider)); c.append(h);
    c.append(el("div", "st" + (u.status === "ok" ? "" : " bad"), u.status === "ok" ? "Up to date" + (u.fetched_ms ? " · " + ago(u.fetched_ms) : "") : (u.message || u.status)));
    for (const w of u.windows) {
      const d = el("div", "w");
      const r = el("div", "wr");
      r.append(el("span", null, `${w.label} · ${Math.round(w.used_percent)}% used`), el("span", "r", until(w.resets_at)));
      const t = el("div", "track"), i = el("i");
      i.style.width = Math.min(100, w.used_percent) + "%";
      i.style.background = tone(w.used_percent);
      t.append(i);
      d.append(r, t);
      c.append(d);
    }
    box.append(c);
  }
}

// ---- alerts ----
const KCOL = { done: "var(--ample)", permission: "var(--watch)", question: "#0A84FF", idle: "#0A84FF", info: "#888" };
function renderAlerts(s) {
  const box = $("alerts");
  box.replaceChildren();
  if (!s.alerts.length) box.append(el("div", "empty", "No alerts yet."));
  for (const a of s.alerts) {
    const r = el("div", "a");
    const k = el("span", "k"); k.style.background = KCOL[a.kind] || "#888";
    const body = el("span");
    body.append(tag(a.provider), document.createTextNode(` ${a.title} · ${a.body}`));
    if (a.queued) body.append(el("span", "q", " (during DND)"));
    r.append(el("span", "tm", new Date(a.ts).toLocaleTimeString()), k, body);
    box.append(r);
  }
}

// ---- settings ----
function renderHooks(s) {
  const box = $("hooks");
  box.replaceChildren();
  for (const p of ["claude", "codex"]) {
    const on = !!s.hooks[p];
    const r = el("div", "hook");
    r.append(el("span", "nm", NAME[p]), el("span", on ? "on" : "off", on ? "On" : "Off"),
      btn(on ? "Disable" : "Enable", () => invoke("set_hooks", { provider: p, enable: !on }).catch((e) => alert(e)), "ghost"));
    if (p === "codex" && on) r.append(el("span", "hint", "Run /hooks once in Codex to trust them."));
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
  document.querySelectorAll(".mode-notch").forEach((e) => (e.style.display = m === "notch" ? "" : "none"));
  document.querySelectorAll(".mode-taskbar").forEach((e) => (e.style.display = m === "taskbar" ? "" : "none"));
}
$("settings").addEventListener("input", () => { formDirty = true; modeFields(); });
$("settings").addEventListener("submit", async (e) => {
  e.preventDefault();
  const f = e.target.elements;
  const num = (v) => (v === "" ? null : Number(v));
  const settings = {
    ...snap.settings,
    providers: f.providers.value,
    display_mode: f.display_mode.value,
    monitor: Number(f.monitor.value || 0),
    edge: f.edge.value,
    notch_on_hover: f.notch_on_hover.checked,
    notch_requests_layout: f.notch_requests_layout.value,
    taskbar_side: f.taskbar_side.value,
    dnd_hotkey: f.dnd_hotkey.value,
    quiet_start: num(f.quiet_start.value),
    quiet_end: num(f.quiet_end.value),
    follow_quiet_time: f.follow_quiet_time.checked,
    sounds: f.sounds.checked,
    toast_when_fullscreen: f.toast_when_fullscreen.checked,
  };
  try { await invoke("save_settings", { settings }); formDirty = false; $("saved").textContent = "Saved"; setTimeout(() => ($("saved").textContent = ""), 2000); }
  catch (err) { $("saved").textContent = String(err); }
});

// ---- shell ----
function render(s) {
  snap = s;
  $("dnd").checked = s.dnd;
  $("dnd-hint").textContent = s.dnd && !s.dnd_manual ? "On from quiet hours or Windows quiet time" : `Hotkey: ${s.settings.dnd_hotkey || "none"}`;
  $("pipe").textContent = s.pipe_error ? "Pipe error: " + s.pipe_error : "";
  const n = s.pending.length, ni = $("n-inbox");
  ni.textContent = n; ni.classList.toggle("show", n > 0);
  const ns = $("n-sessions");
  ns.textContent = s.sessions.length; ns.classList.toggle("show", s.sessions.length > 0); ns.classList.add("muted");
  renderInbox(s); renderSessions(s); renderUsage(s); renderAlerts(s); renderHooks(s);
  if (!formDirty) fillForm(s.settings);
}
$("dnd").addEventListener("change", (e) => invoke("set_dnd", { on: e.target.checked }));
$("refresh").addEventListener("click", () => invoke("refresh_usage"));
$("test").addEventListener("click", () => invoke("test_alert"));
invoke("monitors").then((list) => list.forEach((label, i) => $("monitor").append(new Option(label, String(i)))));
listen("state", (e) => render(e.payload));
invoke("get_state").then(render);
setInterval(() => invoke("get_state").then(render), 1000);
