// Injected into the hidden DeskTime window. Reads the server-rendered dashboard cards and hands the
// numbers to Nubi through the single allowed command. It never reads or sends cookies or page HTML.
(() => {
  if (window.__nubiDesktime || !/(^|\.)desktime\.com$/.test(location.hostname)) return;
  window.__nubiDesktime = true;
  const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
  const pad = (n) => String(n).padStart(2, "0");
  const ymd = (d) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

  // URL forms: /app/my (today), /app/my/YYYY-MM-DD, /app/my/YYYY-MM-DD/week|month
  function target() {
    const m = location.pathname.match(/^\/app\/my(?:\/(\d{4})-(\d{2})-(\d{2}))?(?:\/(day|week|month))?\/?$/);
    if (!m) return null;
    const date = m[1] ? new Date(+m[1], +m[2] - 1, +m[3]) : new Date(new Date().setHours(0, 0, 0, 0));
    const view = m[4] || "day";
    let start = date, end = date;
    if (view === "week") {
      start = new Date(date); start.setDate(date.getDate() - ((date.getDay() + 6) % 7));
      end = new Date(start); end.setDate(start.getDate() + 6);
    } else if (view === "month") {
      start = new Date(date.getFullYear(), date.getMonth(), 1);
      end = new Date(date.getFullYear(), date.getMonth() + 1, 0);
    }
    return { view, start, end };
  }

  // The whole text must be a duration ("6h 12m", "45m", "8h", optional seconds); anything else,
  // such as "8h 30min", is not a known format and reads as missing.
  const minutes = (text) => {
    const m = text.replace(/\s+/g, "").match(/^(?:(\d+)h)?(?:(\d+)m)?(?:\d+s)?$/);
    return m && (m[1] || m[2]) ? (+m[1] || 0) * 60 + (+m[2] || 0) : null;
  };
  // Parent element of the first text node accepted by `test` (linear in page size).
  function textParent(test) {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (test(n.textContent) && n.parentElement !== document.body) return n.parentElement;
    }
    return null;
  }
  // Main duration of a card such as "Productive time" or "Time at work"; null while it is not rendered.
  function card(title) {
    for (const c of document.querySelectorAll(".dt-card")) {
      const t = c.querySelector(".dt-typography-h6");
      if (!t || t.textContent.trim() !== title) continue;
      const body = c.querySelector(".card-body");
      const head = body?.querySelector("h1, h2, h3");
      // First duration in the card's value. A unit must not run into more letters, so "8h 30min"
      // is not read as 8h 30m.
      const v = (head || body)?.textContent.replace(/\s+/g, "").match(/(?:(\d+)h)?(\d+)m(?![a-z])|(\d+)h(?![a-z\d])/i);
      if (v) return v[3] ? +v[3] * 60 : (+v[1] || 0) * 60 + +v[2];
    }
    // Fallback: the first element under the title's ancestors whose whole text is a duration.
    let box = textParent((t) => t.trim() === title);
    for (let i = 0; i < 6 && box; i++) {
      box = box.parentElement;
      if (box && /\d+\s*[hm]/.test(box.textContent.replace(title, ""))) {
        for (const e of box.querySelectorAll("*")) { const n = minutes(e.textContent.trim()); if (n !== null) return n; }
        return null;
      }
    }
    return null;
  }
  function holidays(start, end) {
    const out = new Set();
    const banner = document.body && textParent((t) => /Public Holiday/i.test(t));
    if (!banner) return [];
    const re = /(\d{1,2})\.([A-Za-z]{3})/g;
    const text = (banner.parentElement || banner).textContent;
    for (let m; (m = re.exec(text)); ) {
      const mi = MONTHS.indexOf(m[2].toLowerCase());
      if (mi < 0) continue;
      for (const y of new Set([start.getFullYear(), end.getFullYear()])) {
        const d = new Date(y, mi, +m[1]);
        if (d >= start && d <= end) out.add(ymd(d));
      }
    }
    return [...out].sort().slice(0, 366);
  }

  let sent = false;
  const page = location.href;
  function send(report) {
    // The numbers must belong to the page the range came from (no in-page navigation since).
    if (sent || location.href !== page) return;
    sent = true;
    try { window.__TAURI_INTERNALS__.invoke("desktime_report", { report }).catch(() => {}); } catch (_) {}
  }
  const t = target();
  const now = new Date();
  const base = { view: t ? t.view : "day", productiveMin: null, atWorkMin: null, rangeStart: ymd(t ? t.start : now),
    rangeEnd: ymd(t ? t.end : now), holidays: [], loggedOut: false };
  // Anything outside /app/ (the login page after a redirect) means the session has expired.
  if (!t) {
    if (!location.pathname.startsWith("/app/")) send({ ...base, loggedOut: true });
    return;
  }
  const started = Date.now();
  const timer = setInterval(() => {
    if (document.readyState === "loading") return;
    const productive = card("Productive time");
    const atWork = card("Time at work");
    const waited = Date.now() - started;
    if ((productive !== null && (atWork !== null || waited > 3000)) || waited > 20000) {
      clearInterval(timer);
      send({ ...base, productiveMin: productive, atWorkMin: atWork, holidays: holidays(t.start, t.end) });
    }
  }, 250);
})();
