# Slint toolkit spike results

## H10 toolkit gate — 2026-11-06

Branch: `native/slint`. The owner selected Slint 1.18.1, its software renderer,
and the royalty-free license with the actual `AboutSlint` widget in About.
This supersedes the historical renderer-selection statements below. The
standalone `toolkit-gate` executable contains fake data only. It opens an approval
card, a representative panel, or About beside the original closed notch. No
production backend, settings, IPC, or root `ui/` code is changed. No commit was
created.

**Gate status: incomplete.** Implementation and automated checks pass. Manual
foreground focus, IME, Narrator, desktop composition, and high-contrast review
remain open. The standalone executable also exceeds H1's 10 MB installed-size
budget before adding a production backend. This is not a full panel port or an
approval to replace the production application.

### Gate table

“Pass” identifies the evidence actually collected. Synthetic input and UI
Automation do not establish physical keyboard behavior or Narrator speech.

| H10 item | Result | Evidence and limits |
| --- | --- | --- |
| Approval summary and command | Pass | Real compiled `ApprovalWindow` renders the fake provider/session, summary, and read-only command in both themes. |
| All parameters disclosure | Pass | Enter toggles the actual control in the UI test. Native UI Automation reports `Collapsed then Expanded`; parameters include working directory, timeout, and reason. |
| Allow / Deny | Pass | Compiled UI key events produce the fake `allow` and `deny` callbacks; native UI Automation invokes Deny and hides the card. No real permission is answered. |
| Approval Tab order, reverse order, Enter/Escape | Pass | UI test asserts Command, All parameters, Copy command, Allow, Deny and wrap in both directions. Initial logical focus is Deny; Enter activates the focused button; Escape denies. |
| Deliberate native approval focus and restoration | Untested | One explicit `focus_window()` request after native creation; logical focus and restore paths implemented. Every native sample returns `GetForegroundWindow() == 0` on the isolated desktop, so actual foreground acquisition/restoration cannot be established. |
| Notch remains no-activate | Pass | Every sampled notch is topmost with `WS_EX_NOACTIVATE`; synchronous `WM_MOUSEACTIVATE` returns `3` (`MA_NOACTIVATE`). The native test checks that style rewrites preserve no-activate. Physical focus behavior remains untested. |
| Command Copy button | Pass | UI test checks clipboard contents. Native UI Automation invokes Copy command; Windows clipboard equals `cargo test --offline`. |
| Left navigation rail | Pass | Inbox, About, approval launcher, and theme button render and traverse in declaration order. Navigation preserves the device-name draft; hidden Inbox controls leave About's Tab order. |
| Scrolling list | Pass | Twenty-four fake rows; compiled UI test verifies End/Home and actual mouse-wheel events. Arrow and Page Up/Down handlers share the viewport bounds. A noninteractive position indicator avoids an unnamed scrollbar control. |
| Editable text | Pass | Actual TextInput receives `東京 · Native` through key events. Native UI Automation exposes Device name as an editable value. This does not prove IME composition. |
| Selectable read-only text | Pass | UI test selects `C:` with Shift+Right, copies it, then selects/copies the full path and verifies typing cannot replace it. Native UI Automation reports Session path as `ControlType.Edit`, read-only `true`. |
| One unsaved setting and Save/Discard | Pass | Device-name draft survives navigation. Keyboard activates Save and Discard; Escape also discards. The bar disappears and focus returns to the editor. Changes stay in memory. Date/time/timeout are independent control demonstrations. |
| Tooltip | Pass | Timeout help appears on keyboard focus; rendered in the light/dark panel review images. Hover uses the same text and visibility binding. Physical pointer behavior remains untested. |
| Date/time control | Pass | Typed Gregorian `YYYY-MM-DD` and 24-hour `HH:MM` fields validate through real edit events. Tests cover leap years, invalid days/months, hour/minute limits, malformed and Unicode input. Calendar/clock popups are not implemented. |
| Stepper | Pass | Bounds are 3–95 seconds. Actual Up/Down events exercise both limits. Named decrease/increase buttons and accessibility actions use the same bounds; UI Automation exposes spinner value `30` and RangeValuePattern. |
| Panel keyboard traversal | Pass | Real Tab events assert the rail, list, editable/read-only text, date/time, stepper, its buttons, tooltip, and conditional draft actions. About's badge is keyboard accessible. Container scopes are excluded from traversal. |
| Focus rings | Pass | Action, field, list, stepper, and About wrapper draw token-based rings. Software review images show focused command/help controls. Desktop focus visibility and high contrast remain untested. |
| AboutSlint and royalty-free attribution | Pass | About instantiates the standard Slint 1.18.1 `AboutSlint`, not a copied badge. Offscreen review verifies the widget, version, and URL in both themes. Native UI Automation exposes the named badge button and `#MadeWithSlint` image. Browser launching is not exercised. |
| Shared light/dark theme tokens | Pass | All new surface colors reference the shared Theme global, extended from root `ui/theme.css`. The panel changes all three surfaces; palette initialization is explicit before showing About. A render assertion prevents invisible AboutSlint version/URL text in either theme. |
| Accessible roles and names | Pass | Every new gate control declares a role/name; TextInput owns field semantics; Action and the badge wrapper expose default actions. Native UI Automation trees confirm named buttons, editable/read-only values, spinner, and badge. Decorative children are not independent controls. |
| Narrator | Untested | No Narrator speech was captured. Expected semantic announcements are listed below; actual wording, reading order, live regions, expanded state, and focus need manual verification. |
| IME | Untested | UI and README include a Japanese/Chinese composition checklist. Unicode key insertion is tested; preedit, candidates, commit/cancel, and caret placement require a real Windows IME. |
| Panel private bytes, hard limit 120 MB | Pass | Final software-rendered native samples below include the passive notch and hidden counterpart component. All sampled maxima are below 120,000,000 bytes. |
| Idle CPU, hard limit 0.5% machine CPU | Pass | Ten-second process CPU-time deltas below, normalized over 16 logical processors. One-core percentages are reported separately. |
| Approval private bytes | Pass | Exact native software-rendered counters below; includes the passive notch and the allocated hidden panel component. |
| Executable size against H1 installed budget | Fail | Exact final size below. The spike executable alone exceeds 10,000,000 bytes. No installer or production release was packaged; no download-size result is claimed. |
| Software renderer | Pass | Default Cargo feature is software only; `toolkit-gate` explicitly selects winit/software. No runtime renderer environment fallback is used. FemtoVG is optional for historical notch comparisons. |
| Repeated surface lifetime | Pass | Three native UI Automation open/deny cycles and before/after private bytes are recorded below. This is a short, bounded smoke check; long-run retention, destruction/recreation, and production draft/deadline preservation remain untested. |
| Desktop scaling, monitor transitions, high contrast | Untested | Original notch offscreen 100/125/150% checks pass. New panel/card review images are 100%; physical scaling and Windows high contrast are not validated. |
| Open-time parity with production | Untested | No comparable production open-time baseline is available in this task. |
| Remaining production H10/H11 amendments | Untested | Versioned activation/security, complete behavior parity, cancellation/cache limits, native upgrades/download recovery, and DeskTime's on-demand WebView2 lifecycle are outside this fake-data toolkit spike. H11 remains authoritative for DeskTime; no production feature is removed. |

