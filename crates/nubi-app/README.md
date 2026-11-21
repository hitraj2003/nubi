# Nubi native executable (stage 3)

This is an internal migration build. It runs the existing Tauri-free core with a
Slint software-rendered notch, native tray menu, configured global shortcuts,
Windows toast notifications and a signed updater. The Tauri application remains
in the workspace.

Build and run the native executable in a separate target directory:

```powershell
cargo build --release --offline -p nubi-app --target-dir target/native
$env:NUBI_DEV_DATA_DIR = "$PWD/target/native-dev"
./target/native/release/nubi.exe --dev
```

Both application packages intentionally retain the binary name `nubi`. Cargo
warns about their shared workspace output path. Do not use that ambiguous output
for native measurements or packaging. The dedicated target directory above
contains only the native application.

Development mode uses a separate hook pipe, activation pipe, mutex and settings
directory. It does not read legacy settings, install hooks, register a browser
host, change the production auto-start entry, or check/install updates. The
optional `NUBI_DEV_DATA_DIR` override is honored only with `--dev`. Without that
override, development settings live in `%APPDATA%/nubi-dev`. Production transport
and settings names retain their original defaults.

`--hook`, `--statusline`, `--compact-session`, `--diag`, and Chrome native-host
calls run before GUI ownership or Slint initialization. Development isolation
also applies to those modes. `--settings` opens the settings window;
`--launcher` opens the Explorer-folder launcher. `--dev --exit-after=N` provides
a bounded runtime for local measurements.

## Integration

- `NativeHost` preserves core session transitions, status-line usage, pending
  decisions, held deadlines, terminal takeover, DND and bounded alert history.
  Snapshots are coalesced; activation and UI events use a bounded channel.
- The GUI owns its named mutex on the main thread. Its global namespace matches
  the pipe across Windows sessions, while the SID and DACL retain user isolation.
  The activation protocol has
  an explicit version and typed actions. Both endpoints validate the peer's
  executable. The activation pipe reuses the core's current-user/SYSTEM DACL,
  rejects remote clients and reserves its first instance. A second launch retries
  while the owner starts and recovers a mutex abandoned during startup.
- The tray menu contains the existing labels and saves style/sound changes.
  Launcher and DND shortcuts come from settings. Shortcut changes are registered
  before persistence and rolled back if settings cannot be applied.
- The notch uses the spike's fonts, tokens, geometry, spring integration, curved
  hit testing and 40/140 ms hover dwell. Session rows and usage percentages come
  from core state rather than fixtures. Session activation focuses its terminal.
- Animation advances on native redraws at the current monitor's refresh rate.
  One deadline timer wakes the next frame; the 16 ms service poll does not step
  springs. Shape rendering and curved hit testing retain the same subpixel
  geometry. The tab highlight uses the existing `ui/motion.js` snappy spring.
- Keyboard scopes behind pointer controls have zero-size bounds, so they cannot
  consume the first mouse press. Pointer clicks still explicitly focus controls;
  keyboard navigation and request countdown holds retain their existing behavior.
- Fonts keep the source theme's roles: installed Segoe UI Variable Text for the
  interface, bundled Geist for figures, and JetBrains Mono for paths and commands.
  The notch requests Segoe Text at weight 600 for source CSS 500 labels: Chrome's
  Windows font selection resolves that CSS request to the named Text Semibold
  instance (wght 600, opsz 10.5), not the variable font's interpolated weight 500.
  `assets/prepare-tabular.py` derives Nubi Figures from the bundled static Geist
  faces by mapping digits to their existing `tnum` glyphs. It preserves outlines
  and the Geist OFL license; no Windows font is copied. Small-text rasterization
  still differs from Chrome and requires desktop visual acceptance.
  Closed width comes from content layout rather than fixed provider slots or
  character counts. Magnified invisible text measurements retain fractional
  advances despite Slint rounding preferred text widths to whole pixels; the
  shell rounds only the final width, as DOM offsetWidth does.
  The workspace's small Slint software-renderer patch enables outline hinting;
  its source and original licenses live in `vendor/i-slint-renderer-software`.
