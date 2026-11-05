// Popup: shows whether DeskTime is signed in and whether Nubi has access, and lets the owner
// allow or remove access. The background worker does the work.
const $ = (id) => document.getElementById(id);
const ask = (cmd) => chrome.runtime.sendMessage({ cmd });

function line(id, text, tone) {
  $(id).textContent = text;
  $(id).className = tone || "";
}

function render(s) {
  line("desktime", s.signedIn === true ? "Signed in" : s.signedIn === false ? "Not signed in" : "Couldn't check",
    s.signedIn === true ? "good" : s.signedIn === false ? "bad" : "");
  const failed = s.last && !s.last.ok && s.last.error;
  line("nubi", s.revokePending ? "Removing access, will retry" : failed ? s.last.error : s.allowed ? "Has access" : "No access",
    s.revokePending || failed ? "bad" : s.allowed ? "good" : "");
  $("allow").hidden = s.allowed && !failed;
  $("allow").textContent = s.allowed ? "Send again" : "Allow access";
  $("remove").hidden = !s.allowed;
  $("note").replaceChildren();
  if (s.signedIn === false) {
    const a = Object.assign(document.createElement("a"), { href: "https://desktime.com/login", target: "_blank", textContent: "Log in to DeskTime" });
    $("note").append(a, " first, then allow access.");
  } else {
    $("note").textContent = "Nubi uses the DeskTime session in this browser. It never sees your password.";
  }
}

async function refresh() {
  render(await ask("status"));
}

async function act(cmd) {
  for (const b of document.querySelectorAll("button")) b.disabled = true;
  try {
    await ask(cmd);
  } finally {
    for (const b of document.querySelectorAll("button")) b.disabled = false;
    await refresh();
  }
}

$("allow").addEventListener("click", () => act("allow"));
$("remove").addEventListener("click", () => act("remove"));
refresh();
