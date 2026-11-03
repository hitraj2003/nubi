// Functional panel. Everything rendered with textContent: tool input is untrusted.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);
const name = (p) => (p === "codex" ? "Codex" : "Claude");

function el(tag, text, cls) {
  const e = document.createElement(tag);
  if (text != null) e.textContent = text;
  if (cls) e.className = cls;
  return e;
}
function btn(text, fn) {
  const b = el("button", text);
  b.addEventListener("click", fn);
  return b;
}
function ago(ms) {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  return s < 60 ? s + "s ago" : s < 3600 ? Math.round(s / 60) + "m ago" : Math.round(s / 3600) + "h ago";
}
function until(sec) {
  if (!sec) return "reset unknown";
  const m = Math.max(0, Math.round(sec - Date.now() / 1000) / 60);
  return m < 60 ? Math.round(m) + "m" : m < 1440 ? Math.round(m / 60) + "h" : Math.round(m / 1440) + "d";
}

let formDirty = false;

function render(s) {
  const st = $("status");
  st.replaceChildren();
  st.append(el("div", s.dnd ? "Do Not Disturb: ON" : "Do Not Disturb: off", s.dnd ? "warn" : "muted"));
  st.append(btn(s.dnd_manual ? "Turn DND off" : "Turn DND on", () => invoke("set_dnd", { on: !s.dnd_manual })));
  if (s.pipe_error) st.append(el("div", "Pipe error: " + s.pipe_error, "err"));

  const pend = $("pending");
  pend.replaceChildren();
  if (!s.pending.length) pend.append(el("span", "none", "muted"));
  for (const p of s.pending) {
    const c = el("div", null, "card");
    c.append(el("div", `${name(p.provider)} · ${p.folder || "session"} · ${p.tool_name || "tool"}`));
    if (p.tool_input) c.append(el("pre", p.tool_input));
    c.append(el("div", "expires in " + Math.max(0, Math.round((p.expires_ms - Date.now()) / 1000)) + "s", "muted"));
    const answer = (d) => invoke("answer_permission", { id: p.id, decision: d }).catch((e) => alert(e));
    c.append(btn("Allow", () => answer("allow")), btn("Deny", () => answer("deny")),
             btn("Jump", () => invoke("jump_session", { key: p.session_key })));
    pend.append(c);
  }

  const us = $("usage");
  us.replaceChildren();
  for (const u of s.usage) {
    const r = el("div", null, "row");
    r.append(el("div", `${name(u.provider)}: ${u.status}${u.message ? " — " + u.message : ""}`,
                u.status === "ok" ? "" : "warn"));
    for (const w of u.windows) {
      const line = el("div");
      const bar = el("span", null, "bar");
      const fill = el("i");
      fill.style.width = Math.min(100, w.used_percent) + "%";
      bar.append(fill);
      line.append(el("span", w.label.padEnd(4) + " "), bar,
                  el("span", ` ${Math.round(w.used_percent)}% · ${until(w.resets_at)}`));
      r.append(line);
    }
    us.append(r);
  }
  if (!s.usage.length) us.append(el("span", "loading…", "muted"));

  const ss = $("sessions");
  ss.replaceChildren();
  if (!s.sessions.length) ss.append(el("span", "none yet (start Claude Code or Codex with hooks on)", "muted"));
  for (const x of s.sessions) {
    const r = el("div", null, "row");
    const ctx = x.context_tokens ? ` · ctx ${Math.round(x.context_tokens / 1000)}k` : "";
    const sub = x.subagents ? ` · ${x.subagents} sub-agent(s)` : "";
    r.append(el("div", `${name(x.provider)} · ${x.folder} · ${x.state}${x.attention ? " !" : ""}${sub}${ctx}`,
                x.attention ? "warn" : ""));
    r.append(el("div", `${x.last_event} ${ago(x.last_event_ms)}`, "muted"));
    r.append(btn("Jump to terminal", () => invoke("jump_session", { key: x.key }).then((ok) => { if (!ok) alert("Terminal window not found"); })));
    ss.append(r);
  }

  const al = $("alerts");
  al.replaceChildren();
  if (!s.alerts.length) al.append(el("span", "none", "muted"));
  for (const a of s.alerts.slice(0, 15)) {
    al.append(el("div", `${new Date(a.ts).toLocaleTimeString()} ${name(a.provider)} · ${a.title} · ${a.body}${a.queued ? " (DND)" : ""}`, "row"));
  }

  const hk = $("hooks");
  hk.replaceChildren();
  for (const p of ["claude", "codex"]) {
    const on = !!s.hooks[p];
    const r = el("div", `${name(p)} hooks: ${on ? "on" : "off"}`, "row");
    r.append(el("span", " "), btn(on ? "Disable" : "Enable", () =>
      invoke("set_hooks", { provider: p, enable: !on }).catch((e) => alert(e))));
    if (p === "codex" && on) r.append(el("div", "Codex: run /hooks once in Codex to trust the new hooks.", "muted"));
    hk.append(r);
  }

  if (!formDirty) fillForm(s.settings);
}

function fillForm(cfg) {
  const f = $("settings");
  for (const [k, v] of Object.entries(cfg)) {
    const i = f.elements[k];
    if (!i) continue;
    if (i.type === "checkbox") i.checked = !!v;
    else i.value = v == null ? "" : String(v);
  }
}

$("settings").addEventListener("input", () => (formDirty = true));
$("settings").addEventListener("submit", async (e) => {
  e.preventDefault();
  const f = e.target.elements;
  const num = (v) => (v === "" ? null : Number(v));
  const settings = {
    providers: f.providers.value,
    display_mode: f.display_mode.value,
    monitor: Number(f.monitor.value || 0),
    edge: f.edge.value,
    taskbar_side: f.taskbar_side.value,
    dnd_hotkey: f.dnd_hotkey.value,
    quiet_start: num(f.quiet_start.value),
    quiet_end: num(f.quiet_end.value),
    follow_quiet_time: f.follow_quiet_time.checked,
    sounds: f.sounds.checked,
    toast_when_fullscreen: f.toast_when_fullscreen.checked,
  };
  try {
    await invoke("save_settings", { settings });
    formDirty = false;
    $("saved").textContent = "saved";
  } catch (err) {
    $("saved").textContent = String(err);
  }
});
$("refresh").addEventListener("click", () => invoke("refresh_usage"));
$("test").addEventListener("click", () => invoke("test_alert"));

invoke("monitors").then((list) => {
  const m = $("monitor");
  list.forEach((label, i) => m.append(new Option(label, String(i))));
});
listen("state", (e) => render(e.payload));
invoke("get_state").then(render);
setInterval(() => invoke("get_state").then(render), 1000);
