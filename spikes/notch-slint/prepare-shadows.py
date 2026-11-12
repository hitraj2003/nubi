"""Rasterize the source CSS card shadows as reusable nine-slice images.

Slint 1.18.1's software renderer leaves draw_box_shadow unimplemented. These
procedural assets preserve software rendering and require no runtime blur work.
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

root = Path(__file__).resolve().parent
scale, padding, size = 3, 48, 65
extent = (size + 2 * padding) * scale
for dark in [False, True]:
    result = Image.new("RGBA", (extent, extent))
    layers = [(0, 10, 30, -12, (0, 0, 0, 179))] if dark else [
        (0, 8, 24, -10, (40, 30, 10, 36)),
        (0, 1, 2, 0, (40, 30, 10, 10)),
    ]
    for dx, dy, blur, spread, color in layers:
        mask = Image.new("L", (extent, extent))
        ImageDraw.Draw(mask).rounded_rectangle(
            ((padding + dx - spread) * scale, (padding + dy - spread) * scale,
             (padding + size + dx + spread) * scale - 1, (padding + size + dy + spread) * scale - 1),
            radius=max(0, 18 + spread) * scale, fill=color[3],
        )
        mask = mask.filter(ImageFilter.GaussianBlur(blur * scale / 2))
        layer = Image.new("RGBA", (extent, extent), color[:3] + (0,))
        layer.putalpha(mask)
        result = Image.alpha_composite(result, layer)
    result.save(root / f"assets/shadow-{'dark' if dark else 'light'}.png")
