window.addEventListener("error", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/notch.js: ${e.message} @${e.lineno}` }));
window.addEventListener("unhandledrejection", (e) => window.__TAURI__.core.invoke("log_js", { msg: `ui/notch.js: ${e.reason}` }));
// Edge notch. Ring drawing, tones, card placement and fold timing follow vinzdg/codenotch
// windows/codenotch/ui/notch.html (MIT, commit 6522e9f). All dynamic text goes through esc().
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const AMPLE = "#00FF88", WATCH = "#F2FF00", CRIT = "#FF3F00";
const TRACK = "#303030", INK = "#ffffff", HOLE = "#2a2a2a";
const watchLimit = 0.5, criticalLimit = 0.7;
const tone = (f) => (f >= criticalLimit ? CRIT : f >= watchLimit ? WATCH : AMPLE);
const TAIL_GAP = 10.5, TAIL_LENGTH = 28.2;
const FOLD_GRACE = 450;
const SESSION_ROWS = 5;
const STATE_DOT = { working: INK, waiting: WATCH, idle: TRACK, done: AMPLE, stale: TRACK };

const pill = document.getElementById("pill");
const card = document.getElementById("card");
const tail = document.getElementById("tail");
const rest = document.getElementById("rest");
const reqs = document.getElementById("reqs");
const root = document.getElementById("root");

let snap = null;
let edge = "right";
let hoverId = null;
let folded = false;
let pointerIn = false;
let foldTimer = null, hideTimer = null;
const glyphs = {};

function esc(s) { const d = document.createElement("div"); d.textContent = s == null ? "" : String(s); return d.innerHTML; }
const NAME = { claude: "Claude", codex: "Codex" };

for (const p of ["claude", "codex"]) {
  fetch("glyphs/" + p + ".svg").then((r) => r.text()).then((t) => {
    // Local, bundled files: strip metadata so only the path drawing is inlined.
    glyphs[p] = t.replace(/<metadata>[\s\S]*?<\/metadata>/, "");
    render();
  }).catch(() => {});
}

function svgArc(r, frac, color, width, extra = "") {
  if (!(frac > 0)) return "";
  const C = 2 * Math.PI * r;
  return `<circle cx="28" cy="28" r="${r}" fill="none" stroke="${color}" stroke-width="${width}"
    stroke-dasharray="${(C * frac).toFixed(2)} ${C.toFixed(2)}" stroke-linecap="round"
    transform="rotate(-90 28 28)" ${extra}/>`;
}

function providers() {
  if (!snap) return [];
  const want = snap.settings.providers;
  return ["claude", "codex"].filter((p) => want === "both" || want === p);
}
function usageOf(p) { return (snap.usage || []).find((u) => u.provider === p) || { status: "loading", windows: [] }; }
function sessionsOf(p) { return (snap.sessions || []).filter((s) => s.provider === p); }
function pendingOf(p) { return (snap.pending || []).filter((x) => x.provider === p); }
function headline(u) { return u.windows.find((w) => w.label === "5h") || u.windows[0] || null; }
function weekly(u) { return u.windows.find((w) => w.label === "7d") || null; }
function workState(p) {
  const ss = sessionsOf(p);
  if (pendingOf(p).length || ss.some((s) => s.state === "waiting" || (s.attention && s.state === "idle"))) return "attention";
  if (ss.some((s) => s.state === "working")) return "running";
  return "idle";
}

function renderRing() {
  const ps = providers();
  const want = ps.join(",") + "|" + ps.map((p) => (glyphs[p] ? 1 : 0)).join("");
  if (pill.dataset.cells !== want) {
    pill.innerHTML = ps.map((p, i) => `<div class="cell" data-p="${p}" style="--i:${i}">
      <div class="ringwrap"><svg class="ring" viewBox="0 0 56 56"></svg><svg class="reading" viewBox="0 0 56 56"></svg><svg class="activity" viewBox="0 0 56 56"></svg><div class="glyph">${glyphs[p] || esc(NAME[p][0])}</div></div>
      <div class="pct">—</div></div>`).join("");
    pill.dataset.cells = want;
  }
  for (const p of ps) {
    const cell = pill.querySelector(`.cell[data-p="${p}"]`);
    if (!cell) continue;
    const u = usageOf(p), h = headline(u), wk = weekly(u);
    let inner = `<circle cx="28" cy="28" r="22" fill="${HOLE}"/><circle cx="28" cy="28" r="25" fill="none" stroke="${TRACK}" stroke-width="5"/>`;
    if (wk && wk !== h) {
      const f = Math.min(Math.max(wk.used_percent / 100, 0), 1);
      inner += `<circle cx="28" cy="28" r="31" fill="none" stroke="${TRACK}" stroke-width="2.4" opacity="0.7"/>` +
        svgArc(31, f, tone(f), 2.4, 'opacity="0.85"');
    }
    cell.querySelector("svg.ring").innerHTML = inner;
    const hf = h ? Math.min(Math.max(h.used_percent / 100, 0), 1) : 0;
    cell.querySelector("svg.reading").innerHTML = h ? svgArc(25, hf, tone(hf), 5) : "";
    const ws = workState(p);
    cell.querySelector("svg.activity").innerHTML =
      ws === "running" ? `<g class="arc-spin">${svgArc(19, 0.28, INK, 2.5)}</g>` :
      ws === "attention" ? `<circle cx="28" cy="28" r="19" fill="none" stroke="${WATCH}" stroke-width="2.5"/>` : "";
    cell.querySelector(".pct").textContent = h ? Math.round(h.used_percent) + "%" : (u.status === "loading" ? "…" : "—");
    cell.querySelector(".glyph").classList.toggle("dim", !!(h && h.used_percent >= 100));
    cell.querySelector(".ringwrap").classList.toggle("stale", u.status !== "ok" && !!h);
  }
}

function resetCopy(sec) {
  if (!sec) return "";
  const diff = sec * 1000 - Date.now();
  if (diff <= 0) return "Resetting";
  const min = Math.round(diff / 60000);
  if (min < 60) return `Resets in ${Math.max(1, min)} min`;
  const d = new Date(sec * 1000);
  const t = d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (diff < 864e5) return "Resets " + t;
  return "Resets " + d.toLocaleDateString([], { weekday: "short" }) + " " + t;
}
function ago(ms) { const m = Math.round((Date.now() - ms) / 60000); return m < 1 ? "now" : m < 60 ? m + "m" : Math.round(m / 60) + "h"; }

function renderCard() {
  if (!card.classList.contains("show") || !hoverId || !snap) return;
  const p = hoverId, u = usageOf(p);
  let html = `<div class="c-head"><span class="mark">${glyphs[p] || ""}</span><span class="c-title">${esc(NAME[p])}</span></div>`;
  html += `<div class="c-sub">${esc(u.status === "ok" ? "Usage limits" : (u.message || u.status))}</div>`;
  for (const w of u.windows) {
    const f = Math.min(Math.max(w.used_percent / 100, 0), 1);
    html += `<div class="win"><div class="w-row"><span class="w-label">${esc(w.label)}</span><span class="w-reset">${esc(resetCopy(w.resets_at))}</span></div>
      <div class="w-track"><div class="w-fill" style="width:${(f * 100).toFixed(1)}%;background:${tone(f)}"></div></div>
      <div class="w-used">${Math.round(w.used_percent)}% used</div></div>`;
  }
  const waiting = pendingOf(p).length;
  if (waiting) html += `<div class="c-auth"><button data-requests="1">${waiting} waiting for you</button></div>`;
  const ss = sessionsOf(p);
  if (pendingOf(p).length || ss.some((s) => s.state === "waiting" || (s.attention && s.state === "idle"))) return "attention";
  if (ss.some((s) => s.state === "working")) return "running";
  return "idle";
}

function renderRing() {
  const ps = providers();
  const want = ps.join(",") + "|" + ps.map((p) => (glyphs[p] ? 1 : 0)).join("");
  if (pill.dataset.cells !== want) {
    pill.innerHTML = ps.map((p, i) => `<div class="cell" data-p="${p}" style="--i:${i}">
      <div class="ringwrap"><svg class="ring" viewBox="0 0 56 56"></svg><svg class="reading" viewBox="0 0 56 56"></svg><svg class="activity" viewBox="0 0 56 56"></svg><div class="glyph">${glyphs[p] || esc(NAME[p][0])}</div></div>
      <div class="pct">—</div></div>`).join("");
    pill.dataset.cells = want;
  }
  for (const p of ps) {
    const cell = pill.querySelector(`.cell[data-p="${p}"]`);
    if (!cell) continue;
    const u = usageOf(p), h = headline(u), wk = weekly(u);
    let inner = `<circle cx="28" cy="28" r="22" fill="${HOLE}"/><circle cx="28" cy="28" r="25" fill="none" stroke="${TRACK}" stroke-width="5"/>`;
    if (wk && wk !== h) {
      const f = Math.min(Math.max(wk.used_percent / 100, 0), 1);
      inner += `<circle cx="28" cy="28" r="31" fill="none" stroke="${TRACK}" stroke-width="2.4" opacity="0.7"/>` +
        svgArc(31, f, tone(f), 2.4, 'opacity="0.85"');
    }
    cell.querySelector("svg.ring").innerHTML = inner;
    const hf = h ? Math.min(Math.max(h.used_percent / 100, 0), 1) : 0;
    cell.querySelector("svg.reading").innerHTML = h ? svgArc(25, hf, tone(hf), 5) : "";
    const ws = workState(p);
    cell.querySelector("svg.activity").innerHTML =
      ws === "running" ? `<g class="arc-spin">${svgArc(19, 0.28, INK, 2.5)}</g>` :
      ws === "attention" ? `<g class="arc-pulse"><circle cx="28" cy="28" r="19" fill="none" stroke="${WATCH}" stroke-width="2.5"/></g>` : "";
    cell.querySelector(".pct").textContent = h ? Math.round(h.used_percent) + "%" : (u.status === "loading" ? "…" : "—");
    cell.querySelector(".glyph").classList.toggle("dim", !!(h && h.used_percent >= 100));
    cell.querySelector(".ringwrap").classList.toggle("stale", u.status !== "ok" && !!h);
  }
}

function resetCopy(sec) {
  if (!sec) return "";
  const diff = sec * 1000 - Date.now();
  if (diff <= 0) return "Resetting";
  const min = Math.round(diff / 60000);
  if (min < 60) return `Resets in ${Math.max(1, min)} min`;
  const d = new Date(sec * 1000);
  const t = d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (diff < 864e5) return "Resets " + t;
  return "Resets " + d.toLocaleDateString([], { weekday: "short" }) + " " + t;
}
function ago(ms) { const m = Math.round((Date.now() - ms) / 60000); return m < 1 ? "now" : m < 60 ? m + "m" : Math.round(m / 60) + "h"; }

function renderCard() {
  if (!card.classList.contains("show") || !hoverId || !snap) return;
  const p = hoverId, u = usageOf(p);
  let html = `<div class="c-head"><span class="mark">${glyphs[p] || ""}</span><span class="c-title">${esc(NAME[p])}</span></div>`;
  html += `<div class="c-sub">${esc(u.status === "ok" ? "Usage limits" : (u.message || u.status))}</div>`;
  for (const w of u.windows) {
    const f = Math.min(Math.max(w.used_percent / 100, 0), 1);
    html += `<div class="win"><div class="w-row"><span class="w-label">${esc(w.label)}</span><span class="w-reset">${esc(resetCopy(w.resets_at))}</span></div>
      <div class="w-track"><div class="w-fill" style="width:${(f * 100).toFixed(1)}%;background:${tone(f)}"></div></div>
      <div class="w-used">${Math.round(w.used_percent)}% used</div></div>`;
  }
  for (const a of pendingOf(p).filter((x) => x.kind === "question")) html += questionHtml(a);
  for (const a of pendingOf(p).filter((x) => x.kind !== "question")) {
    html += `<div class="g-head">Permission needed</div><div class="g-box">
      <div class="c-note">${esc(a.folder || "session")} · ${esc(a.tool_name || "tool")}</div>
      ${a.tool_input ? `<div class="cmd">${esc(a.tool_input)}</div>` : ""}
      <div class="c-auth"><button data-allow="${a.id}">Allow</button><button data-deny="${a.id}">Deny</button><button data-jump="${esc(a.session_key)}">Jump</button></div></div>`;
  }
  const ss = sessionsOf(p);
  if (ss.length) {
    html += `<div class="c-sessions">`;
    for (const s of ss.slice(0, SESSION_ROWS)) {
      const ctx = s.context_tokens ? ` · ${Math.round(s.context_tokens / 1000)}k` : "";
      html += `<div class="s-row" data-jump="${esc(s.key)}" title="Jump to terminal"><span class="s-dot" style="background:${STATE_DOT[s.state] || TRACK}"></span>${esc(s.folder)} · ${esc(s.state)}${esc(ctx)} · ${esc(ago(s.last_event_ms))}</div>`;
    }
    if (ss.length > SESSION_ROWS) html += `<div class="s-more">and ${ss.length - SESSION_ROWS} more</div>`;
    html += `</div>`;
  }
  card.innerHTML = html;
  placeCard();
  reportHot();
}

// ---- AskUserQuestion cards (Claude). Picks live here until sent. ----
const picks = {}; // pending id -> { questionText: Set(labels) }
function questionHtml(a) {
  const sel = picks[a.id] || (picks[a.id] = {});
  let h = `<div class="g-head">Question</div><div class="g-box" data-qid="${a.id}">`;
  (a.questions || []).forEach((q, qi) => {
    const chosen = sel[q.question] || new Set();
    h += `<div class="c-note"><b>${esc(q.header || "")}</b> ${esc(q.question)}${q.multiSelect ? " <span class='w-used'>(pick any)</span>" : ""}</div><div class="c-auth">`;
    (q.options || []).forEach((o) => {
      const on = chosen.has(o.label);
      h += `<button class="opt${on ? " on" : ""}" data-q="${qi}" data-label="${esc(o.label)}" title="${esc(o.description || "")}">${on ? "✓ " : ""}${esc(o.label)}</button>`;
    });
    h += `</div><input class="other" data-q="${qi}" placeholder="Other…" value="${esc([...chosen].find((l) => !(q.options || []).some((o) => o.label === l)) || "")}">`;
  });
  h += `<div class="c-auth"><button data-send="${a.id}">Send</button><button data-skip="${a.id}">Answer in terminal</button><button data-jump="${esc(a.session_key)}">Jump</button></div>
    <div class="w-used">Terminal asks instead in ${Math.max(0, Math.round((a.expires_ms - Date.now()) / 1000))}s</div></div>`;
  return h;
}
function pendingById(id) { return (snap.pending || []).find((x) => x.id === id); }
root.addEventListener("input", (e) => {
  const inp = e.target.closest("input.other");
  if (!inp) return;
  const a = pendingById(Number(inp.closest("[data-qid]").dataset.qid));
  const q = a && a.questions[Number(inp.dataset.q)];
  if (!q) return;
  const sel = picks[a.id][q.question] || (picks[a.id][q.question] = new Set());
  for (const l of [...sel]) if (!(q.options || []).some((o) => o.label === l)) sel.delete(l);
  if (inp.value.trim()) { if (!q.multiSelect) sel.clear(); sel.add(inp.value.trim()); }
});
root.addEventListener("click", (e) => {
  if (e.target.closest("[data-requests]")) { invoke("show_requests"); return; }
  const opt = e.target.closest("button.opt");
  if (opt) {
    const a = pendingById(Number(opt.closest("[data-qid]").dataset.qid));
    const q = a && a.questions[Number(opt.dataset.q)];
    if (!q) return;
    const sel = picks[a.id][q.question] || (picks[a.id][q.question] = new Set());
    const label = opt.dataset.label;
    if (q.multiSelect) sel.has(label) ? sel.delete(label) : sel.add(label);
    else { sel.clear(); sel.add(label); }
    renderCard();
    renderReqs();
    return;
  }
  const send = e.target.closest("[data-send],[data-skip]");
  if (send) {
    const id = Number(send.dataset.send || send.dataset.skip);
    const answers = {};
    if (send.dataset.send) {
      const a = pendingById(id);
      for (const q of (a && a.questions) || []) {
        const sel = picks[id][q.question];
        if (!sel || !sel.size) return; // every question needs an answer
        answers[q.question] = [...sel].join(", ");
      }
    }
    invoke("answer_question", { id, answers }).catch(() => {});
    delete picks[id];
    return;
  }
  const t = e.target.closest("[data-allow],[data-deny],[data-jump]");
  if (!t) return;
  if (t.dataset.allow) invoke("answer_permission", { id: Number(t.dataset.allow), decision: "allow" }).catch(() => {});
  else if (t.dataset.deny) invoke("answer_permission", { id: Number(t.dataset.deny), decision: "deny" }).catch(() => {});
  else invoke("jump_session", { key: t.dataset.jump }).catch(() => {});
});

function permHtml(a) {
  return `<div class="g-head">Permission needed</div><div class="c-note">${esc(NAME[a.provider])} · ${esc(a.folder || "session")} · ${esc(a.tool_name || "tool")}</div>
    ${a.tool_input ? `<div class="cmd">${esc(a.tool_input)}</div>` : ""}
    <div class="c-auth"><button data-allow="${a.id}">Allow</button><button data-deny="${a.id}">Deny</button><button data-jump="${esc(a.session_key)}">Jump</button></div>
    <div class="w-used">Terminal asks instead in ${Math.max(0, Math.round((a.expires_ms - Date.now()) / 1000))}s</div>`;
}
// Waiting requests pop out on their own. Items keep their DOM while their id stays, so typing is not lost.
function renderReqs() { reqs.className = ""; }
function placeReqs() {
  const pr = pill.getBoundingClientRect();
  const gap = TAIL_GAP + 8;
  reqs.style.left = reqs.style.right = reqs.style.top = reqs.style.bottom = "";
  reqs.style.transform = "none";
  if (edge === "right") { reqs.style.right = (innerWidth - pr.left + gap) + "px"; reqs.style.top = Math.max(8, Math.min(innerHeight - reqs.offsetHeight - 8, pr.top + pr.height / 2 - reqs.offsetHeight / 2)) + "px"; }
  else if (edge === "left") { reqs.style.left = (pr.right + gap) + "px"; reqs.style.top = Math.max(8, Math.min(innerHeight - reqs.offsetHeight - 8, pr.top + pr.height / 2 - reqs.offsetHeight / 2)) + "px"; }
  else {
    const left = Math.max(8, Math.min(innerWidth - reqs.offsetWidth - 8, pr.left + pr.width / 2 - reqs.offsetWidth / 2));
    reqs.style.left = left + "px";
    if (edge === "top") reqs.style.top = (pr.bottom + gap) + "px";
    else reqs.style.bottom = (innerHeight - pr.top + gap) + "px";
  }
}

function edgeIsVertical() { return edge === "left" || edge === "right"; }
function placeCard() {
  const cell = pill.querySelector(`.cell[data-p="${hoverId}"]`) || pill;
  const cr = cell.getBoundingClientRect();
  const rr = (cell.querySelector(".ringwrap") || cell).getBoundingClientRect();
  card.style.transform = "none";
  if (edgeIsVertical()) {
    const cy = cr.top + cr.height / 2, H = innerHeight, ch = card.offsetHeight || 0;
    let top = Math.round(cy - ch / 2); top = Math.max(8, Math.min(top, H - ch - 8));
    card.style.top = top + "px"; card.style.left = "";
    const ry = rr.top + rr.height / 2, th = tail.offsetHeight || 33;
    const ty = Math.max(top + 16 + th / 2, Math.min(top + ch - 16 - th / 2, ry));
    tail.style.top = Math.round(ty - th / 2) + "px"; tail.style.left = "";
  } else {
    const pr = pill.getBoundingClientRect();
    const cx = cr.left + cr.width / 2, W = innerWidth, cw = card.offsetWidth || 0, ch = card.offsetHeight || 0;
    let left = Math.round(cx - cw / 2); left = Math.max(8, Math.min(left, W - cw - 8));
    card.style.left = left + "px";
    const rx = rr.left + rr.width / 2, tw = tail.offsetWidth || 33;
    const tx = Math.max(left + 16 + tw / 2, Math.min(left + cw - 16 - tw / 2, rx));
    tail.style.left = Math.round(tx - tw / 2) + "px";
    if (edge === "top") { card.style.top = Math.round(pr.bottom + TAIL_GAP + TAIL_LENGTH) + "px"; tail.style.top = Math.round(pr.bottom + TAIL_GAP) + "px"; }
    else { card.style.top = Math.round(pr.top - TAIL_GAP - TAIL_LENGTH - ch) + "px"; tail.style.top = Math.round(pr.top - TAIL_GAP - TAIL_LENGTH) + "px"; }
  }
}

// Interactive area in physical px: the tab when folded, else pill (with fillets) and open card.
function reportHot() {
  const k = window.devicePixelRatio || 1;
  let r;
  if (folded) r = rest.getBoundingClientRect();
  else {
    const p = pill.getBoundingClientRect();
    const f = 39;
    r = edgeIsVertical()
      ? { left: p.left, top: p.top - f, right: p.right, bottom: p.bottom + f }
      : { left: p.left - f, top: p.top, right: p.right + f, bottom: p.bottom };
    if (reqs.classList.contains("show")) {
      const q = reqs.getBoundingClientRect();
      r = { left: Math.min(r.left, q.left), top: Math.min(r.top, q.top), right: Math.max(r.right, q.right), bottom: Math.max(r.bottom, q.bottom) };
    }
    if (card.classList.contains("show")) {
      const c = card.getBoundingClientRect();
      r = { left: Math.min(r.left, c.left), top: Math.min(r.top, c.top), right: Math.max(r.right, c.right), bottom: Math.max(r.bottom, c.bottom) };
    }
  }
  invoke("report_hot", { rect: [r.left * k, r.top * k, (r.right - r.left) * k, (r.bottom - r.top) * k] }).catch(() => {});
}

function showCard() { if (reqs.classList.contains("show") || dragging) return; clearTimeout(hideTimer); card.classList.add("show"); renderCard(); }
function hideCard() { card.classList.remove("show"); reportHot(); }
function cellAt(x, y) {
  for (const el of pill.querySelectorAll(".cell")) {
    const r = el.getBoundingClientRect();
    if (x >= r.left - 6 && y >= r.top - 6 && x < r.right + 6 && y < r.bottom + 6) return el.dataset.p;
  }
  return null;
}
document.addEventListener("mousemove", (e) => {
  if (folded) return;
  const id = cellAt(e.clientX, e.clientY);
  if (id) {
    clearTimeout(hideTimer);
    if (id !== hoverId) { hoverId = id; if (card.classList.contains("show")) renderCard(); }
    if (!card.classList.contains("show")) showCard();
  }
});

let dragging = false, press = null;
pill.addEventListener("mousedown", (e) => { if (e.button === 0) press = { x: e.screenX, y: e.screenY }; });
document.addEventListener("mousemove", (e) => {
  if (!press) return;
  if (!dragging && Math.hypot(e.screenX - press.x, e.screenY - press.y) < 5) return;
  if (!dragging) { dragging = true; document.body.classList.add("dragging"); hideCard(); }
  const k = window.devicePixelRatio || 1;
  invoke("notch_drag", { x: e.screenX * k, y: e.screenY * k });
});
document.addEventListener("mouseup", () => {
  if (dragging) invoke("notch_drag_end");
  dragging = false; press = null;
  document.body.classList.remove("dragging");
  setTimeout(() => { if (reqs.classList.contains("show")) placeReqs(); reportHot(); }, 50);
});

function setFolded(f) {
  if (f === folded) return;
  folded = f;
  if (f) { clearTimeout(hideTimer); hideCard(); }
  document.body.classList.toggle("folded", f);
  setTimeout(reportHot, 400);
  reportHot();
}
function scheduleFold() {
  clearTimeout(foldTimer);
  if (reqs.classList.contains("show")) return; // stays open while something waits
  if (!snap || !snap.settings.notch_on_hover) { setFolded(false); return; }
  if (pointerIn) return;
  foldTimer = setTimeout(() => { if (!pointerIn) setFolded(true); }, FOLD_GRACE);
}

listen("notch_pointer", (e) => {
  pointerIn = e.payload === true;
  if (pointerIn) { clearTimeout(foldTimer); setFolded(false); }
  else { hideTimer = setTimeout(hideCard, 250); scheduleFold(); }
});
listen("notch_edge", (e) => { edge = e.payload; document.body.dataset.edge = edge; render(); });
window.addEventListener("resize", () => { if (reqs.classList.contains("show")) placeReqs(); reportHot(); });

function render() {
  if (!snap) return;
  renderRing();
  renderReqs();
  renderCard();
  reportHot();
}
listen("state", (e) => {
  const first = !snap;
  snap = e.payload;
  render();
  if (first) scheduleFold();
});
invoke("get_state").then((s) => { snap = s; edge = s.settings.edge || "right"; document.body.dataset.edge = edge; render(); scheduleFold(); });
setInterval(render, 30000);
