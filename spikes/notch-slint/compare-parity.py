"""Convert the real software renders to PNG and place reference on the left."""
from pathlib import Path
import json
from PIL import Image, ImageChops, ImageStat

root = Path(__file__).resolve().parent
out = root / "compare"
out.mkdir(exist_ok=True)
measurements = []
overlay_names = [f"overlay-{state}-{theme}-{scale}" for state in
                 ["approval", "parameters", "expanded", "question", "multi", "custom"]
                 for theme in ["dark", "light"] for scale in [100, 150]]
for name in overlay_names:
    for path in [root / "target/parity" / (name + ".bmp"), root / "reference" / (name + ".png")]:
        if not path.exists():
            raise FileNotFoundError(f"Missing overlay parity evidence: {path}")
for bitmap in sorted((root / "target/parity").glob("*.bmp")):
    reference = root / "reference" / (bitmap.stem + ".png")
    if not reference.exists():
        continue
    actual = Image.open(bitmap).convert("RGB")
    expected = Image.open(reference).convert("RGB")
    if actual.size != expected.size:
        raise ValueError(f"Size mismatch: {bitmap.name}: {actual.size} vs {expected.size}")
    actual.save(out / (bitmap.stem + "-slint.png"))
    side = Image.new("RGB", (expected.width * 2, expected.height))
    side.paste(expected, (0, 0))
    side.paste(actual, (expected.width, 0))
    side.save(out / (bitmap.stem + ".png"))
    diff = ImageChops.difference(expected, actual)
    diff.save(out / (bitmap.stem + "-difference.png"))
    channels = ImageStat.Stat(diff).mean
    changed = sum(max(pixel) > 12 for pixel in diff.get_flattened_data())
    measurements.append({"name": bitmap.stem, "size": list(actual.size), "mean_absolute_rgb_error": sum(channels) / 3,
                         "pixels_over_12": changed, "fraction_over_12": changed / (actual.width * actual.height)})
    if bitmap.stem.startswith("notch"):
        scale = actual.width / 720
        region = (int(30*scale), 0, int(690*scale), int((52 if "closed" in bitmap.stem else 200)*scale))
    elif bitmap.stem.startswith("overlay"):
        scale = actual.width / 720
        region = (int(185*scale), int(8*scale), int(535*scale), int(520*scale))
    else:
        scale = actual.width / 1100
        region = (0, 0, actual.width, int((500 if "settings" in bitmap.stem else 300)*scale))
    crop = diff.crop(region)
    measurements[-1]["content_region"] = list(region)
    measurements[-1]["content_mean_absolute_rgb_error"] = sum(ImageStat.Stat(crop).mean) / 3
    measurements[-1]["content_fraction_over_12"] = sum(max(pixel)>12 for pixel in crop.get_flattened_data()) / (crop.width * crop.height)
(out / "metrics.json").write_text(json.dumps(measurements, indent=2))
print(f"Saved {len(measurements)} reference-left comparisons, Slint PNGs, difference images and metrics.")
