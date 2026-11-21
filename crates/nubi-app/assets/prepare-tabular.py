"""Bake Geist's existing tnum substitutions for Slint Text, which has no feature API.
Run from the repository root with fontTools already available. No outlines change.
The source Geist license remains in this assets directory.
"""
from pathlib import Path
from fontTools.ttLib import TTFont

assets = Path(__file__).resolve().parent
for weight in (400, 500, 600):
    font = TTFont(assets / f"Geist{weight}.ttf")
    substitutions = {}
    for record in font["GSUB"].table.FeatureList.FeatureRecord:
        if record.FeatureTag == "tnum":
            for index in record.Feature.LookupListIndex:
                for table in font["GSUB"].table.LookupList.Lookup[index].SubTable:
                    substitutions.update(table.mapping)
    assert len(substitutions) == 10
    for table in font["cmap"].tables:
        if table.isUnicode():
            table.cmap = {code: substitutions.get(glyph, glyph) for code, glyph in table.cmap.items()}
    for record in font["name"].names:
        if record.nameID in (1, 3, 4, 6, 16):
            value = record.toUnicode().replace("Geist", "Nubi Figures" if record.nameID != 6 else "NubiFigures")
            record.string = value.encode(record.getEncoding())
    font.save(assets / f"NubiFigures{weight}.ttf")
