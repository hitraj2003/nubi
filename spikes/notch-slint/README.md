# Slint notch and H10 toolkit spike

This standalone Windows crate uses static session data. It does not start Tauri,
connect to Nubi's pipe, register hooks, or read or write production settings.
It reproduces the top-edge shape and ports the four shape springs from
`ui/island.js` and `ui/motion.js`. See [RESULTS.md](RESULTS.md) for validation limits.

Build from this directory on Windows with Rust and the MSVC build tools:

```powershell
cargo build --release --offline
cargo clippy --offline -- -D warnings
cargo run --release --offline --bin notch-slint -- --theme=dark
cargo run --release --offline --bin notch-slint -- --theme=light
cargo run --release --offline --bin notch-slint -- --renderer=software
cargo test --offline
cargo build --release --offline --no-default-features --features software
cargo clippy --release --offline --no-default-features --features software -- -D warnings
cargo run --release --offline --no-default-features --features software --bin notch-slint -- --renderer=software
```

## H10 approval and panel

The separate `toolkit-gate` executable always selects the software renderer and
keeps a closed, no-activate notch beside one interactive surface. It shares
`ui/theme.slint` with the original notch and has no production backend connections.

```powershell
cargo run --release --offline --bin toolkit-gate -- --surface=panel --theme=dark
cargo run --release --offline --bin toolkit-gate -- --surface=approval --theme=light
cargo run --release --offline --bin toolkit-gate -- --surface=about --theme=dark
./measure-h10.ps1 -InspectAccessibility
./measure-h10.ps1 -Theme dark -Surface panel -InspectAccessibility -ExerciseControls -CycleCount 3
```

Pass `--exit-after=30` for a timed review. The panel's Approval card button opens
the card and deliberately requests OS focus once its native window exists.
Command, All parameters, Copy command, Allow, and Deny form a trapped forward
and reverse Tab cycle. Initial focus is Deny. Enter/Space activates the focused
button; Escape denies. Closing the card also denies. The card returns focus to
the panel's invoking button, or the previous live foreground window when launched
alone. The notch never requests focus and retains `WS_EX_NOACTIVATE` and
`WM_MOUSEACTIVATE = MA_NOACTIVATE`.

The panel demonstrates a rail, 24 scrolling rows, a device-name setting with
in-memory Save/Discard, selectable read-only session path, validated Gregorian
date and 24-hour time fields, a bounded 3–95 second stepper, and a tooltip shown
on hover or keyboard focus. Date/time use typed fields; there is no calendar or
clock popup. Escape discards the device-name draft. Navigation preserves it.
Only the device-name setting participates in Save/Discard. Date, time, and
timeout are separate temporary control demonstrations. Theme changes reach all
three surfaces. About embeds the actual Slint 1.18.1 `AboutSlint` widget with a
keyboard-accessible wrapper; no imitation badge is used.

Panel Tab order is Inbox, About, Approval card, theme, session list, device name,
session path, date, time, stepper, decrease, increase, timeout help, then Discard
and Save when dirty. Hidden Inbox controls leave traversal on About. The list
supports mouse wheel, arrows, Page Up/Down, Home, and End. The stepper supports
Up/Down. Text fields use Slint TextInput selection and clipboard behavior.

`cargo test --offline` exercises real compiled controls through a software
platform: both Tab directions, approval actions, command copy, partial read-only
selection, Unicode insertion, date/time errors, draft retention and Save/Discard,
stepper bounds, scrolling, and both themes. It writes offscreen review images
to `target/h10/`. This does not establish native focus, Narrator speech, or IME
composition. `measure-h10.ps1` samples the native release executable after three
seconds of warmup for ten seconds and records exact counters, window styles,
foreground handle, hashes, and optional UI Automation trees in `measurements/`.
The optional control exercise opens and denies approval cards repeatedly through
UI Automation, checks the native clipboard and expanded state, and records
private bytes before and after. UI Automation calls can be slow on an isolated
desktop; they run after the idle sample and have a separate process lifetime.
No build runs inside the measurement script; build first, then sample on an idle
machine. Both the panel and approval measurements include the passive notch and
the allocated, hidden counterpart component. No child process is started by the
spike. The 120 MB limit means 120,000,000 private bytes.

