# TODO

## Pick up here (next session)
- [ ] Part B: same spring motion as the notch (`ui/motion.js`) in the other views, then test each with the owner (owner says "again" to replay, "next" to move on):
  - [ ] Toast (`ui/overlay.*`): alert pill and request cards. Replace CSS width/height transitions with springs, blur-in content.
  - [ ] Settings panel (`ui/panel.*`): redesign dark, same material as the toast (pure black, white ink), mixing the owner's moodboard: sidebar with grouped rows, segmented controls, toggles, card rows. Add motion (sliding nav highlight, view crossfade, springy switches).
  - [ ] Floating ring (`ui/ring.*`, `ringtip.*`): hover tooltip and ring updates with springs; use the same OKLCH usage colours and inner/outer rings as the notch.
  - [ ] Taskbar strip (`ui/taskbar.*`): value changes animate; same usage colours.
- [ ] Part C: real-session checks (list under "Needs manual check").
- [ ] Remove the old edge notch files (`ui/notch.*`, notch window in `tauri.conf.json`) once the owner confirms the new notch replaces it. Keep `notch.rs` helpers (edge placement, drag, cursor watch) used by `island.rs`.
- [ ] Inner session ring is amber-ish at 60–84 %, close to the outer week ring colour range: ask the owner whether to keep.
- [ ] Commit (nothing since cd9c4d5 is committed). Release build after Part B.

## Needs manual check (not yet verified)
- [ ] Bug (owner, 2026-11-04): Settings > "Fold until hovered" switch is not clickable, and it saved automatically. Seen on the build before the draft/Save change (d33cb55); check whether it still happens.
- [ ] Codex approvals after `/hooks` trust.
- [ ] What Claude Code and Codex do when the hook times out (expect native prompt).
- [ ] Jump to terminal in Windows Terminal and VS Code (session rows in the notch are clickable).
- [ ] Toast while a full-screen app is open.
- [ ] Release build: hook mode stdout under `windows_subsystem = "windows"`.
- [ ] Notch on the bottom and left edges, and on a monitor with another DPI (top and right tested).
- [ ] Context size column in the notch Sessions tab with real sessions.

## Next
- [ ] Later (owner, 2026-11-04): agent-drafted bug reports. Report dialog in Settings: user writes a sentence; Nubi collects version, Windows, display mode/edge, providers, hook and usage status, Claude/Codex versions (no tokens); "Draft with Claude/Codex" runs the local CLI (`claude -p --bare --tools "" --no-session-persistence`, or `codex exec --ephemeral --skip-git-repo-check -o <file>`, prompt on stdin, temp cwd) and gets JSON for every bug-form field; user edits, then the prefilled `bug_report.yml` form opens and they only press Submit. Keep the URL under ~7.5k chars.
- [ ] Taskbar strip: recreate the window after Explorer restarts (CodeZeno listens for `TaskbarCreated`).
- [ ] Taskbar strip hover tooltip (web view inside the taskbar gets no mouse input).
- [ ] Check on screen: Claude usage from the status line (`--statusline`), owner's status line still shows, turning Claude hooks off restores it.
- [ ] Codex usage fallbacks: native app-server read, rollout log; keyring storage.

## After the build is done: distribution
- [x] Published v0.1.0 (2026-11-04): public repo `hitraj2003/nubi` (source pushed 2026-11-04, MIT), release `v0.1.0` with `Nubi_0.1.0_x64-setup.exe` and `latest.json` (both in `src-tauri/target/release/bundle/nsis/`). Updater (tauri-plugin-updater, passive NSIS) checks `releases/latest/download/latest.json` 20 s after start and every 6 h; release builds only. Not yet tested end to end.
- [ ] Each new release: bump version, tag `vX.Y.Z`, push; `.github/workflows/build.yml` builds and publishes (see README "Releasing"). Needs repo secret `TAURI_SIGNING_PRIVATE_KEY`; manual fallback: `scripts/latest-json.mjs`.
- [ ] Owner: set the secret (`gh secret set TAURI_SIGNING_PRIVATE_KEY -R hitraj2003/nubi < ~/.tauri/nubi.key`), then push; first CI run not yet checked. Back up `~/.tauri/nubi.key`: losing it breaks updates for installed copies.
- [ ] Portable exe with self-update (option B, chosen 2026-11-04): the app checks GitHub Releases, downloads `<name>.new.exe`, verifies SHA256/signature, renames the running exe to `<name>.old.exe`, swaps in the new one, restarts, and deletes `.old.exe` on the next start. No installer; not `tauri-plugin-updater` (it runs an NSIS/MSI installer on Windows).
- [ ] Code signing (owner signs as an individual in India; Azure Trusted Signing does not cover that):
  1. Make the repo public with an OSI license (e.g. MIT).
  2. Build the exe in GitHub Actions.
  3. Publish one unsigned release.
  4. Apply to SignPath Foundation (free for OSS, signs in CI, publisher shows as "SignPath Foundation").
  5. Fallback: Certum Open Source Code Signing with SimplySign cloud (shows owner's name, about EUR 25-70/yr, harder to automate).
