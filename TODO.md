# TODO

Every request from the owner goes here (with date) until it is done. Never push, tag or release
without asking the owner first.

## Pick up here (next session)
- [ ] Push: `main` is 7 commits ahead of `origin` (hitraj2003/nubi). Ask the owner first.
- [ ] Release v0.1.1: owner said wait (2026-11-04), more bug fixes first. Ask before tagging.
- [ ] Owner's bug list: owner has more bugs to report (2026-11-04). Get the list and log each here.
- [ ] Settings panel redesign (Part B, `ui/panel.*`): dark, same material as the toast (pure black, white ink), owner's moodboard: sidebar with grouped rows, segmented controls, toggles, card rows; motion (sliding nav highlight, view crossfade, springy switches).
- [ ] Remove the old edge notch files (`ui/notch.*`, notch window in `tauri.conf.json`) once the owner confirms the new notch replaces it. Keep `notch.rs` helpers used by `island.rs`.
- [ ] Inner session ring is amber-ish at 60–84 %, close to the outer week ring colour range: ask the owner whether to keep.
- [ ] Back up `~/.tauri/nubi.key` (owner). Losing it breaks updates for installed copies.

## Built, not yet checked on screen (owner said test later)
- [ ] Toast springs: alert pill, request cards, deck (`ui/overlay.*`).
- [ ] Notch fold mode: hover shows the pill, click opens it.
- [ ] Session names instead of folders (toast, notch, panel, alert text). `/rename` key `customTitle` not confirmed.
- [ ] Floating ring in notch style: rings on springs, split halves with own percentages and D-shaped activity loops; tooltip blur-in and bar sweep; taskbar strip colours and springs.
- [ ] Sounds: one synthesized tone per alert kind (`ui/sounds.js`).
- [ ] Claude usage from the status line (`--statusline`): numbers update live, owner's `statusline.js` still shows, turning Claude hooks off restores the old status line.
- [ ] Single instance: a second launch opens the panel of the running copy.
- [ ] Settings: auto-update switch; draft + Save/Discard bar; save-or-discard pop-up when leaving Settings.
- [ ] Report an issue button opens the prefilled bug form (needs a push first: forms live on GitHub).
- [ ] Bug (owner, 2026-11-04): Settings > "Fold until hovered" switch is not clickable, and it saved automatically. Seen before the draft/Save change (d33cb55); check whether it still happens.
- [ ] Auto-update end to end: install 0.1.0, release a newer version, watch it update.

## Needs manual check (real sessions)
- [ ] Codex approvals after `/hooks` trust.
- [ ] What Claude Code and Codex do when the hook times out (expect native prompt).
- [ ] Jump to terminal in Windows Terminal and VS Code (session rows in the notch are clickable).
- [ ] Toast while a full-screen app is open.
- [ ] Release build: hook mode stdout under `windows_subsystem = "windows"`.
- [ ] Notch on the bottom and left edges, and on a monitor with another DPI (top and right tested).
- [ ] Context size column in the notch Sessions tab with real sessions.

## Later
- [ ] Agent-drafted bug reports (owner, 2026-11-04). Report dialog in Settings: user writes a sentence; Nubi collects version, Windows, display mode/edge, providers, hook and usage status, Claude/Codex versions (no tokens); "Draft with Claude/Codex" runs the local CLI (`claude -p --bare --tools "" --no-session-persistence`, or `codex exec --ephemeral --skip-git-repo-check -o <file>`, prompt on stdin, temp cwd) and gets JSON for every bug-form field; user edits, then the prefilled `bug_report.yml` form opens and they only press Submit. Keep the URL under ~7.5k chars.
- [ ] Usage API fallback polish: show last good numbers with "updated N min ago" instead of "rate limited"; send a `nubi/<version>` User-Agent.
- [ ] Taskbar strip: recreate the window after Explorer restarts (CodeZeno listens for `TaskbarCreated`).
- [ ] Taskbar strip hover tooltip (web view inside the taskbar gets no mouse input).
- [ ] Codex usage fallbacks: native app-server read, rollout log; keyring storage.

## Distribution
- [x] v0.1.0 published (2026-11-04): public repo `hitraj2003/nubi` (source, MIT), release with installer and `latest.json`. Updater: tauri-plugin-updater, passive NSIS, checks `releases/latest/download/latest.json` 20 s after start and every 6 h, release builds only, off when Settings > Update automatically is off.
- [x] CI (2026-11-04): `.github/workflows/build.yml` builds on main, publishes a signed release on `v*` tags; secret `TAURI_SIGNING_PRIVATE_KEY` set; first run passed. Older versions stay on the Releases page.
- [ ] Each new release (only when the owner says): bump version in `tauri.conf.json` and `Cargo.toml`, tag `vX.Y.Z`, push. Manual fallback: `scripts/latest-json.mjs`.
- [ ] Portable exe with self-update (option B, chosen 2026-11-04): check GitHub Releases, download `<name>.new.exe`, verify signature, rename the running exe to `<name>.old.exe`, swap, restart, delete `.old.exe` next start. Replaces the installer-based updater.
- [ ] Code signing (owner is an individual in India; Azure Trusted Signing does not cover that):
  1. [x] Public repo with an OSI license (MIT).
  2. [x] Build the exe in GitHub Actions.
  3. [x] Publish one unsigned release.
  4. [ ] Apply to SignPath Foundation (free for OSS, signs in CI, publisher shows as "SignPath Foundation").
  5. [ ] Fallback: Certum Open Source Code Signing with SimplySign cloud (shows owner's name, about EUR 25-70/yr, harder to automate).
