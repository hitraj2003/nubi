const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = id => document.getElementById(id);
let notice = null, hovered = false, focused = false, held = false, remaining = 0;

function countdown() {
  if (!notice || !notice.expires_ms) { $("countdown").textContent = ""; return; }
  const seconds = Math.ceil((notice.held ? remaining : Math.max(0, notice.expires_ms - Date.now())) / 1000);
  $("countdown").textContent = notice.held ? "Paused while you read" : notice.automatic ? `Updates in ${seconds}s` : `Dismisses in ${seconds}s`;
}

function render(value) {
  const changed = value?.id !== notice?.id;
  notice = value;
  $("notice").hidden = !notice;
  if (!notice) { held = false; hovered = false; focused = false; return; }
  remaining = Math.max(0, notice.expires_ms - Date.now());
  $("title").textContent = notice.phase === "error" ? "Nubi update failed" : "Nubi update available";
  $("version").textContent = "Version " + notice.version;
  // Release notes are untrusted publisher text. Never interpret them as HTML.
  $("notes").textContent = notice.notes.trim() || "Release notes are available on the release page.";
  $("message").textContent = notice.message;
  $("install").disabled = ["downloading", "installing"].includes(notice.phase);
  $("install").textContent = notice.phase === "downloading" ? "Downloading…" : notice.phase === "installing" ? "Installing…" : notice.phase === "error" ? "Retry" : "Update";
  $("cancel").disabled = notice.phase === "installing";
  if (changed) {
    $("notes").scrollTop = 0; held = false; syncHold();
    invoke("update_notice_ready", { id: notice.id }).catch(() => {});
  }
  countdown();
}

function syncHold() {
  if (!notice) return;
  const on = hovered || focused;
  if (on === held) return;
  held = on;
  invoke("hold_update", { id: notice.id, on }).catch(() => {});
}

function dismiss() {
  if (notice && notice.phase !== "installing") invoke("dismiss_update", { id: notice.id }).catch(() => {});
}

$("notice").addEventListener("mouseenter", () => { hovered = true; syncHold(); });
$("notice").addEventListener("mouseleave", () => { hovered = false; syncHold(); });
$("notice").addEventListener("focusin", () => { focused = true; syncHold(); });
$("notice").addEventListener("focusout", event => { focused = $("notice").contains(event.relatedTarget); syncHold(); });
window.addEventListener("blur", () => { focused = false; syncHold(); });
document.addEventListener("keydown", event => { if (event.key === "Escape") dismiss(); });
$("cancel").addEventListener("click", dismiss);
$("install").addEventListener("click", async () => {
  if (!notice || $("install").disabled) return;
  const id = notice.id;
  $("install").disabled = true;
  try { await invoke("install_update", { id }); }
  catch (error) { if (notice?.id === id) { $("message").textContent = String(error); $("install").disabled = false; } }
});
$("release-notes").addEventListener("click", () => {
  if (notice) invoke("open_release_link", { url: "https://github.com/hitraj2003/nubi/releases/tag/v" + notice.version }).catch(() => {});
});
// Subscribe before the initial read so a late-loading notification cannot miss its state.
listen("update_notice", event => render(event.payload)).then(() => invoke("get_update_notice")).then(render).catch(() => {});
setInterval(countdown, 250);
