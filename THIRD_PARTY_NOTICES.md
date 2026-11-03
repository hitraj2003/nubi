# Third-party notices

claude-overlay copies or recreates code, styles, sounds and designs from the projects below.
Full licence texts are in [`third_party/`](third_party/).

| Project | Licence | Commit | What we use | Where |
|---|---|---|---|---|
| [Terryxxx/Navi](https://github.com/Terryxxx/Navi) | Apache-2.0 | e8e0c40 | Alert island look, motion, icons, queue timing; `success.wav`, `error.mp3`; placeholder app icon | `ui/overlay.*`, `ui/sounds/`, `src-tauri/icons/` |
| [vinzdg/codenotch](https://github.com/vinzdg/codenotch) | MIT, © 2026 Vinz | 6522e9f | Edge notch styles, ring drawing, tones, card and tail, fold-on-hover motion; notch window sizes and work-area placement; Claude/Codex usage endpoints | `ui/notch.*`, `src-tauri/src/notch.rs`, `src-tauri/src/usage.rs` |
| [lobehub/lobe-icons](https://github.com/lobehub/lobe-icons) (via codenotch) | MIT, © LobeHub | 1.95.0 | `claude.svg`, `codex.svg` (OpenAI mark). Trademarks of Anthropic and OpenAI, used only to name the product shown. | `ui/glyphs/` |
| [yonathanamir/claude-usage-meter](https://github.com/yonathanamir/claude-usage-meter) | MIT, © 2026 Claude Usage Meter Contributors | 260e4f2 | Floating ring and tooltip design, sizes, colours, snap-to-edge motion (recreated from Python/Qt) | `ui/ring.*`, `ui/ringtip.*`, `src-tauri/src/ring.rs` |
| [CodeZeno/Claude-Code-Usage-Monitor](https://github.com/CodeZeno/Claude-Code-Usage-Monitor) | MIT, © 2026 Code Zeno Pty Ltd | dea72c9 | Taskbar strip layout, colours, duration format; taskbar discovery, tray-relative placement, child embedding, full-screen rule | `ui/taskbar.*`, `src-tauri/src/taskbar.rs` |
| [Affirm/navi](https://github.com/Affirm/navi) | BSD-3-Clause | — | Ideas only (session list, inline approvals); no code copied | — |

## Changes from the originals

- Navi: the island is clickable (opens the panel) and shows Claude or Codex in the title.
- codenotch: settings orb, move grip, carry and adaptive pill are not ported; the card adds
  approvals, Claude questions and a session list; click-through is driven by a hot rectangle the
  page reports.
- claude-usage-meter: drawn in SVG; a bottom-left badge also lights for waiting approvals.
- Claude-Code-Usage-Monitor: drawn in HTML; rows are Claude 5h + Codex 5h when both providers are
  on; clicks are read natively because a web view inside the taskbar does not receive them.
