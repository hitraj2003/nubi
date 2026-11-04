// Shared theme: reads Settings (theme, accent, reduce_motion) and sets data-* on <html>.
// Loaded first on every surface. "system" follows the Windows app theme.
(() => {
  const root = document.documentElement;
  const mq = matchMedia("(prefers-color-scheme: dark)");
  let pref = { theme: "system", accent: "yellow", reduce_motion: false };
  function apply(p) {
    if (p) pref = { ...pref, ...p };
    const dark = pref.theme === "dark" || (pref.theme !== "light" && mq.matches);
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
  const take = (s) => s && s.settings && !window.nubiTheme.hold && apply({ theme: s.settings.theme, accent: s.settings.accent, reduce_motion: s.settings.reduce_motion });
  T.core.invoke("get_state").then(take).catch(() => {});
  T.event.listen("theme", (e) => apply(e.payload));
  T.event.listen("state", (e) => take(e.payload));
})();
