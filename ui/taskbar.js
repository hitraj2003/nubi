// Taskbar strip, after CodeZeno/Claude-Code-Usage-Monitor (MIT, dea72c9): 10 segments of 10 px with
// 1 px gaps, "27% · 2h" values, half-step durations (format_duration_short).
// Rows: Claude + Codex on = Claude 5h, then Codex 5h. One provider = its 5h and 7d.
// Colours are the shared usage scale (usage-color.js) like the notch; the strip stays transparent.
// Bar values ride springs, so changes sweep.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const rowsEl = document.getElementById("rows");
const dot = document.getElementById("dot");
let snap = null, glyphs = {};

for (const p of ["claude", "codex"]) {
  fetch("glyphs/" + p + ".svg").then((r) => r.text()).then((t) => { glyphs[p] = t.replace(/<metadata>[\s\S]*?<\/metadata>/, ""); render(); }).catch(() => {});
}

function dur(sec) {
  const s = sec - Date.now() / 1000;
  if (s <= 0) return "now";
  const half = (x) => (Math.round(x * 2) / 2).toString();
  const h = s / 3600, d = s / 86400;
  if (d >= 10) return Math.round(d) + "d";
  if (d >= 1) return half(d) + "d";
  if (h >= 10) return Math.round(h) + "h";
  if (h >= 1) return half(h) + "h";
  if (s >= 60) return Math.min(59, Math.round(s / 60)) + "m";
  return Math.round(s) + "s";
}
function line(u, label) {
  if (!u || u.status === "loading") return { pct: null, text: "Loading" };
  const w = u.windows.find((x) => x.label === label);
  if (!w) return { pct: null, text: u.status === "needs_auth" ? "Sign in" : "No data" };
  const pct = Math.round(w.used_percent);
  return { pct, text: w.resets_at ? `${pct}% · ${dur(w.resets_at)}` : `${pct}%` };
}

function rowHtml(i, provider, label) {
  let segs = "";
  for (let k = 0; k < 10; k++) segs += `<div class="seg"></div>`;
  const icon = glyphs[provider] || "";
  return `<div class="row r${i} ${provider}"><span class="lab">${icon}${label}</span><div class="bar">${segs}</div><span class="val"></span></div>`;
}
// Segment fill for each row, from its spring value.
const springs = [new Motion.Spring(0, Motion.PRESET.content), new Motion.Spring(0, Motion.PRESET.content)];
const anim = new Motion.Animator({ a: springs[0], b: springs[1] }, (v) => { paint(0, v.a); paint(1, v.b); });
function paint(i, pct) {
  const segs = rowsEl.querySelectorAll(`.r${i} .seg`);
  const col = usageColor(Math.max(0, pct), false);
  segs.forEach((sg, k) => {
    const pc = (Math.max(0, Math.min(1, pct / 10 - k)) * 100).toFixed(1);
    sg.style.background = `linear-gradient(90deg,${col} ${pc}%,var(--track) ${pc}%)`;
  });
}
let lastRows = "";

function render() {
  if (!snap) return;
  const want = snap.settings.providers;
  const usage = (p) => (snap.usage || []).find((u) => u.provider === p);
  const rows = want === "both"
    ? [["claude", "5h"], ["codex", "5h"]]
    : [[want, "5h"], [want, "7d"]];
  const key = rows.join("|") + Object.keys(glyphs).join();
  if (key !== lastRows) { rowsEl.innerHTML = rows.map(([p, lab], i) => rowHtml(i, p, lab)).join(""); lastRows = key; }
  rows.forEach(([p, lab], i) => {
    const l = line(usage(p), lab);
    rowsEl.querySelector(`.r${i} .val`).textContent = l.text;
    springs[i].to(l.pct == null ? 0 : l.pct);
  });
  paint(0, springs[0].x); paint(1, springs[1].x);
  anim.kick();
  const tips = rows.map(([p, lab]) => {
    const u = usage(p);
    const name = p === "codex" ? "Codex" : "Claude";
    const w = u && u.windows.find((x) => x.label === lab);
    const value = w ? `${Math.round(w.used_percent)}% used${w.resets_at ? ", resets in " + dur(w.resets_at) : ""}` : line(u, lab).text;
    return `${name}, ${lab === "5h" ? "last 5 hours" : "this week"}: ${value}. ${UsageStatus.text(u)}`;
  });
  const waiting = (snap.pending || []).length;
  const missed = (snap.missed || []).filter((m) => want === "both" || m.provider === want).length;
  if (waiting) tips.push(`${waiting} questions & approvals need your OK`);
  if (missed) tips.push(`${missed} waiting in terminal`);
  dot.textContent = missed ? String(missed) : "";
  dot.classList.toggle("count", missed > 0);
  dot.title = missed ? `${missed} waiting in terminal` : "Needs your OK";
  document.body.title = tips.join("\n");
  dot.style.display = waiting || missed ? "block" : "none";
}

document.addEventListener("click", () => invoke("show_panel"));
document.addEventListener("contextmenu", (e) => { e.preventDefault(); invoke("show_panel"); });
listen("state", (e) => { snap = e.payload; render(); });
invoke("get_state").then((s) => { snap = s; render(); });
setInterval(render, 60000);
