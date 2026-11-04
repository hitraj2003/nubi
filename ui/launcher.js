// Launcher: pick Claude or Codex from the keyboard for the focused Explorer folder.
// Text via textContent only.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const opts = [...document.querySelectorAll(".opt")];
let sel = localStorage.getItem("launcher.cli") === "codex" ? 1 : 0;
let ready = false;

function paint() {
  opts.forEach((o, i) => o.classList.toggle("on", ready && i === sel));
  if (ready) Motion.tween(opts[sel], { scale: 0.97 }, { scale: 1 }, Motion.PRESET.snappy);
}

function run(i) {
  if (!ready) return;
  const cli = opts[i].dataset.cli;
  localStorage.setItem("launcher.cli", cli);
  ready = false;
  close().then(() => invoke("launcher_run", { cli })).catch((e) => show({ error: String(e) }));
}

function show(p) {
  ready = !p.error;
  document.getElementById("dir").textContent = p.dir || "";
  document.getElementById("err").textContent = p.error || "";
  document.getElementById("opts").style.display = ready ? "" : "none";
  document.getElementById("foot").textContent = ready ? "Arrows to choose, Enter to open, Esc to close" : "Esc to close";
  paint();
  closing = false;
  Motion.enter(card);
}

// Same spring blur-in and blur-out as the ring tooltip and notch cards.
const card = document.getElementById("card");
let closing = false;
function close() {
  if (closing) return Promise.resolve();
  closing = true;
  return Motion.leave(card);
}
function cancel() { close().then(() => invoke("launcher_cancel")); }
Motion.pressable(document.getElementById("opts"));

listen("launcher", (e) => show(e.payload));
opts.forEach((o, i) => o.addEventListener("click", () => run(i)));
window.addEventListener("keydown", (e) => {
  const k = e.key.toLowerCase();
  if (k === "escape") return cancel();
  if (!ready) return;
  if (k === "c") run(0);
  else if (k === "x") run(1);
  else if (["arrowleft", "arrowright", "arrowup", "arrowdown", "tab"].includes(k)) { e.preventDefault(); sel = 1 - sel; paint(); }
  else if (k === "enter") { e.preventDefault(); run(sel); }
});
window.addEventListener("blur", () => { if (!closing) cancel(); });
