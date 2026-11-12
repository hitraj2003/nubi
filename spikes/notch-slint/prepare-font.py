"""Convert the repository's Geist WOFF2 to TTF without changing its glyphs.

Requires fontTools. Uses Node's built-in Brotli decoder when Python brotli is absent.
The resulting font is checked in; building the spike does not require Python/Node.
Run from this crate directory.
"""
from pathlib import Path
import subprocess
import sys
import types

try:
    import brotli  # noqa: F401
except ImportError:
    def decompress(data):
        return subprocess.run(
            ["node", "-e", "process.stdout.write(require('node:zlib').brotliDecompressSync(require('node:fs').readFileSync(0)))"],
            input=data, stdout=subprocess.PIPE, check=True,
        ).stdout

    sys.modules["brotli"] = types.SimpleNamespace(decompress=decompress)

from fontTools.ttLib import TTFont

source = Path("../../ui/fonts/geist.woff2")
font = TTFont(source)
font.flavor = None
font.save("assets/Geist.ttf")
