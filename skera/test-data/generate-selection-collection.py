"""Regenerate a small collection with visibly different faces."""

from copy import deepcopy
from pathlib import Path
from fontTools.ttLib import TTCollection

root = Path(__file__).parents[2]
collection = TTCollection(root / "font-test-data/test_data/ttc/TTC.ttc")
collection.fonts[1] = deepcopy(collection.fonts[1])
font = collection.fonts[1]
font.recalcTimestamp = False
font["head"].fontRevision = 2
for record in font["name"].names:
    if record.nameID in (1, 4):
        record.string = "Second face".encode(record.getEncoding())
name = font.getGlyphOrder()[1]
width, bearing = font["hmtx"][name]
font["hmtx"][name] = (width + 37, bearing)
collection.fonts[0].recalcTimestamp = False
collection.save(Path(__file__).parent / "fonts/selection-collection.ttc")
