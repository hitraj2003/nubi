# Native software font rasterization

This directory contains the crates.io `i-slint-renderer-software` 1.18.1 source,
from Slint commit `372cf0ee5577c3dfec309a45e7b778ba4e81b734`. The original
license notices are retained in every source file and in `LICENSES/`.
The source archive SHA-256 is
`2f8c641e138a2cc96537cac4481f5c7c8bb8dcc66d09acaed269329c8738628a`.

Nubi enables Swash outline hinting in `fonts/vectorfont.rs`. Upstream leaves
hinting disabled. At the notch's 11–13 logical pixel text sizes, the unhinted
outlines spread stems across adjacent physical pixels. Hinting fits the glyphs
to the pixel grid without substituting fonts or changing the text layout.
Quarter-pixel horizontal positioning and variable font coordinates remain intact.

The workspace patch is required for this change to be reproducible. It changes
the native Slint software renderer; it does not change the Tauri/WebView app.
No Windows font is copied or redistributed. The UI continues using the installed
Segoe UI Variable Text family, bundled static Geist faces for figures, and bundled
JetBrains Mono for code and paths, matching `ui/theme.css`.

Validate renderer upgrades with the native typography render test at 100%, 125%,
150%, and 175% DPI in both themes. Those images measure raster output, not visual
parity with DirectWrite or the installed application. Remove the patch if a
future upstream renderer provides an equivalent supported setting.
