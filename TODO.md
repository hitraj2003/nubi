# TODO

## Needs manual check (not yet verified)
- [ ] Allow and Deny from the panel against a real Claude Code session (round trip proven only for timeout and client-disconnect).
- [ ] Same for Codex after `/hooks` trust.
- [ ] What Claude Code and Codex do when the hook times out (expect native prompt).
- [ ] Jump to terminal in Windows Terminal and VS Code.
- [ ] Toast while a full-screen app is open.
- [ ] Release build (`cargo tauri build`): hook mode stdout under `windows_subsystem = "windows"`.

## Next
- [ ] UI pass: edge notch with codenotch animations, floating ring (claude-usage-meter look), taskbar strip (CodeZeno embedding, monitor + left/right side).
- [ ] Claude usage fallback when the saved token is stale (codenotch asks the installed `claude` for `/usage`).
- [ ] Codex usage fallbacks: native app-server read, then rollout log; keyring storage.
- [ ] Re-place overlay on monitor/DPI/taskbar changes.
- [ ] Licence texts, copyrights and pinned commits in THIRD_PARTY_NOTICES.md when code is copied in.
- [ ] Remove debug eprintln lines in pipe.rs before release.
