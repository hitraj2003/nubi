"""Prepare source-derived icons, fonts, colours and Slint fixture; no downloads."""
from pathlib import Path
import json
import re
import shutil
import runpy
from datetime import datetime

root = Path(__file__).resolve().parent
source = root.parent.parent / "ui"
runpy.run_path(str(root / "prepare-font.py"))
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

geist = TTFont(root / "assets/Geist.ttf")
for weight in [400, 500, 600]:
    instantiateVariableFont(geist, {"wght": weight}, inplace=False, updateFontNames=True).save(root / f"assets/Geist{weight}.ttf")

font = TTFont(source / "fonts/jetbrains-mono.woff2")
font.flavor = None
font.save(root / "assets/JetBrainsMono.ttf")
shutil.copyfile(source / "fonts/OFL-JetBrainsMono.txt", root / "assets/OFL-JetBrainsMono.txt")
shutil.copyfile(source / "glyphs/nubi-logo.png", root / "assets/nubi-logo.png")
html = (source / "panel.html").read_text(encoding="utf-8")
icons = {}
for name, body in re.findall(r'<button[^>]*data-(?:view|pane)="([^"]+)"[^>]*>(.*?)</button>', html, re.S):
    match = re.search(r"<svg.*?</svg>", body, re.S)
    if match:
        icons[name] = match[0]
island = (source / "island.html").read_text(encoding="utf-8")
for name, body in re.findall(r'<button[^>]*id="([^"]+)"[^>]*>(.*?)</button>', island, re.S):
    match = re.search(r"<svg.*?</svg>", body, re.S)
    if match:
        icons[name] = match[0]
for name, svg in icons.items():
    svg = svg.replace('<svg ', '<svg xmlns="http://www.w3.org/2000/svg" fill="none" stroke="#000" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" ', 1)
    (root / f"assets/icon-{name}.svg").write_text(svg, encoding="utf-8")
cases = " : ".join(f'name == "{name}" ? @image-url("../assets/icon-{name}.svg")' for name in icons)
(root / "ui/icons.slint").write_text('export component SourceIcon inherits Image {\n in property <string> name;\n width: 18px; height: 18px;\n source: '+cases+' : @image-url("../assets/moon.svg");\n}\n', encoding="utf-8")
fixture = json.loads((root / "fixture.json").read_text(encoding="utf-8"))
rows = []
for session in fixture["sessions"]:
    row = {key: session[key] for key in ["name", "provider"]}
    row.update(state=session["state"].title(), context=f'{session["context_tokens"]//1000}k', ago=f'{(fixture["now"]-session["last_event_ms"])//60000}m')
    rows.append("{" + ", ".join(f"{key}: {json.dumps(value, ensure_ascii=False)}" for key, value in row.items()) + "}")
colors = json.loads((root / "reference/usage-colors.json").read_text())
palette = [["#" + "".join(f"{channel:02x}" for channel in rgb) for rgb in pair] for pair in colors]
usage = fixture["usage"]
(root / "ui/fixture.slint").write_text('''// Generated from fixture.json and Chrome's evaluation of ui/usage-color.js.
export struct Session { name: string, provider: string, state: string, context: string, ago: string }
export global Fixture {
    out property <[Session]> sessions: [\n        ''' + ",\n        ".join(rows) + '''];
    out property <[float]> session-percent: [''' + ",".join(str(u["windows"][0]["used_percent"]) for u in usage) + '''];
    out property <[float]> week-percent: [''' + ",".join(str(u["windows"][1]["used_percent"]) for u in usage) + '''];
    out property <[string]> alert-times: [''' + ",".join(json.dumps(datetime.fromtimestamp(alert["ts"] / 1000).strftime("%I:%M %p").lstrip("0")) for alert in fixture["alerts"]) + '''];
    out property <[string]> alert-lines: [''' + ",".join(json.dumps(alert["title"] + " · " + alert["body"], ensure_ascii=False) for alert in fixture["alerts"]) + '''];
    out property <[color]> full-colors: [''' + ",".join(pair[0] for pair in palette) + '''];
    out property <[color]> soft-colors: [''' + ",".join(pair[1] for pair in palette) + '''];
    public pure function usage-color(percent: float, soft: bool) -> color {
        return soft ? soft-colors[round(clamp(percent,0,100))] : full-colors[round(clamp(percent,0,100))];
    }
}
''', encoding="utf-8")
print("Prepared source SVGs, both fonts, fixture and Chrome-resolved OKLCH palette.")
runpy.run_path(str(root / "prepare-overlay.py"))