Manual Windows checks still required:

1. Launch the approval from a focused editor and from the panel. Verify initial
   Deny focus, focus ring, Tab/Shift+Tab, Enter, Escape, close, focus restoration,
   and that clicking or hovering the notch never activates it.
2. With a Japanese or Chinese IME, compose in Device name. Verify preedit,
   candidate placement, commit, cancellation, caret, and saved text. Plain Unicode
   insertion in automated tests is not an IME test.
3. Run Narrator through each control in both themes. Check names, roles, expanded
   state, selected navigation, read-only state, stepper bounds, validation, and
   unsaved/saved announcements. Compare the expected utterances in RESULTS.md
   with actual speech; UI Automation trees alone do not establish Narrator output.
4. Review text and focus rings at 100%, 125%, and 150%, across monitors, and with
   Windows high contrast. The new surfaces' desktop scaling is not yet validated.

## Original notch

Hover for 40 ms to open; leave for 140 ms to close. Clicking the pill also opens it.
The fixed 720 × 700 logical-pixel native window stays centered at the monitor's top
edge. Its shape changes inside those bounds. Middle-click the notch to exit.
For timed reviews, pass `--exit-after=30`.

`--state=open` and `--state=closed` hold a settled state for measurements, ignoring
hover/click expansion. `--reduce-motion` removes shape motion; the Windows client
area animation setting is also respected. `--renderer=femtovg|software` rejects
renderers omitted from the build. The owner selected software: the default build
includes software only and prefers software when both features are enabled.
FemtoVG remains available for historical comparisons. Dark is the default; `--theme=light` selects the light notch
tokens before first paint.

`cargo test --offline` renders the compiled UI offscreen at 100%, 125%, and 150%
scale. It checks top anchoring, session positions, and clipping through open,
closed, and intermediate frames using a reused buffer. Review images are saved
under `target/layout/<scale>-<state>-<depth>.bmp`; transparency is composited over
gray. These images do not test Windows desktop composition.

Run the two-renderer release build, clippy, and four idle samples:

```powershell
./measure.ps1
./measure.ps1 -Theme light
```

Each scenario warms up for 3 s, then samples private bytes and working set every
100 ms for at least 10 s. CPU uses the process CPU-time delta divided by actual
elapsed time and logical processor count. Reports also include one-core CPU,
exact executable bytes, SHA-256, sample range, and raw CSV data. Renderers have
separate executable snapshots under `target/<renderer>/release/`, while Cargo
reuses the shared `target/` dependency cache. All Cargo commands run offline.
The spike spawns no child processes. GPU memory is not measured by this
script. Keep the windows visible and the machine otherwise idle.

When Cargo dependencies are unavailable, run the independent numerical and
native boundary tests (these do not validate Slint rendering):

```powershell
New-Item -ItemType Directory -Force checks | Out-Null
rustc --edition 2024 --test src/core_tests.rs -o checks/core-tests.exe
./checks/core-tests.exe --test-threads=1
node compare-motion.cjs
```

The Geist TTF is a format conversion of `ui/fonts/geist.woff2`, with its OFL notice
beside it. To regenerate, run `python prepare-font.py` with fontTools installed;
Node supplies Brotli decoding if the Python extension is absent. Build and run
do not need Python or Node. Provider glyphs are copied unchanged from `ui/glyphs`.

## Distribution terms

The spike's own source is MIT; Slint has separate terms. Read the license section
in [RESULTS.md](RESULTS.md) before binary distribution. Official attribution badge:

[![Made with Slint](https://raw.githubusercontent.com/slint-ui/slint/v1.18.1/logo/MadeWithSlint-logo-whitebg.png)](https://slint.dev)
