// Alert pill: queue, timing, icons and sounds from Terryxxx/Navi island.js (Apache-2.0, e8e0c40).
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const icons = {
  check: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg>',
  warning: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>',
  info: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>',
};
// kind from the backend: done | permission | question | idle | info
const KIND = {
  done: { type: "success", icon: "check", sound: "success" },
  permission: { type: "warning", icon: "warning", sound: "error" },
  question: { type: "info", icon: "info", sound: "success" },
  idle: { type: "info", icon: "info", sound: "success" },
  info: { type: "info", icon: "info", sound: "success" },
};

const audioCtx = new (window.AudioContext || window.webkitAudioContext)();
const buffers = {};
const lowPass = audioCtx.createBiquadFilter();
lowPass.type = "lowpass";
lowPass.frequency.value = 2000;
const gain = audioCtx.createGain();
gain.gain.value = 0.15;
lowPass.connect(gain);
gain.connect(audioCtx.destination);
async function load(name, url) {
  try {
    const buf = await (await fetch(url)).arrayBuffer();
    buffers[name] = await audioCtx.decodeAudioData(buf);
  } catch (_) {}
}
load("success", "sounds/success.wav");
load("error", "sounds/error.mp3");
function play(name) {
  if (!buffers[name]) return;
  if (audioCtx.state === "suspended") audioCtx.resume();
  const s = audioCtx.createBufferSource();
  s.buffer = buffers[name];
  s.playbackRate.value = 0.98 + Math.random() * 0.04;
  s.connect(lowPass);
  s.start(0);
}

const island = document.getElementById("island");
const queue = [];
let showing = false;

function showNext() {
  if (!queue.length) { showing = false; invoke("hide_overlay"); return; }
  showing = true;
  const a = queue.shift();
  const k = KIND[a.kind] || KIND.info;
  document.getElementById("icon").innerHTML = icons[k.icon];
  document.getElementById("title").textContent = (a.provider === "codex" ? "Codex" : "Claude") + " · " + a.title;
  document.getElementById("message").textContent = (a.body || "").slice(0, 200);
  document.getElementById("project").textContent = "";
  island.className = "island expanded type-" + k.type;
  if (a.sound) play(k.sound);
  const duration = a.kind === "permission" || a.kind === "question" ? 9000 : 4500;
  setTimeout(() => {
    island.style.width = island.getBoundingClientRect().width + "px";
    island.offsetHeight;
    island.classList.remove("expanded");
    island.classList.add("collapsed");
    island.style.width = "";
    setTimeout(showNext, 600);
  }, duration);
}

listen("pill", (e) => {
  queue.push(e.payload);
  if (!showing) showNext();
});

island.addEventListener("click", () => invoke("show_panel"));
