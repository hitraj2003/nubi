const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const root = path.resolve(__dirname, "..");
const context = { window: {} };
vm.runInNewContext(fs.readFileSync(path.join(root, "ui/usage-status.js"), "utf8"), context);
const usageStatus = context.window.UsageStatus;
const now = 600_000;

test("cached usage displays the last successful age through failures", () => {
  for (const status of ["ok", "backoff", "error", "needs_auth", "unavailable"]) {
    assert.equal(usageStatus.text({ status, windows: [{ used_percent: 25 }], fetched_ms: 300_000 }, now), "Updated 5 min ago");
  }
});

test("without cached usage the user still sees actionable status", () => {
  assert.equal(usageStatus.text({ status: "backoff", windows: [] }, now), "Waiting to try again");
  assert.equal(usageStatus.text({ status: "needs_auth", windows: [] }, now), "Sign in again");
  assert.equal(usageStatus.text({ status: "error", windows: [] }, now), "Could not update usage");
});

test("age handles minute boundaries, clock skew and missing timestamps", () => {
  assert.equal(usageStatus.age({ windows: [{}], fetched_ms: 599_999 }, now), "Updated just now");
  assert.equal(usageStatus.age({ windows: [{}], fetched_ms: 540_000 }, now), "Updated 1 min ago");
  assert.equal(usageStatus.age({ windows: [{}], fetched_ms: 700_000 }, now), "Updated just now");
  assert.equal(usageStatus.age({ windows: [{}] }, now), "Not updated yet");
});

test("usage copy loads before each surface uses it", () => {
  for (const file of ["panel", "island", "ringtip", "taskbar"]) {
    const html = fs.readFileSync(path.join(root, `ui/${file}.html`), "utf8");
    const helper = html.indexOf('src="usage-status.js"');
    assert(helper >= 0 && helper < html.indexOf(`src="${file}.js"`));
  }
});
