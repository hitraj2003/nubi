# Worklog (newest first)

## 2026-11-04 (evening) — Claude Opus 5.5 — local commits, not pushed
- Single instance (tauri-plugin-single-instance): a second launch opens the panel; fixes "pipe already in use". Pushed to main before the owner asked to approve pushes (no release).
- CI: `.github/workflows/build.yml` (tests + build on main, signed release on `v*` tags). First run passed in 10.5 min.
- Claude usage now comes from Claude Code's status line input (`rate_limits.five_hour/seven_day`), not the rate-limited `/api/oauth/usage`. Nubi takes `statusLine` (`nubi-hook.exe --statusline`), saves the previous one to `%APPDATA%\nubi\statusline_prev.json`, runs it and prints its output; turning Claude hooks off restores it. API is a fallback at most every 30 min when no status line has reported for 30 min.
- Verified: 24 tests; `--statusline` with the owner's `statusline.js` prints the same line. Not verified: live update in the GUI, restore on disable.

## 2026-11-04 (later) — Claude Opus 5.5 — release v0.1.0 published
- Renamed to Nubi: Cargo package `nubi`, productName `Nubi`, identifier `dev.nubi.app`, pipe `nubi-<user>`, hook exe `nubi-hook.exe` in `%LOCALAPPDATA%
ubi`, settings in `%APPDATA%
ubi` (read from the old `claude-overlay` folder when empty), env vars `NUBI_DEBUG`, `NUBI_SIM_DIR`. Old `claude-overlay-hook.exe` hook entries count as ours and are replaced on refresh.
- Logo: `src-tauri/icons/nubi.svg` (black creature rising from the bottom edge, terracotta ring on white); all icons regenerated.
- Toast (`ui/overlay.*`): pill, cards and deck on springs (`ui/motion.js`), content blurs in; CSS transitions removed.
- Notch: in fold mode, hover shows the pill and a click opens it.
- Session names: `name` on sessions and requests (Claude `custom-title`/`ai-title` from the transcript, Codex `thread_name` from `~/.codex/session_index.jsonl`, re-read at most every 20 s); shown instead of the folder in toast, notch, panel and alert text. `customTitle` key for `/rename` not confirmed.
- Floating ring, tooltip, taskbar strip: notch material and the shared OKLCH usage scale (`ui/usage-color.js`); ring has session and week rings on springs and an activity ring; with both providers it splits into two clipped halves, each with its own percentage and D-shaped activity loop. Taskbar stays transparent.
- Sounds: one synthesized tone per alert kind (`ui/sounds.js`); Navi sound files removed, notices updated.
- Settings save automatically (no Save button).
- Updater: tauri-plugin-updater, passive NSIS install, checks `github.com/hitraj2003/nubi/releases/latest/download/latest.json` 20 s after start and every 6 h (release builds only). Key `~/.tauri/nubi.key` (no password). `scripts/latest-json.mjs` writes latest.json.
- Published: public repo `hitraj2003/nubi` (source and MIT LICENSE pushed), release `v0.1.0` with installer and latest.json; URL returns 200.
- Verified: 23 unit tests pass, cargo check, release build. Not verified on screen: toast springs, hover fix, session names, new ring/tooltip/taskbar, sounds, autosave. Auto-update not tested end to end.

## 2026-11-04 — Claude Opus 5.5 — uncommitted, debug build only
- New notch (display mode `island`, label "Notch"; old "Edge notch" settings convert to it): one black shape on any edge (top, bottom, left, right), drawn as an SVG path with concave ears, driven by springs in new `ui/motion.js` (tuning numbers from jackson-storm/dynamicnotch, GPL-3.0, no code copied). Files: `ui/island.*`, `src-tauri/src/island.rs`.
- Closed pill: provider marks inside rings (inner session, outer week, spinning when working, pulsing when it needs you); one session shows folder and working bars, several show "N working". Open: tabs Waiting / Sessions / Usage; side edges open as a narrow, tall panel. Drag along the edge reaches both ends; fold until hovered shrinks it to a thin tab.
- Top edge: alerts and requests show inside the notch. Other edges: they go to the top toast. System-prompt approvals are one row (command, session name, Allow, Deny).
- Usage colours: OKLCH scale, pure green 25 %, blue 50 %, orange 75 %, red 100 %, blended; inner ring lighter. Solid colour per ring.
- Fixes: `window.emit` broadcasts to every window in Tauri 2, so all window events now use `emit_to`; page reads edge and offset from state updates (startup race); removed debug eprintln lines in pipe.rs.
- Debug-only test inbox (`CLAUDE_OVERLAY_SIM_DIR`, `pipe::start_sim`): sandboxed scripts cannot reach the named pipe, so tests drop hook JSON files instead. Not in release builds.
- Verified on screen with simulated hooks: pill, open/close motion, tabs, Allow, Deny, question answer, auto-fold, alert, two requests, DND, settings button, fold to tab, right edge, side-edge requests in toast, drag to the ends. 23 unit tests pass. A release build was made before the notch work only.
- Not done: Part B motion for toast, panel (redesign), ring, taskbar; real-session checks. See TODO.md.

