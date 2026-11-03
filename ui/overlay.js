// Basic pill. Final notch/ring/taskbar visuals come in the UI pass.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const pill = document.getElementById("pill");
let timer = null;

listen("pill", (e) => {
  const a = e.payload;
  pill.className = a.kind;
  document.getElementById("title").textContent =
    (a.provider === "codex" ? "Codex" : "Claude") + " · " + a.title;
  document.getElementById("body").textContent = a.body;
  clearTimeout(timer);
  // Permission alerts stay longer; the card in the panel is the real control.
  timer = setTimeout(() => invoke("hide_overlay"), a.kind === "permission" ? 12000 : 5000);
});

pill.addEventListener("click", () => {
  invoke("show_panel");
  invoke("hide_overlay");
});
