// Shared theme: reads Settings and sets data-* on <html>. Loaded first on every surface.
// The panel uses theme/accent; every other surface (notch, toast, ring, ring tip, taskbar strip)
// uses notch_theme/notch_accent. "system" follows the Windows app theme.
(() => {
  const root = document.documentElement;
  const mq = matchMedia("(prefers-color-scheme: dark)");
  const isPanel = /panel\.html$/.test(location.pathname);
  let taskbarTheme = null;
  const isTaskbar = /taskbar\.html$/.test(location.pathname);
  let pref = { theme: isPanel ? "system" : "dark", accent: "yellow", reduce_motion: false };
  function apply(p) {
    if (p) pref = { ...pref, ...p };
    const systemDark = isTaskbar && taskbarTheme ? taskbarTheme === "dark" : mq.matches;
    // The taskbar strip sits on the Windows taskbar, so it always matches the taskbar's theme
    // (a light strip on a dark taskbar was unreadable); its text cannot follow the notch theme.
    const dark = isTaskbar && taskbarTheme ? systemDark : pref.theme === "dark" || (pref.theme !== "light" && systemDark);
    root.dataset.theme = dark ? "dark" : "light";
    root.dataset.accent = pref.accent || "yellow";
    root.dataset.motion = pref.reduce_motion ? "reduce" : "full";
  }
  // hold: the panel sets it while Settings has unsaved changes, so its live preview is not undone.
  window.nubiTheme = { apply, hold: false };
  apply();
  mq.addEventListener("change", () => apply());
  const T = window.__TAURI__;
  if (!T) return;
  const take = (s) => s && s.settings && !window.nubiTheme.hold && apply(pick(s.settings));
  // pick: the theme/accent pair for this surface.
  function pick(c) {
    return isPanel
      ? { theme: c.theme, accent: c.accent, reduce_motion: c.reduce_motion }
      : { theme: c.notch_theme, accent: c.notch_accent, reduce_motion: c.reduce_motion };
  }
  if (isTaskbar) T.event.listen("taskbar_theme", (e) => { taskbarTheme = e.payload; apply(); });
  T.core.invoke("get_state").then(take).catch(() => {});
  T.event.listen("theme", (e) => apply(pick(e.payload)));
  T.event.listen("state", (e) => take(e.payload));
})();

// Custom tooltip for every surface with room for it, instead of the browser's native title box.
// Any [title] becomes data-tip on first hover or focus (the native tooltip then never shows); the
// text stays available to screen readers through aria-label when the element has no other name.
// The ring and taskbar strip windows are too small to hold a tooltip, so they keep native titles.
(() => {
  if (/(ring|taskbar)\.html$/.test(location.pathname)) return;
  let tip = null, timer = 0, cur = null;
  const DELAY = 450, GAP = 6, EDGE = 8;
  function adopt(el) {
    const t = el.getAttribute("title");
    if (t) {
      el.dataset.tip = t;
      el.removeAttribute("title");
      if (!el.hasAttribute("aria-label") && !el.textContent.trim()) el.setAttribute("aria-label", t);
    }
    return el.dataset.tip;
  }
  function hide() {
    clearTimeout(timer); cur = null;
    if (tip) tip.classList.remove("on");
  }
  function show(el) {
    if (!el.isConnected || !el.dataset.tip) return;
    if (!tip) { tip = document.createElement("div"); tip.className = "nubi-tip"; tip.setAttribute("role", "tooltip"); document.body.append(tip); }
    tip.textContent = el.dataset.tip;
    tip.classList.remove("on", "up");
    const r = el.getBoundingClientRect(), w = tip.offsetWidth, h = tip.offsetHeight;
    let top = r.bottom + GAP, up = false;
    if (top + h > innerHeight - EDGE && r.top - GAP - h >= EDGE) { top = r.top - GAP - h; up = true; }
    const left = Math.max(EDGE, Math.min(innerWidth - EDGE - w, r.left + r.width / 2 - w / 2));
    tip.style.left = left + "px"; tip.style.top = top + "px";
    tip.classList.toggle("up", up);
    requestAnimationFrame(() => tip.classList.add("on"));
  }
  function arm(el, now) {
    if (el === cur) return;
    hide();
    cur = el;
    timer = setTimeout(() => show(el), now ? 0 : DELAY);
  }
  const target = (e) => { const el = e.target.closest && e.target.closest("[title],[data-tip]"); return el && adopt(el) ? el : null; };
  addEventListener("pointerover", (e) => { const el = target(e); if (el) arm(el); else if (cur && !cur.contains(e.target)) hide(); });
  addEventListener("pointerout", (e) => { if (cur && !cur.contains(e.relatedTarget)) hide(); });
  addEventListener("focusin", (e) => { const el = target(e); if (el && el.matches(":focus-visible")) arm(el, false); });
  addEventListener("focusout", hide);
  addEventListener("pointerdown", hide, true);
  addEventListener("keydown", (e) => { if (e.key === "Escape") hide(); });
  addEventListener("scroll", hide, true);
  addEventListener("blur", hide);
})();