## 2026-11-03 (evening) — Claude Opus 5.5 — uncommitted, not built
- Requests moved from the notch into the top notification island: Navi material and motion, stacked deck or side by side, auto-fold after N s, exit straight up while shrinking.
- Approvals in "System prompts" mode are one slim pill (dot, "Allow: command?", Allow, Deny, provider mark); questions list one option per line; Full mode adds Other, Answer in terminal, Jump.
- Settings: questions_mode (off/system/full), request_layout (stack/list), request_popup_secs, takeover_secs (default 10, time bar on each card). Most settings also in the tray menu.
- Claude terracotta / Codex blue on cards; notch attention ring no longer blinks; no window shadows; panel redesigned (unslop pass).
- Verified on screen before the last edits: deck, side by side, slim pill, 10 s takeover with empty hook output, auto-fold. Last edits (provider colours, exit-up motion) not built or checked. No release build of this state.

## 2026-11-03 (later) — Claude Opus 5.5
- Full UI build: edge notch (codenotch styles, rings, fold motion, card), floating ring + tooltip (claude-usage-meter), taskbar strip embedded in the taskbar (CodeZeno), alert island with sounds (Navi), new panel (Inbox, Sessions, Usage, Alerts, Settings).
- Claude AskUserQuestion answered from the notch card or panel via PreToolUse allow + updatedInput (documented Claude Code hook contract).
- Fixes: Tauri CSP hashes blocked inline styles (disabled asset CSP modification for style-src only); usage schedule no longer persisted except 429 backoff; Windows 11 hides topmost windows under the taskbar, so the strip is re-parented like CodeZeno; strip clicks read natively.
- Verified with screenshots and simulated hooks: notch hover + question card; question Send produced the exact updatedInput JSON; panel Allow produced the PermissionRequest allow JSON; strip shows Codex 50% · 4.5h; alert island top-centre; 21 unit tests pass.
- Floating ring: one ring split in half when both providers are on (left Claude, right Codex); tooltip checked on screen.
- Notch: waiting approvals/questions pop out automatically, stacked or side by side (setting); drag along the edge, position saved. Both checked on screen.
- Release installer built: src-tauri/target/release/bundle/nsis/claude-overlay_0.1.0_x64-setup.exe.
- Enabled hooks are rewritten at startup so older entries pick up the 300 s question timeout.
- Not verified: ring tooltip, real sessions, toast, release-mode hook I/O, other monitors/edges.

## 2026-11-03 — Claude Opus 5.5
- Analysed Terryxxx/Navi, vinzdg/codenotch, Affirm/navi, yonathanamir/claude-usage-meter, CodeZeno/Claude-Code-Usage-Monitor; wrote plan; Codex (gpt-6-astra low) reviewed it, findings merged.
- Confirmed Codex CLI 0.166.0 has a blocking PermissionRequest hook with the same contract as Claude.
- Installed rustup, VS 2022 C++ Build Tools, tauri-cli 2.12.4.
- Built basic functional version: hook client, secured named pipe, Claude + Codex hook install, sessions, inline approvals, usage (Claude + Codex), alerts with toast fallback, DND (hotkey, quiet hours), settings, plain panel UI.
- Verified: 15 unit tests pass; hook with GUI closed exits 0 silently; foreign pipe client rejected; unanswered approval ends at 95 s with no output; client disconnect cleans up; Codex usage live (5h 34%, 7d 73%).
- Not verified: real Allow/Deny round trip, terminal jump, toast, release build. Claude usage shows needs_auth: saved token in ~/.claude/.credentials.json expired ~8 days ago.
