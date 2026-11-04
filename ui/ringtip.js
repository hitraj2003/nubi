// Ring tooltip. Layout after claude-usage-meter tooltip_widget.py (MIT, 260e4f2); notch material and
// usage colours (usage-color.js: session rows in the lighter shade, like the inner ring). The card
// blurs in on a spring and bars sweep to their value. Text via textContent only.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const tip = document.getElementById("tip");
let snap = null, cur = { provider: "claude", mode: "5h" };
let shownFor = "";

const NAME = { claude: "Claude", codex: "Codex" };
const LABEL = { "5h": "Last 5 hours", "7d": "This week", "7d Opus": "This week, Opus", "7d Sonnet": "This week, Sonnet" };
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
  const u = (snap.usage || []).find((x) => x.provider === p) || { status: "loading", windows: [] };
  tip.append(el("div", "title", NAME[p]));
  tip.append(el("div", "tier", u.windows.length ? u.status === "ok" ? "Share of your limit used" : "Saved usage" : UsageStatus.text(u)));
  tip.append(el("div", "div"));
  const sweep = shownFor !== p + cur.mode; // new hover: bars sweep in from zero
  for (const w of u.windows) {
    const session = w.label === "5h";
    const row = el("div", "row" + (w.label === cur.mode ? " on" : ""));
    const head = el("div", "rh");
    head.append(el("span", "l", LABEL[w.label] || w.label), el("span", "v", Math.round(w.used_percent) + "% used"));
    const bar = el("div", "bar"), fill = el("div", "fill");
    const frac = Math.min(100, w.used_percent) / 100;
    fill.style.background = usageColor(w.used_percent, session);
    fill.style.transform = `scaleX(${sweep ? 0 : frac})`;
    bar.append(fill);
    row.append(head, bar, el("div", "reset", resetText(w, session)));
    tip.append(row);
    if (sweep) {
      const s = new Motion.Spring(0, Motion.PRESET.content).to(frac);
      new Motion.Animator({ s }, (v) => { fill.style.transform = `scaleX(${Math.max(0, v.s)})`; }).kick();
    }
  }

  const waiting = (snap.missed || []).filter((x) => {
    const want = snap.settings.providers;
    return want === "both" || x.provider === want;
  }).length;
  if (waiting) {
    tip.append(el("div", "waiting", `${waiting} waiting in terminal`));
    tip.append(el("div", "reset", "Click the ring count to open terminal. Right-click the count to dismiss."));
  }
  tip.append(el("div", "foot", u.windows.length ? UsageStatus.age(u) : ""));
  if (sweep) { tip.style.opacity = "0"; Motion.enter(tip); }
  shownFor = p + cur.mode;
}

listen("ringtip", (e) => { cur = e.payload; render(); });
listen("state", (e) => { snap = e.payload; render(); });
invoke("get_state").then((s) => { snap = s; render(); });
// Hidden tooltip: next hover blurs in again.
listen("ringtip_hide", () => { shownFor = ""; });
setInterval(() => {
  const u = snap?.usage.find((u) => u.provider === cur.provider);
  const foot = tip.querySelector(".foot");
  if (foot && u?.windows.length) foot.textContent = UsageStatus.age(u);
}, 30000);
