// Writes latest.json for the updater after `cargo tauri build`.
// Usage: node scripts/latest-json.mjs "release notes"
// Upload latest.json and the installer to the GitHub release tagged v<version>.
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/(\w:)/, "$1")), "..");
const conf = JSON.parse(fs.readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const v = conf.version;
const name = `${conf.productName}_${v}_x64-setup.exe`;
const dir = path.join(root, "src-tauri", "target", "release", "bundle", "nsis");
const sig = fs.readFileSync(path.join(dir, name + ".sig"), "utf8").trim();

const latest = {
  version: v,
  notes: process.argv[2] || "",
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: sig,
      url: `https://github.com/hitraj2003/nubi/releases/download/v${v}/${name}`,
    },
  },
};
const out = path.join(dir, "latest.json");
fs.writeFileSync(out, JSON.stringify(latest, null, 2) + "\n");
console.log("wrote " + out);