- Settings opens lazily. A hidden settings surface is released after 60 seconds;
  editable General values and navigation survive release. Launcher surfaces are
  released after cancellation. The panel is still the partial spike port.
- The updater reads its endpoint and minisign public key directly from the
  canonical Tauri configuration at compile time. It accepts the existing static
  and dynamic manifests and verifies the artifact and authenticated version
  comment. Responses are bounded, truncated downloads fail, and installer bytes
  are verified again while held against writes. NSIS retains passive update and
  restart arguments. Automatic updates download silently, then show a persistent
  ready card; installation requires Install now. Later retains verified bytes and
  shows the card again at the next check. With automatic updates off, the offer
  dismisses after ten displayed seconds, paused while hidden, hovered or focused.
  Update now downloads and installs without a second prompt; Cancel stops the
  pending installation, and failures expose Retry and Later. The Waiting-pane card
  follows permission and question prompts. The development-only
  `NUBI_DEV_UPDATE_NOTICE=on` previews the ready card; `off` or `1` previews the
  available card. Samples never use the network and report release-only updates
  when installation is requested.

## Acceptance limits

The spike's [PARITY.md](../../spikes/notch-slint/PARITY.md) is a historical visual
comparison. Its old font-weight limitation is not reproduced by the current
typography tests. This executable does not establish visual or interaction parity
for later stages.

- The live notch renders requests, alerts, all tabs and dynamic states. Stage 3
  still requires owner acceptance of smoothness and typography, physical pointer
  checks, monitor transitions, dragging, and complete keyboard behavior.
- Pending decisions are connected to the notch. Separate approval/question
  overlay reconciliation remains stage 4. Unanswered requests fall back to terminals.
- The tray stores ring/taskbar selections, but their native surfaces remain
  stage 5. The current minimal notch remains the display surface.
- The launcher window provides functional callbacks and keyboard actions; its
  exact source styling, motion, focus restoration and monitor placement still
  require the stage 5 parity pass. Update notices use the notch card described above.
- The partial settings panel implements General persistence. Other settings panes,
  Inbox requests, complete session actions, report/release browsing, accessibility
  and responsive behavior remain stage 6. Some inherited prototype controls have
  no native implementation yet.
- DeskTime session messages return an explicit unavailable error. Cookie import,
  email login and the on-demand WebView2 reader remain stage 7. This executable
  has no Tauri, Wry or WebView2 dependency.
- Real toast delivery with the installed AUMID, physical shortcuts, Narrator,
  IME, high contrast, multiple monitors and foreground/focus restoration need a
  desktop acceptance pass. No registrations were installed for this work.
- Installer launch, upgrades from 0.1.3, native-to-native upgrades, locked-helper
  recovery and stale-installer cleanup remain unverified or incomplete. Downloads
  canceled by the UI cannot install, but the bounded HTTP read can finish before
  the worker returns. No production installer was run.
- The current executable exceeds the plan's 10 MB installed-size budget. Packaging
  and size optimization remain open.

## Verification

For an isolated animation trace, set `NUBI_TRACE_FRAMES=1` before launching with
`--dev` and a dedicated `NUBI_DEV_DATA_DIR`. This logs monitor refresh, mouse input
events, selected tabs, and bounded animation-update gap samples. The samples are
event-loop measurements, not compositor presentation times or visual acceptance.
Keep the development notch separate from the installed notch during comparison.

Run `scripts/measure-native.ps1` after the dedicated release build. It launches
only its own development process, waits 60 seconds, then records ten private-byte,
working-set and process-CPU samples. `-Panel` measures the partial General panel
against the 120 MB limit. The script records the binary's SHA-256 and never selects
processes for termination by name. Raw output lives under the ignored target
directory.

Validation and measured results are recorded in the final worker report and the
repository work log. The workspace session-close test must remain present even
when the sandbox denies its child-process termination.
