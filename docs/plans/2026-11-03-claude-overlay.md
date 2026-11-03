# claude-overlay — plan (2026-11-03)

Windows overlay for Claude Code that combines three open-source projects into one
Tauri 2 + Rust app. Status: draft, awaiting user approval before build.

## Sources and licenses

| Source | License | What we take |
|---|---|---|
| Terryxxx/Navi | Apache-2.0 | Pill alerts, named-pipe hook client, safe settings.json hook toggle |
| vinzdg/codenotch (`windows/`) | MIT | Usage-limit rings, Claude usage readers, notch look |
| Affirm/navi | BSD-3-Clause | Session list, terminal jump, inline approve/deny, sub-agent tree, context bar (ideas; Swift, so rewrite) |

Keep all three license notices in `THIRD_PARTY_NOTICES.md`. Ported files keep their original headers.

## Architecture

```
Claude Code hooks ──stdin JSON──► claude-overlay.exe --hook ──► \\.\pipe\ClaudeOverlay ──► GUI process
                                        ▲                                                   │
                                        └──── PermissionRequest reply (allow/deny) ◄────────┘
```

- One exe, two modes: GUI (tray + overlay windows) and `--hook` (short-lived client).
- Pipe server in GUI; ACL limited to current user. Payload cap 10 KB, field allowlist.
- `PermissionRequest` hook blocks, waits for user answer on the pipe, prints the hook
  decision JSON to stdout. Timeout (default 60 s) falls back to Claude Code's own prompt.
- Session discovery: scan `~/.claude/projects/*/*.jsonl` + hook events; state machine
  per session: working / idle / waiting-permission / done / error.
- Usage: Claude `/usage` data via OAuth token from Claude Code credentials, read-only,
  never logged or sent anywhere except the official endpoint.

## Modules (src-tauri/src)

- `main.rs` — mode switch, tray, windows
- `hook_client.rs` — `--hook` mode
- `pipe.rs` — server, framing, ACL
- `hooks_config.rs` — enable/disable entries tagged `"source": "claude-overlay"`
- `sessions.rs` — discovery + state machine + sub-agents + context size
- `permissions.rs` — pending requests, reply routing, timeout
- `usage.rs` — Claude limits (5 h session, weekly), refresh every 60 s
- `terminal.rs` — focus terminal window by PID (Windows Terminal, VS Code, conhost)
- `ui/` — pill, notch rings, expanded panel (session list, approval cards)

## Phases

1. Toolchain + scaffold: rustup, MSVC, `cargo tauri init`, tray, empty overlay.
2. Hooks + pipe + pill alerts (Navi parity). Test notification.
3. Session list + state machine + jump to terminal.
4. Inline approve/deny (blocking hook). Most risk; test with timeout fallback.
5. Usage rings (Claude first; Codex/Cursor later).
6. Settings window, sounds, autostart, installer (NSIS, per-user).

Each phase ends with a working build and a manual check.

## Accepted scope additions (2026-11-03)

- **Providers:** Claude and Codex. Setting: Claude only / Codex only / both. Codex usage
  from the local Codex sign-in (5 h + weekly windows), ported from codenotch `windows/`.
- **Three display modes** (user picks one in settings, switch any time):
  1. **Edge notch** — pinned to a screen edge, codenotch style.
  2. **Floating ring** — small draggable ring icon, expands on click.
  3. **Taskbar strip** — compact bars sitting on the taskbar left of the tray, per
     reference image: rows `5h` and `7d`, segmented bar, `27% · 2h` (used % · time to
     reset). Windows 11 has no taskbar-widget API, so this is a borderless always-on-top
     window aligned to the taskbar rect (TrafficMonitor approach). Must re-align on
     taskbar move, auto-hide, DPI change, and full-screen apps.
- **Windows toast fallback** (idea 7) when a full-screen app is in front.
- **Do-not-disturb** (idea 8): quiet hours, follow Focus Assist, plus a global hotkey to
  toggle DND (default Ctrl+Alt+D, rebindable).
- **Multi-monitor** (idea 10): choose monitor and edge/position per display mode.

### Display-mode sources (2026-11-03, user choice)

| Mode | Copy from | License | Notes |
|---|---|---|---|
| Edge notch + its animations | vinzdg/codenotch `windows/codenotch/ui/notch.html` (+ CSS/JS) | MIT | Keep expand/collapse, pulse and state motion as-is |
| Floating ring | yonathanamir/claude-usage-meter (`meter_widget.py`, `tooltip_widget.py`) | MIT | Python/Qt; recreate look and hover tooltip in HTML/CSS |
| Taskbar strip | CodeZeno/Claude-Code-Usage-Monitor (`src/`) | MIT | Native Rust taskbar widget; port its taskbar placement code directly — replaces our own TrafficMonitor-style approach |

Add both new repos to `THIRD_PARTY_NOTICES.md`.

Phase changes: usage phase (5) covers Claude + Codex. Display modes split across phase 2
(edge notch), phase 5 (floating ring + taskbar strip). DND, toast, hotkey, multi-monitor in
phase 6.

### Taskbar strip placement (2026-11-03, user)

Settings: which monitor's taskbar, and which side — left or right of the taskbar
(right = beside the tray, left = start of the taskbar). Same pattern as the other modes.

## Codex review (2026-11-03, gpt-6-astra low, read-only) — accepted changes

1. **Phase 1b spikes before any UI work:** blocking approval round-trip, Windows
   credential reads (Claude + Codex), taskbar embedding of a native HWND next to Tauri.
2. **Approval contract:** hook exits 0 and prints only
   `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"|"deny"}}}`.
   Client deadline below the hook timeout. On timeout, disconnect or oversize input: print
   nothing, exit 0 (native prompt takes over). Never approve truncated content. Each reply
   bound to request id + session + connection; late replies rejected, cards expire.
3. **Pipe:** per-user pipe name, explicit current-user DACL, reject remote clients,
   `FILE_FLAG_FIRST_PIPE_INSTANCE` against squatting, check client PID/image path.
   Bounded stdin, frames, queues.
4. **Usage:** parse the windows the API actually returns (missing is not zero). Poll with a
   5-minute floor and persisted 429 backoff. Codex: honour `CODEX_HOME`, keyring storage,
   and `wham/usage` → app-server → rollout fallbacks from codenotch; never refresh Codex
   tokens ourselves. Unsupported/unavailable states shown explicitly.
5. **Capability matrix:** Claude = alerts, sessions, approvals, usage. Codex = usage only
   (v1). Cursor removed from scope.
6. **Terminal jump:** best effort — activate the owning window; tab/pane focus only where
   the terminal exposes it.
7. **settings.json edits:** atomic write, keep unrelated hooks, remove only our entries.
8. **Sessions:** incremental JSONL reads, tolerate partial lines, stale/crashed timeout.
9. **Tauri boundary:** tool input rendered as text only, minimal IPC capabilities,
   credentials stay in Rust, no remote content. `--hook` mode handles stdin/stdout before
   any WebView/tray start.
10. **Licenses:** full license texts + copyrights, Apache NOTICE, note modifications, pin
    source commits in `THIRD_PARTY_NOTICES.md`.
11. **DND:** approval cards stay reachable in the panel; alerts queue; toasts follow
    Windows notification settings.
12. Automated tests for the hook protocol and race cases, not only manual checks.

## Open questions

- ~~Taskbar strip with both providers~~ Decided 2026-11-03: row 1 = Claude 5 h, row 2 = Codex 5 h.
  With one provider on, the strip keeps the reference layout (5 h + 7 d of that provider).
- Code signing: none (SmartScreen warning) unless user has a certificate.
