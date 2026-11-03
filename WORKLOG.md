# Worklog (newest first)

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