### Final native measurements

Run `cargo build --release --offline` first, then `./measure-h10.ps1
-InspectAccessibility`. Each process warms up for three seconds, then samples
private bytes/working set every nominal 100 ms over ten seconds. CPU is process
CPU-time delta divided by actual elapsed time and 16 logical processors.
One-core CPU omits the processor divisor. No compilation or UI Automation action
runs during an accepted sample. The spike starts no child process. Each process
includes the visible passive notch and one interactive surface; both counterpart
components are allocated, but the hidden one has not been shown. Native windows
report 96 DPI. MB means 1,000,000 bytes.

<!-- H10-MEASUREMENTS -->
| Surface | Theme | Private bytes (mean = min = max) | Machine CPU | One-core CPU |
| --- | --- | ---: | ---: | ---: |
| panel | dark | 11,657,216 | 0.038837% | 0.621395% |
| panel | light | 14,897,152 | 0.019510% | 0.312161% |
| approval | dark | 12,615,680 | 0.009759% | 0.156141% |
| approval | light | 12,734,464 | 0.000000% | 0.000000% |

| Surface | Theme | Sample seconds | Samples | Largest gap, seconds | Process CPU seconds |
| --- | --- | ---: | ---: | ---: | ---: |
| panel | dark | 10.0580122 | 90 | 0.1253270 | 0.062500 |
| panel | light | 10.0108644 | 89 | 0.1268883 | 0.031250 |
| approval | dark | 10.0070119 | 88 | 0.1267588 | 0.015625 |
| approval | light | 10.0020223 | 89 | 0.1277459 | 0.000000 |

