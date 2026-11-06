# Slint notch spike

This standalone Windows crate uses static session data. It does not start Tauri,
connect to Nubi's pipe, register hooks, or read or write production settings.
It reproduces the top-edge shape and ports the four shape springs from
`ui/island.js` and `ui/motion.js`. See [RESULTS.md](RESULTS.md) for validation limits.

Build from this directory on Windows with Rust and the MSVC build tools:

```powershell
cargo build --release --offline
cargo clippy --offline -- -D warnings
cargo run --release --offline -- --theme=dark
cargo run --release --offline -- --theme=light
cargo run --release --offline -- --renderer=software
cargo test --offline
cargo build --release --offline --no-default-features --features software
cargo clippy --release --offline --no-default-features --features software -- -D warnings
cargo run --release --offline --no-default-features --features software -- --renderer=software
```

Hover for 40 ms to open; leave for 140 ms to close. Clicking the pill also opens it.
The fixed 720 × 700 logical-pixel native window stays centered at the monitor's top
edge. Its shape changes inside those bounds. Middle-click the notch to exit.
For timed reviews, pass `--exit-after=30`.

`--state=open` and `--state=closed` hold a settled state for measurements, ignoring
hover/click expansion. `--reduce-motion` removes shape motion; the Windows client
area animation setting is also respected. `--renderer=femtovg|software` rejects
renderers omitted from the build. The default build includes both renderers and
starts with femtovg. Dark is the default; `--theme=light` selects the light notch
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
