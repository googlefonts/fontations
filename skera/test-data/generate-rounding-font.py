"""Regenerate the near-half TrueType rounding regression and HB reference.

Run with hb-subset as the first argument; the reference includes the fix
from harfbuzz/harfbuzz#6351.
"""

from pathlib import Path
import subprocess
import sys

from fontTools.ttLib import TTFont
from fontTools.ttLib.tables.TupleVariation import TupleVariation


directory = Path(__file__).parent
font = TTFont(directory / "fonts/cubic-glyf-variable.ttf", recalcTimestamp=False)
name = font.getGlyphOrder()[2]
glyph = font["glyf"][name]
# Use quadratic outlines and put two points at x=0, where adding a near-half
# delta does not lose precision before the final rounding operation.
glyph.flags = [flag & ~0x80 for flag in glyph.flags]
for i, (x, y) in enumerate(glyph.coordinates):
    glyph.coordinates[i] = (x - 278, y)
font["hmtx"][name] = (font["hmtx"][name][0], 0)
font["gvar"].variations = {g: [] for g in font.getGlyphOrder()}
font["gvar"].variations[name] = [TupleVariation(
    {"TEST": (0, 1, 1), "AXIS": (0, 1, 1)}, [(8, 0)] * 6 + [(0, 0)] * 4)]
source = directory / "fonts/float-rounding-variable.ttf"
font.save(source)
# 8 * (4095/16384) * (4097/16384) is the f32 predecessor of 0.5.
output = directory / "expected/rounding/truetype.ttf"
subprocess.run(
    [sys.argv[1], str(source), "--keep-everything",
     "--instance=TEST=0.24993896484375,AXIS=0.25006103515625",
     f"--output-file={output}"],
    check=True,
)
