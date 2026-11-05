// Nubi DeskTime connect: hands the DeskTime session cookies of this browser to Nubi's native
// messaging host. Nothing else leaves the browser, and the extension never sees a password.
const HOST = "dev.nubi.desktime";
const isDesktime = (domain) => /(^|\.)desktime\.com$/.test(domain.replace(/^\./, ""));

async function sessionCookies() {
  const all = await chrome.cookies.getAll({ domain: "desktime.com" });
  return all.filter((c) => isDesktime(c.domain)).map((c) => ({
    // The domain is passed as the browser stores it: ".desktime.com" for a domain cookie,
    // "desktime.com" for a host-only one, so Nubi keeps the same scope.
    name: c.name, value: c.value, domain: c.domain, path: c.path, secure: c.secure, httpOnly: c.httpOnly,
    expires: c.session ? null : c.expirationDate ?? null, sameSite: c.sameSite,
  }));
}

async function send(message) {
  try {
    return await chrome.runtime.sendNativeMessage(HOST, message);
  } catch (e) {
    const text = String(e && e.message || e);
    // Chrome's wording when the host is not registered (Nubi never started on this PC).
    const error = /not found/i.test(text) ? "Nubi is not installed, or has not started yet" : text;
    return { ok: false, error };
  }
}

async function remember(reply) {
  const last = { ok: !!(reply && reply.ok), error: (reply && reply.error) || "", at: Date.now() };
  await chrome.storage.local.set(reply && reply.revoked ? { last, allowed: false } : { last });
  return reply;
}

// `sync` marks a background re-send; Nubi refuses it after the owner logged out in Nubi.
// A failed send is retried by the "retry" alarm (Nubi closed, or still starting).
async function push(sync) {
  const reply = await remember(await send({ type: "desktime_session", cookies: await sessionCookies(), sync }));
  if (!(reply && reply.ok) && !(reply && reply.revoked)) retrySoon();
  return reply;
}
const retrySoon = () => chrome.alarms.create("retry", { delayInMinutes: 1 });
// Remove access stops all sending at once. Nubi's copy is cleared when Nubi confirms; until then
// `revokePending` stays set and the retry alarm tries again.
async function revokeNow() {
  const reply = await remember(await send({ type: "desktime_revoke" }));
  if (reply && reply.ok) await chrome.storage.local.remove("revokePending");
  else retrySoon();
  return reply;
}

async function signedIn() {
  try {
    const res = await fetch("https://desktime.com/app/my", { credentials: "include", cache: "no-store" });
    return new URL(res.url).pathname.startsWith("/app/");
  } catch (_) {
    return null;
  }
}

const handlers = {
  async status() {
    const { allowed = false, last = null, revokePending = false } = await chrome.storage.local.get(["allowed", "last", "revokePending"]);
    return { allowed, last, revokePending, signedIn: await signedIn() };
  },
  async allow() {
    await chrome.storage.local.remove("revokePending");
    const reply = await push(false);
    if (reply && reply.ok) await chrome.storage.local.set({ allowed: true });
    return reply;
  },
  async remove() {
    await chrome.storage.local.set({ allowed: false, revokePending: true });
    return revokeNow();
  },
};

chrome.runtime.onMessage.addListener((msg, sender, respond) => {
  const run = sender.id === chrome.runtime.id && handlers[msg && msg.cmd];
  if (!run) return false;
  run().then(respond, (e) => respond({ ok: false, error: String(e) }));
  return true;
});

// While access is allowed, any DeskTime cookie change (new login, refreshed session, logout) is
// re-sent after a short pause, so Nubi follows the browser without a second login.
let timer = null;
async function syncIfAllowed() {
  const { allowed } = await chrome.storage.local.get("allowed");
  if (allowed) await push(true);
}
chrome.cookies.onChanged.addListener(({ cookie }) => {
  if (!isDesktime(cookie.domain)) return;
  clearTimeout(timer);
  timer = setTimeout(syncIfAllowed, 2000);
});
// Pending revoke first; otherwise re-send while allowed. Runs at browser start, on the retry
// alarm, and every 30 minutes so a restarted Nubi gets the session back without a click.
async function catchUp() {
  const { revokePending } = await chrome.storage.local.get("revokePending");
  if (revokePending) await revokeNow();
  else await syncIfAllowed();
}
chrome.runtime.onStartup.addListener(catchUp);
chrome.alarms.onAlarm.addListener(catchUp);
chrome.alarms.create("periodic", { periodInMinutes: 30 });