Final release executable: **11,924,480 bytes (11.924480 MB)**. This exceeds the
10,000,000-byte H1 installed-size budget by 1,924,480 bytes before packaging or
adding a production backend. The independent notch executable is 10,936,320 bytes.

Toolkit SHA-256: `A66E6F5AF4A7B1908BC20CA0594325C2E1A89537ABE73BFE4131AC0706047650`.

Accepted final samples were recorded at 10:26:11–10:27:15 UTC on 2026-11-06.
Windows reports Microsoft Windows NT 10.0.26300.0. Every captured foreground
handle is zero; all interactive windows lack the no-activate style, while the
notch retains it. Native stderr is empty.

The final native UI Automation exercise completes **3 open/deny cycles**.
Private bytes before the exercise: **12,464,128**; after: **16,646,144**; delta: **4,182,016**.
The first show creates and renders the previously hidden card, so this short
increase includes surface creation and accessibility initialization. It does not
prove long-run retention or release of native buffers. The ending value remains
below the 120 MB limit. Native command clipboard contents match, and the
disclosure reports `Collapsed then Expanded`.
<!-- /H10-MEASUREMENTS -->

The script rejects samples longer than 11 seconds or gaps above 500 ms. Private
bytes were constant within each accepted sample. These are short observations,
not a long-run peak or a performance ranking between themes. Windows process CPU
accounting has 15.625 ms increments. GPU allocations and desktop composition are
not measured. Zero stderr and visible native handles do not prove rendered
desktop pixels.

Raw CSV samples, counter JSON, UI Automation trees and native exercise results
are under the ignored `measurements/h10-*` paths. Software render review images
are under ignored `target/h10/{panel,approval,about}-{light,dark}.bmp`. Scripts
and tests regenerate these artifacts. The native exercise uses UI Automation
InvokePattern and ExpandCollapsePattern, rather than physical input. An early
probe incorrectly assumed every button used InvokePattern; the disclosure
correctly exposes ExpandCollapsePattern. The corrected probe checks the actual
supported pattern. UI Automation is slow on this desktop, so the exercise runs
after sampling and has its own bounded process lifetime.

The optional native About-dark tree was captured before the final palette fix;
it verifies the unchanged widget structure, not final resource counters. Both
final About themes are validated by the compiled render test; the final native
About-light timed smoke exits with code 0 and empty stderr.

### Expected Narrator output — untested

These describe expected names, roles, values, and states from source semantics
and the inspected UI Automation tree. They are not a transcript or a guarantee
of exact Narrator wording. Slint 1.18.1 has no `dialog` AccessibleRole; the
approval component uses a named group inside its native window. Narrator's
window/group announcement and the trapped interaction need explicit review.

