// Ring tooltip, after claude-usage-meter tooltip_widget.py (MIT, 260e4f2). Text via textContent only.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const tip = document.getElementById("tip");
let snap = null, cur = { provider: "claude", mode: "5h" };

const NAME = { claude: "Claude", codex: "Codex" };
const ACCENT = { claude: ["#d9773c", "#e8a838", "#e74c3c"], codex: ["#2f80ed", "#56ccf2", "#eb5757"] };
const LABEL = { "5h": "Current session (5h)", "7d": "Weekly (all models)", "7d Opus": "Weekly (Opus)", "7d Sonnet": "Weekly (Sonnet)" };
function color(p, pct) { const a = ACCENT[p]; return pct > 80 ? a[2] : pct > 50 ? a[1] : a[0]; }
function el(tag, cls, text) { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; }

function resetText(w, session) {
  if (!w.resets_at) return "";
  const d = new Date(w.resets_at * 1000);
  if (!session) return "resets on " + d.toLocaleDateString("en-US", { month: "short", day: "2-digit" });
  const mins = Math.max(0, Math.floor((d - Date.now()) / 60000)), h = Math.floor(mins / 60), m = mins % 60;
  return h > 0 ? `resets in ${h}h ${m}m` : `resets in ${m}m`;
}

function render() {
  tip.replaceChildren();
  if (!snap) return;
  const p = cur.provider;
  const u = (snap.usage || []).find((x) => x.provider === p);
  if (!u || !u.windows) { tip.append(el("div", "loading", "Loading...")); return; }
  tip.append(el("div", "title", NAME[p]));
  tip.append(el("div", "tier", u.status === "ok" ? "Usage limits" : u.status));
  tip.append(el("div", "div"));
  for (const w of u.windows) {
    const row = el("div", "row" + (w.label === cur.mode ? " on" : ""));
    const head = el("div", "rh");
    head.append(el("span", "l", LABEL[w.label] || w.label));
    const pct = el("span", null, Math.round(w.used_percent) + "%");
    pct.style.color = color(p, w.used_percent);
    head.append(pct);
    const bar = el("div", "bar"), fill = el("div", "fill");
    fill.style.width = Math.min(100, w.used_percent) + "%";
    fill.style.background = color(p, w.used_percent);
    bar.append(fill);
    row.append(head, bar, el("div", "reset", resetText(w, w.label === "5h")));
    tip.append(row);
  }
  if (u.message && u.status !== "ok") tip.append(el("div", "warn", u.message));
  const m = u.fetched_ms ? Math.round((Date.now() - u.fetched_ms) / 60000) : null;
  tip.append(el("div", "foot", m == null ? "" : m < 1 ? "Updated just now" : `Updated ${m}m ago`));
}

listen("ringtip", (e) => { cur = e.payload; render(); });
listen("state", (e) => { snap = e.payload; render(); });
invoke("get_state").then((s) => { snap = s; render(); });
