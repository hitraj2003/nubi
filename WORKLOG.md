# Worklog (newest first)

## 2026-11-03 — Claude Opus 5.5
- Analysed Terryxxx/Navi, vinzdg/codenotch, Affirm/navi, yonathanamir/claude-usage-meter, CodeZeno/Claude-Code-Usage-Monitor; wrote plan; Codex (gpt-6-astra low) reviewed it, findings merged.
- Confirmed Codex CLI 0.166.0 has a blocking PermissionRequest hook with the same contract as Claude.
- Installed rustup, VS 2022 C++ Build Tools, tauri-cli 2.12.4.
- Built basic functional version: hook client, secured named pipe, Claude + Codex hook install, sessions, inline approvals, usage (Claude + Codex), alerts with toast fallback, DND (hotkey, quiet hours), settings, plain panel UI.
- Verified: 15 unit tests pass; hook with GUI closed exits 0 silently; foreign pipe client rejected; unanswered approval ends at 95 s with no output; client disconnect cleans up; Codex usage live (5h 34%, 7d 73%).
- Not verified: real Allow/Deny round trip, terminal jump, toast, release build. Claude usage shows needs_auth: saved token in ~/.claude/.credentials.json expired ~8 days ago.
