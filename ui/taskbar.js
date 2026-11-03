// Taskbar strip, after CodeZeno/Claude-Code-Usage-Monitor (MIT, dea72c9): 10 segments of 10 px with
// 1 px gaps, "27% · 2h" values, half-step durations (format_duration_short).
// Rows: Claude + Codex on = Claude 5h, then Codex 5h. One provider = its 5h and 7d.
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
  if (!u || u.status === "loading") return { pct: null, text: "--" };
  const w = u.windows.find((x) => x.label === label);
  if (!w) return { pct: null, text: u.status === "ok" ? "--" : "!" };
  const pct = Math.round(w.used_percent);
  return { pct, text: w.resets_at ? `${pct}% · ${dur(w.resets_at)}` : `${pct}%` };
}

function rowHtml(i, provider, label, l) {
  let segs = "";
  for (let k = 0; k < 10; k++) {
    const fill = l.pct == null ? 0 : Math.max(0, Math.min(1, l.pct / 10 - k));
    const pc = (fill * 100).toFixed(0);
    segs += `<div class="seg" style="background:linear-gradient(90deg,var(--f) ${pc}%,var(--track) ${pc}%)"></div>`;
  }
  const icon = glyphs[provider] || "";
  return `<div class="row r${i} ${provider}"><span class="lab">${icon}${label}</span><div class="bar">${segs}</div><span class="val"></span></div>`;
}

function render() {
  if (!snap) return;
  const want = snap.settings.providers;
  const usage = (p) => (snap.usage || []).find((u) => u.provider === p);
  const rows = want === "both"
    ? [["claude", "5h"], ["codex", "5h"]]
    : [[want, "5h"], [want, "7d"]];
  rowsEl.innerHTML = rows.map(([p, lab], i) => rowHtml(i, p, lab, line(usage(p), lab))).join("");
  rows.forEach(([p, lab], i) => {
    rowsEl.querySelector(`.r${i} .val`).textContent = line(usage(p), lab).text;
  });
  const tips = rows.map(([p, lab]) => {
    const u = usage(p);
    const name = p === "codex" ? "Codex" : "Claude";
    return `${name} ${lab}: ${line(u, lab).text}${u && u.message ? " (" + u.message + ")" : ""}`;
  });
  const waiting = (snap.pending || []).length;
  if (waiting) tips.push(`${waiting} waiting for you — click to open`);
  document.body.title = tips.join("\n");
  dot.style.display = waiting ? "block" : "none";
}

document.addEventListener("click", () => invoke("show_panel"));
document.addEventListener("contextmenu", (e) => { e.preventDefault(); invoke("show_panel"); });
listen("taskbar_theme", (e) => document.body.classList.toggle("light", e.payload === "light"));
listen("state", (e) => { snap = e.payload; render(); });
invoke("get_state").then((s) => { snap = s; render(); });
setInterval(render, 60000);
