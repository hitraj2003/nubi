# claude-overlay

Windows overlay for **Claude Code** and **Codex CLI**. One `claude-overlay.exe`, built with
Tauri 2 and Rust.

Status: basic functional build. The final visuals (edge notch with codenotch animations,
floating ring, taskbar strip) come in the UI pass. Plan:
[`docs/plans/2026-11-03-claude-overlay.md`](docs/plans/2026-11-03-claude-overlay.md).

## What works now

- **Alerts:** task complete, permission needed, waiting for input. A pill window with a
  sound, or a Windows toast when a full-screen app is in front.
- **Inline approve/deny** for Claude Code and Codex `PermissionRequest` hooks, from the
  panel. No answer within ~95 s, or any error: the hook prints nothing and the CLI shows
  its own prompt.
- **Session list:** state per session (idle / working / waiting / done / stale),
  sub-agent count, context size, jump to the terminal window.
- **Usage limits:** Claude (5 h, 7 d) and Codex (5 h, weekly). Read-only use of each CLI's
  own sign-in. Polled every 5 minutes; HTTP 429 backs off.
- **Providers setting:** Claude, Codex, or both.
- **Do Not Disturb:** tray item, panel button, global hotkey (default `Ctrl+Alt+D`), quiet
  hours, Windows quiet time. Alerts during DND are queued in the panel, not shown.
- **Display settings:** mode (notch / ring / taskbar), monitor, notch edge, taskbar side.
  Placement is basic until the UI pass.

## Build and run

Needs Rust stable, VS 2022 C++ Build Tools, WebView2 (included in Windows 11), and
`cargo install tauri-cli --version "^2"`.

```
cd src-tauri
cargo test
cargo tauri dev        # run
cargo tauri build      # installer in target/release/bundle/nsis
claude-overlay.exe --diag   # print usage and hook status (no tokens)
```

## Turn on hooks

Open the panel from the tray icon, then **Hooks › Enable** for Claude and/or Codex.

- Claude: entries are added to `~/.claude/settings.json`.
- Codex: entries are added to `$CODEX_HOME/hooks.json` (default `~/.codex`). Then run
  `/hooks` once inside Codex to trust them; Codex does not run new hooks until you do.

Hooks call a stable copy of the app at
`%LOCALAPPDATA%\claude-overlay\claude-overlay-hook.exe`, so moving the app is safe.
**Disable** removes only our entries. A backup of each file is kept as
`*.json.claude-overlay.bak` the first time we change it.

## Security notes

- The pipe `\\.\pipe\claude-overlay-<user>` only allows the current user and SYSTEM,
  rejects network clients, refuses to start if another process already owns the name,
  and only accepts clients running this exe or the hook copy.
- Tool input larger than 32 KB is not shown and cannot be approved from the overlay.
- Credentials stay in the Rust process. They are never logged or sent anywhere except the
  provider's own usage endpoint.

## Credits

See [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
