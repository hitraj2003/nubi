# TODO

## Owner review
- [ ] Review all three display modes, the alert island and the panel.
- [ ] Try Allow/Deny and a Claude question against a real session (both proven with simulated hook input).

## Needs manual check (not yet verified)

- [ ] Codex approvals after `/hooks` trust.
- [ ] What Claude Code and Codex do when the hook times out (expect native prompt).
- [ ] Jump to terminal in Windows Terminal and VS Code.
- [ ] Toast while a full-screen app is open.
- [ ] Release build: hook mode stdout under `windows_subsystem = "windows"`.
- [ ] Notch with the top/bottom edge and on a second monitor / other DPI.

## Next
- [ ] Taskbar strip: recreate the window after Explorer restarts (CodeZeno listens for `TaskbarCreated`).
- [ ] Taskbar strip hover tooltip (web view inside the taskbar gets no mouse input).
- [ ] Claude usage fallback when the saved token is stale (codenotch asks `claude` for `/usage`).
- [ ] Codex usage fallbacks: native app-server read, rollout log; keyring storage.
- [ ] Notch: drag along the edge, settings orb (not ported).
- [ ] Remove debug eprintln lines in pipe.rs before release.
