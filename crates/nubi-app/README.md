# Nubi native executable (stage 2)

This is an internal migration build. It runs the existing Tauri-free core with a
Slint software-rendered notch, native tray menu, configured global shortcuts,
Windows toast notifications and a signed updater. The Tauri application remains
in the workspace.

Build and run the native executable in a separate target directory:

```powershell
cargo build --release --offline -p nubi-app --target-dir src-tauri/target/native
$env:NUBI_DEV_DATA_DIR = "$PWD/src-tauri/target/native-dev"
./src-tauri/target/native/release/nubi.exe --dev
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
- Settings opens lazily. A hidden settings surface is released after 60 seconds;
  editable General values and navigation survive release. Launcher surfaces are
  released after cancellation. The panel is still the partial spike port.
- The updater reads its endpoint and minisign public key directly from the
  canonical Tauri configuration at compile time. It accepts the existing static
  and dynamic manifests and verifies the artifact and authenticated version
  comment. Responses are bounded, truncated downloads fail, and installer bytes
  are verified again while held against writes. NSIS retains passive update and
  restart arguments. Startup-only automatic installation, session gating,
  displayed-note acknowledgment, hover/focus holds, cancellation and error policy
  preserve the existing updater state machine. Development never runs an installer.

## Acceptance limits

The spike's [PARITY.md](../../spikes/notch-slint/PARITY.md) remains applicable.
This executable does not establish visual or interaction parity for later stages.

- Full notch request/alert presentation, all tabs and dynamic states, edge and
  monitor placement, dragging and complete keyboard behavior remain stage 3.
- The native Host preserves pending requests, but approval/question UI and backend
  reconciliation remain stage 4. Unanswered requests still fall back to terminals.
- The tray stores ring/taskbar selections, but their native surfaces remain
  stage 5. The current minimal notch remains the display surface.
- Launcher and update windows provide functional callbacks and keyboard actions;
  their exact source styling, motion, focus restoration and monitor placement
  still require the stage 5 parity pass.
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

Run `scripts/measure-native.ps1` after the dedicated release build. It launches
only its own development process, waits 60 seconds, then records ten private-byte,
working-set and process-CPU samples. `-Panel` measures the partial General panel
against the 120 MB limit. The script records the binary's SHA-256 and never selects
processes for termination by name. Raw output lives under the ignored target
directory.

Validation and measured results are recorded in the final worker report and the
repository work log. The workspace session-close test must remain present even
when the sandbox denies its child-process termination.
