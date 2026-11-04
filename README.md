# Nubi

Nubi is a small Windows app for people who run Claude Code or Codex CLI in a terminal. It tells
you when a session finishes or needs you, lets you approve tool calls and answer Claude's
questions without switching windows, and shows how much of your usage limit is left. It is one
`nubi.exe`, built with Tauri 2 and Rust.

## Download

Get `Nubi_<version>_x64-setup.exe` from the
[latest release](https://github.com/hitraj2003/nubi/releases/latest). It installs for your user
account and needs no admin rights. The installer is not code-signed yet, so Windows SmartScreen
may stop it: choose More info, then Run anyway.

Older installers stay on the [releases page](https://github.com/hitraj2003/nubi/releases).
Nubi checks for a new version when it starts and every 6 hours after that, and updates itself.
You can turn that off in Settings.

## Display modes

Pick one in Settings > Display.

Notch: a black island at the top of the screen, or on any other edge. It shows one ring per
tool (5-hour limit inside, weekly limit outside), spins while a session works, and pulses when
something waits for you. Hover it for the card with limits, sessions, approvals and questions.
Drag it along its edge to move it; Nubi remembers the spot.

Floating ring: a ring you can drag anywhere; it snaps to the nearest side of the screen. With
Claude and Codex both on, the left half is Claude and the right half is Codex. Click to switch
between the 5-hour and weekly limit, right-click to open the panel.

Taskbar strip: sits inside the Windows taskbar, next to the tray or at the left end. With both
tools on, the top row is Claude and the bottom row is Codex. Click it to open the panel.

In every mode, alerts appear as a pill at the top centre of the screen, with a short sound for
each kind (done, permission, question, waiting).

## Approve and answer from Nubi

When Claude Code or Codex asks to run a tool, the request appears in the notch and in the panel
(Inbox, and on top of Sessions). Allow or Deny it there. If you don't answer within the time set
in Settings > Requests (10 seconds by default), the terminal asks you instead. The same happens
if anything goes wrong.

When Claude Code asks a multiple-choice question (`AskUserQuestion`), pick an option or type
your own and press Send. Claude gets it as your answer. Answer in terminal hands the question
back. Codex has no documented way to answer its questions from outside, so those stay in the
terminal.

## The panel

Open it from the tray icon. Closing it sends it back to the tray.

Inbox lists waiting approvals and questions. Sessions lists every running session with its
state, context size and sub-agents; Jump brings its terminal to the front. Usage shows the
5-hour and weekly limits for Claude and Codex. Alerts keeps the last 50.

Settings covers display mode, monitor, theme (System, Light or Dark) and accent colour, request
timing, sounds, do not disturb, start with Windows, automatic updates and hooks.

Do not disturb can be turned on from the panel, the tray, a hotkey (`Ctrl+Alt+D` by default),
quiet hours, or Windows quiet time. Alerts that arrive meanwhile wait in the panel.

## Where the usage numbers come from

Claude's limits come from the status line Claude Code passes to Nubi. If you already had a
status line, Nubi runs it and prints its output, so it still shows; turning Claude hooks off
puts it back. Codex limits come from Codex's own sign-in. Nubi only reads them and never sends
your credentials anywhere except the provider's own usage endpoint.

## Turn on hooks

In the panel, open Settings > Hooks and turn on Claude, Codex, or both.

Claude: Nubi adds its entries to `~/.claude/settings.json`.

Codex: Nubi adds its entries to `$CODEX_HOME/hooks.json` (default `~/.codex`). Run `/hooks` once
inside Codex to trust them; Codex ignores new hooks until you do.

The hooks call a fixed copy of the app at `%LOCALAPPDATA%\nubi\nubi-hook.exe`, so you can move
or update Nubi without breaking them. Turning hooks off removes only Nubi's entries. The first
time Nubi changes one of these files it keeps a backup next to it as `*.json.nubi.bak`.

## Security

Nubi talks to the hooks over the pipe `\\.\pipe\nubi-<user>`. Only your user and SYSTEM can
open it, network clients are refused, and Nubi only accepts clients running its own exe or the
hook copy. If another process already owns the pipe name, Nubi won't start.

Tool input larger than 32 KB is not shown, and you can't approve it from Nubi.

## Build from source

You need Rust stable, the Visual Studio 2022 C++ build tools, WebView2 (part of Windows 11) and
`cargo install tauri-cli --version "^2"`.

```
cd src-tauri
cargo test
cargo tauri dev        # run
cargo tauri build      # installer in target/release/bundle/nsis
nubi.exe --diag   # print usage and hook status (no tokens)
```

## Releasing a new version

Bump `version` in `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`, commit, then tag and
push:

```
git tag v0.1.1 && git push origin main v0.1.1
```

GitHub Actions (`.github/workflows/build.yml`) builds the installer, signs the update and
publishes the release with `latest.json`. It needs the repo secret `TAURI_SIGNING_PRIVATE_KEY`,
which holds the contents of `~/.tauri/nubi.key`.

## Credits

Nubi borrows ideas from codenotch, claude-usage-meter, Claude-Code-Usage-Monitor and Navi. See
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