| Control | Expected semantic announcement |
| --- | --- |
| Approval window | “Approval request from Claude Code”; summary and command; initial focus on Deny |
| Command | “Command”, edit, read only, `cargo test --offline` |
| Disclosure | “All parameters”, button, collapsed or expanded |
| Copy / Allow / Deny | “Copy command”, “Allow”, or “Deny”, button |
| Inbox / About | Corresponding name, button, checked for the current page |
| Approval launcher / theme | “Approval card” or “Light theme”/“Dark theme”, button |
| Session list / rows | “Recent sessions, 24 entries”; each visible session's number, provider, and state |
| Device name | “Device name”, edit, current draft value |
| Session path | “Session path”, edit, read only, full path |
| Date / time | “Holiday date” or “End of day time”, edit, value; format and validation description |
| Stepper | “Move to terminal after, seconds”, spinner, current value, minimum 3, maximum 95 |
| Stepper buttons | “Decrease timeout” / “Increase timeout”, button |
| Tooltip | “Timeout help”, button; explanation available on focus |
| Unsaved bar | “Unsaved changes”; “Discard” and “Save”, button; saved/discarded status afterward |
| About widget | “Made with Slint, version 1.18.1. Open slint.dev”, button; `#MadeWithSlint` image; version and URL |

The captured UI Automation `HelpText` fields are empty even where source
`accessible-description` is set. Description reading and live-region speech
therefore remain unverified. The tree also includes nonfocusable Custom nodes
for some decorative children; Narrator must be checked for redundant reading.

### Checks

All commands run from `spikes/notch-slint` using the local cache. No missing
crate or network fetch was encountered.

| Check | Result |
| --- | --- |
| `cargo build --release --offline` | Pass; software-only default, both standalone executables built |
| `cargo clippy --offline -- -D warnings` | Pass |
| `cargo clippy --offline --tests -- -D warnings` | Pass |
| `cargo test --offline` | Pass; nine tests across the two executables, including actual compiled UI input/rendering and native boundary checks |
| `cargo fmt --check` | Pass |
| `node compare-motion.cjs` | Pass; 240 frames, maximum absolute error `5.204170427930421e-18` |
| PowerShell measurement script parse | Pass |
| Final native `--surface=about --theme=light --exit-after=2` | Pass; exit code 0, empty stderr |
| `git diff --check` | Pass |
| Production `src-tauri/` and root `ui/` diff | Empty |

## Historical stage 1 notch results

## Layout repair — 2026-11-06

The owner's displaced-content screenshot reproduces in an offscreen render at
100% scale. The content Rectangle had an explicit height but no `y` binding.
Slint 1.18.1 centers such items in their parent, so the opened clip and its
children started at `(700 - 188) / 2 = 256` logical pixels. The Path explicitly
started at `y: 0px`. The header therefore appeared around y=268 and the first
row around y=308, below the 188-pixel surface. The clip was moving with the
content; it was not clipping against the outline's coordinates. This occurs on
the first full frame, without partial redraws or native window resizing.

The content clip now binds `y` to `outline.y`. Both the closed and opened content
containers explicitly use `y: 0px`, including during intermediate spring heights.
Slint lengths and Path coordinates multiplied by `1px` both use logical pixels.
The existing Win32 placement multiplies the backing dimensions by the monitor
scale exactly once; hit testing divides physical cursor offsets by that scale.
No placement or spring changes were needed.

| Scale | Backing window, physical pixels | Open surface depth | Content origin y |
| --- | --- | --- | --- |
| 100% | 720 × 700 | 188 | 0 |
| 125% | 900 × 875 | 235 | 0 |
| 150% | 1080 × 1050 | 282 | 0 |

The default build now includes femtovg and software, with femtovg still selected
when no arguments are supplied. Previously, `--renderer=software` immediately
returned `renderer unavailable in this build: software` because the default
features only enabled femtovg. Explicit single-renderer builds remain supported.
Middle-button release over the notch now requests exit through the existing
16 ms UI timer without activating the window.

The new offscreen test renders the actual compiled Slint component at all three
scales, checking header and row positions, the closed ring, and zero alpha below
the current outline. It reuses the framebuffer through open, closed, and
100-pixel intermediate heights in both directions to check clipping and stale
pixels. The test failed on the original markup with content below the outline,
then passed after anchoring. BMP review images are generated under
`target/layout/`, composited over gray. They validate Slint rendering, not Windows
desktop composition or physical multi-monitor transitions.

