# Stage 1 Slint notch spike

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
