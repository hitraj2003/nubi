# Nubi

Windows overlay for **Claude Code** and **Codex CLI**. One `nubi.exe`, built with
Tauri 2 and Rust.

Status: full build with all three display modes, awaiting the owner's review. Plan:
[`docs/plans/2026-11-03-claude-overlay.md`](docs/plans/2026-11-03-claude-overlay.md).

## Download

- **Latest:** [latest release](https://github.com/hitraj2003/nubi/releases/latest), file
  `Nubi_<version>_x64-setup.exe`. Installs per user, no admin needed. Windows SmartScreen may warn
  because the installer is not code-signed yet: choose More info, then Run anyway.
- **Older versions:** [all releases](https://github.com/hitraj2003/nubi/releases). Every version
  stays there with its own installer.
- Installed copies update themselves from the latest release (checked at start and every 6 hours).

## Releasing a new version

1. Bump `version` in `src-tauri/tauri.conf.json` (and `src-tauri/Cargo.toml`).
2. Commit, then tag and push: `git tag v0.1.1 && git push origin main v0.1.1`.
3. GitHub Actions (`.github/workflows/build.yml`) builds, signs the update, and publishes the
   release with the installer and `latest.json`. Needs the repo secret
   `TAURI_SIGNING_PRIVATE_KEY` (contents of `~/.tauri/nubi.key`).

## Display modes (pick one in Settings)

- **Edge notch** (from codenotch): pinned to any screen edge, folds to a small tab until the
  pointer reaches it, one ring per provider (5h arc, 7d outer ring, spinning arc while working,
  yellow pulse when something waits). Hover a ring for the card: limits, sessions, approvals and
  Claude's questions. Approvals and questions pop out beside the notch on their own, stacked or
  side by side (Settings). Drag the notch along its edge to move it; the spot is saved.
- **Floating ring** (from claude-usage-meter): draggable, snaps to the nearest side edge. With
  both providers on it is one ring split in half: left Claude, right Codex. Click toggles 5h/7d; hover shows the tooltip; right-click opens the panel.
- **Taskbar strip** (from Claude-Code-Usage-Monitor): sits inside the taskbar, right beside the
  tray or at the left. Claude + Codex on: row 1 Claude 5h, row 2 Codex 5h. Click opens the panel.
- **Alert island** (from Navi): top-centre pill with sound for done / permission / question /
  waiting, in every mode.

## Answer Claude's questions from the overlay

When Claude Code asks a multiple-choice question (`AskUserQuestion`), the question appears in the
notch card and the panel Inbox. Pick an option (or type your own) and press **Send**. Claude
receives it as your answer. **Answer in terminal**, or no answer within ~4 minutes, hands the
question back to the terminal. Codex has no documented way to do this, so its questions stay in
the terminal.

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
nubi.exe --diag   # print usage and hook status (no tokens)
```

## Turn on hooks

Open the panel from the tray icon, then **Hooks › Enable** for Claude and/or Codex.

- Claude: entries are added to `~/.claude/settings.json`.
- Codex: entries are added to `$CODEX_HOME/hooks.json` (default `~/.codex`). Then run
  `/hooks` once inside Codex to trust them; Codex does not run new hooks until you do.

Hooks call a stable copy of the app at
`%LOCALAPPDATA%\nubi\nubi-hook.exe`, so moving the app is safe.
**Disable** removes only our entries. A backup of each file is kept as
`*.json.nubi.bak` the first time we change it.

## Security notes

- The pipe `\\.\pipe\nubi-<user>` only allows the current user and SYSTEM,
  rejects network clients, refuses to start if another process already owns the name,
  and only accepts clients running this exe or the hook copy.
- Tool input larger than 32 KB is not shown and cannot be approved from the overlay.
- Credentials stay in the Rust process. They are never logged or sent anywhere except the
  provider's own usage endpoint.

## Credits

See [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