Validation for this repair:

| Check | Result |
| --- | --- |
| `cargo build --release --offline` | Pass; default executable includes both renderers |
| `cargo clippy --offline -- -D warnings` | Pass |
| `cargo clippy --offline --tests -- -D warnings` | Pass, including the new render test |
| `cargo test --offline` | Pass: five tests, including real UI rendering at all three scales and native middle-button handling |
| `cargo fmt --check` and `git diff --check` | Pass |
| `node compare-motion.cjs` | Pass: 240 frames; maximum absolute error `5.204170427930421e-18` |
| Default executable, no arguments | Native window ready; middle-button release exits with code 0; empty stderr |
| Default executable, `--renderer=software` | Native window ready; middle-button release exits with code 0; empty stderr |
| Default executable, `--state=open` | Native window ready; middle-button release exits with code 0; empty stderr |
| Default executable, `--renderer=software --state=open` | Native window ready; middle-button release exits with code 0; empty stderr |

All four native smoke runs report 720 × 700 physical pixels at 96 DPI, topmost,
and no-activate styles. Middle-button release was posted to the native window;
physical input and desktop composition still need owner review. The 125% and
150% checks use offscreen scale-factor events, not changes to the owner's display
settings. No claim of physical monitor-transition validation is made. The owner
reported smooth motion before this repair; the springs, cadence, and dwell times
are unchanged. No repaint workaround was added.

Only files inside `spikes/notch-slint` were changed. Production `src-tauri/` and
root `ui/` were not edited. No commit was created. Historical measurements below
describe the earlier binaries and were not rerun for this layout repair. Their
sizes and hashes do not describe the repaired default executable (11,672,064
bytes).

## Earlier stage 1 results (before the layout repair)

Date: 2026-11-06. Branch: `native/slint`. Reference source revision:
`538a8b774605fe579fd141c12f0a72d0c0ce7a69`.

**Status: build and idle measurements complete; visual approval pending.** Both
renderer features compile offline, pass strict clippy, and run as native Windows
processes. The four validated samples below replace the earlier blocked results.
No renderer selection or visual parity approval is implied by these measurements.

## Requested measurements

All four scenarios request a visible, settled notch with static data, dark theme,
three inactive/done session rows, and no IPC. Each process has a visible native
window handle with topmost and no-activate styles; desktop capture is unavailable
in this sandbox, so displayed pixels and compositing remain unverified. Sampling
uses 3 s warmup, then 10 s of CPU-time deltas and nominal 100 ms memory samples.
All four native windows report 96 DPI.

| Renderer feature | State | Release executable bytes | Mean private bytes | Machine CPU | One-core CPU |
| --- | --- | ---: | ---: | ---: | ---: |
| `femtovg` | closed | 10,656,768 | 107,327,488 | 0.029264% | 0.468223% |
| `femtovg` | open | 10,656,768 | 108,085,248 | 0.029119% | 0.465897% |
| `software` | closed | 10,903,552 | 7,864,320 | 0.038712% | 0.619393% |
| `software` | open | 10,903,552 | 8,232,960 | 0.029170% | 0.466727% |

| Renderer feature | State | Actual sample seconds | Memory samples | Largest sample gap | Process CPU seconds |
| --- | --- | ---: | ---: | ---: | ---: |
| `femtovg` | closed | 10.0112635 | 84 | 0.1386338 s | 0.046875 |
| `femtovg` | open | 10.0612317 | 86 | 0.1278109 s | 0.046875 |
| `software` | closed | 10.0905255 | 88 | 0.1383924 s | 0.062500 |
| `software` | open | 10.0433359 | 84 | 0.1267711 s | 0.046875 |

