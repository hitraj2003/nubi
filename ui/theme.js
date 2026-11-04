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
    const dark = pref.theme === "dark" || (pref.theme !== "light" && systemDark);
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