Private bytes were constant throughout each accepted sample (minimum and maximum
equal its reported mean). Samples were recorded at 05:54:11–05:54:56 UTC on
2026-11-06. CPU values are single-run observations with 15.625 ms accounting
increments; their small differences do not establish a renderer performance rank.
Initial femtovg samples had long timing gaps and were discarded. The script now
rejects samples longer than 11 s or with a gap above 500 ms. All reported runs
pass these checks; no compilation ran concurrently with the accepted samples.

Release executable SHA-256:

- `femtovg`: `FD46CE78873F1C04EDEFE34EE3EB4BB4B10C24E5F2418F7AA21E6BABE2563848`
- `software`: `A657A73B0129D8999F22DA375585447D387015989D28368B3F671D60177AC5F4`

`./measure.ps1` builds each renderer offline using the shared dependency cache,
runs strict release clippy, saves an executable snapshot under
`target/<renderer>/release/`, checks native window styles, and takes these four
samples. `-Theme light` repeats in light mode. It stores exact bytes,
SHA-256, raw memory/CPU samples, means, ranges, elapsed time, processor count and
DPI in `measurements/`. CPU is `100 × process CPU seconds / elapsed seconds /
logical processors`; one-core CPU is also reported. MB means 1,000,000 bytes;
MiB means 1,048,576 bytes. The script does not measure GPU memory. There are no
child processes in the spike.

## Environment and compiler fixes

- Windows reports `Microsoft Windows NT 10.0.26300.0`; 16 logical processors.
- CPU: AMD Ryzen 7 6800H with Radeon Graphics, as reported by the Windows registry.
- `rustc 1.99.1 (58cf8f9c3 2026-10-13)`; `cargo 1.99.1 (5f94df478 2026-08-27)`.
- The coordinator populated Cargo's cache. All builds and lint checks in this
  continuation use `--offline`; the generated `Cargo.lock` is retained.
- Removed the extra semicolon after `export { Theme }` in the spike's
  `ui/notch.slint`. This is valid Slint export syntax without a semicolon.
- Removed the redundant `String::into()` in `BackendSelector::renderer_name`,
  resolving clippy's `useless_conversion` error under `-D warnings`.
- Desktop capture through `System.Drawing.Graphics.CopyFromScreen` fails with
  `The handle is invalid`. Native process measurements are available; screenshots
  and interactive desktop review are not.
- No git commit or push was attempted; the coordinator owns the commit.

## Implemented scope

The new crate is independent of `src-tauri` and `ui`; neither production tree
was edited. Slint and slint-build are pinned to 1.18.1. Only the winit backend,
chosen renderer, accessibility, raw handle access and required compatibility
feature are enabled. No Qt, Skia, Tauri, WebView, runtime interpreter, pipe,
production settings, hooks, or tray services participate.

- Fixed 720 × 700 logical-pixel backing window; top-center placement uses the
  whole current monitor, with per-monitor-v2 DPI and DPI/display change handling.
- Transparent background and topmost window request; Win32 no-activate/toolwindow
  styles, `WM_MOUSEACTIVATE=MA_NOACTIVATE`, and style preservation after rewrites.
  Slint content stays transparent until the native window is initialized.
- Four shape springs drive width, depth, bottom corners and ears. The outline
  uses today's `k=0.62` cubic control points. Closed size is 180 × 44; open size
  is 620 × 188 for three 40-pixel rows and today's header/padding.
- Open response/damping are 0.5/0.75; close 0.42/0.9. The engine retains velocity
  on retarget, caps elapsed time at 64 ms, integrates with 4 ms substeps, and uses
  the same 0.01 velocity/distance settling thresholds as `ui/motion.js`.
- Same 16 ms pointer polling, 40 ms hover-entry dwell and 140 ms exit dwell.
  Clicking the pill opens the card. Settled springs do not invalidate UI properties.
- Native hit tests use the current animated cubic outline sampled at 24 points
  per curve. Outside it, `HTTRANSPARENT` plus toggled layered/cursor-transparent
  styles request click-through across applications. This boundary is stricter
  than today's expanded rectangular hot area.
- Light/dark notch roles come from `ui/theme.css`. Segoe UI Variable Text remains
  the body font; embedded Geist supplies figures as in today's theme. The TTF
  asset is 65,532 bytes, converted from the existing WOFF2; its OFL notice is kept.
- `--state=open|closed`, `--theme=light|dark`, `--renderer=femtovg|software`,
  `--reduce-motion`, and `--exit-after=SECONDS` support isolated comparison.

## Checks completed

| Check | Result |
| --- | --- |
| `cargo build --release --offline` | Pass |
| `cargo build --release --no-default-features --features femtovg --offline` | Pass |
| `cargo build --release --no-default-features --features software --offline` | Pass |
| `cargo clippy --offline -- -D warnings` | Pass |
| `cargo clippy --release --no-default-features --features femtovg --offline -- -D warnings` | Pass |
| `cargo clippy --release --no-default-features --features software --offline -- -D warnings` | Pass |
| `./measure.ps1 -SkipBuild -Renderer femtovg` | Pass: closed/open, validated 10 s samples |
| `./measure.ps1 -SkipBuild -Renderer software` | Pass: closed/open, validated 10 s samples |
| Both renderer snapshots with `--state=open --exit-after=2` | Pass: exit code 0; empty stderr |
| `cargo fmt --check` | Pass |
| `clippy-driver --edition 2024 --test src/core_tests.rs -o checks/core-clippy.exe -D warnings` | Pass; independent modules only |
| `./checks/core-clippy.exe --test-threads=1` | Pass: 4 tests |
| `node compare-motion.cjs` | Pass: 240 frames against the original `ui/motion.js` |
| PowerShell parser on `measure.ps1` | Pass |

The four tests cover velocity-preserving reversal and settling, late-frame
capping, curved hit-region geometry, and a real hidden Win32 test window's
topmost/no-activate setup, `WM_MOUSEACTIVATE`, style rewriting, and hit-test
responses. They do not instantiate Slint. The JavaScript comparison includes
8/16/33/100 ms frames and three retargets; maximum absolute position/velocity
error is `5.204170427930421e-18` against the original engine.

## Remaining parity and runtime checks

The Slint markup and application glue compile for both renderer features. Native
window creation, 96 DPI, topmost/no-activate styles and four settled process
measurements pass. Windows transparency/compositing interaction with layered
styles, foreground focus preservation during real clicks, actual cross-process
click-through, monitor/DPI transitions, text rendering and frame pacing remain
unverified. No screenshot or smoothness claim is made. GPU allocations are not
included in private bytes, so these numbers are not total graphics memory costs.

This slice includes only static sessions. Tabs, provider ring percentages, moon
and panel icons are visual fixtures; session actions, usage/DeskTime, drag/fold,
live countdowns and working/waiting activity loops are outside this spike. The
compact ring strokes are simplified. Today's content blur/scale/crossfade and
row reveal animations are not yet ported; the shape springs are ported. These
differences must be resolved or explicitly reviewed before claiming visual
parity. The `software` feature is Slint's software renderer, not Skia software.
Neither renderer is recommended until the owner's side-by-side review.

## Slint license route for the MIT project

Slint 1.18.1 offers GPLv3, royalty-free 2.0 and commercial alternatives. For this
desktop application, the royalty-free 2.0 route permits application integration
and distribution without royalty payments or applying GPL to the MIT application
source. Slint itself retains its separate license and notices.

That route requires either the prescribed AboutSlint widget in an accessible
About screen/dialog (or splash in its absence), or an easily found official
Slint badge on a public application webpage, preferably the download page.
The README links the badge; a private README alone is insufficient. Complete
one route before binary distribution. The license excludes embedded systems,
standalone toolkit distribution and applications exposing Slint's APIs. Preserve
notices and audit transitive dependency notices before packaging. These terms fit
an MIT desktop application if the attribution requirement is accepted; they do
not make Slint an MIT dependency. Sources: [pinned license overview](https://github.com/slint-ui/slint/blob/v1.18.1/LICENSE.md),
[royalty-free 2.0 terms](https://github.com/slint-ui/slint/blob/v1.18.1/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md).
