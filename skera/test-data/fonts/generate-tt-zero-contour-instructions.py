"""Generate a zero-contour glyph with phantom-point instructions and variations."""

from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.ttProgram import Program
from fontTools.ttLib.tables.TupleVariation import TupleVariation

f = FontBuilder(1000, isTTF=True)
f.font.recalcTimestamp = False
f.font["head"].created = f.font["head"].modified = 2082844800
names = [".notdef", "space"]
f.setupGlyphOrder(names)
f.setupCharacterMap({32: "space"})
glyphs = {n: TTGlyphPen(None).glyph() for n in names}
glyphs["space"].program = Program()
glyphs["space"].program.fromBytecode([0xB0, 0, 0x2F])
f.setupGlyf(glyphs)
f.setupHorizontalMetrics({n: (500, 0) for n in names})
f.setupHorizontalHeader(ascent=800, descent=-200)
f.setupNameTable({"familyName": "Zero contour instructions", "styleName": "Regular"})
f.setupOS2()
f.setupPost()
f.setupFvar([("wght", 0, 0, 1, "Weight")], [])
gvar = newTable("gvar")
gvar.version = 1
gvar.reserved = 0
gvar.variations = {
    ".notdef": [],
    "space": [TupleVariation({"wght": (0, 1, 1)}, [(0, 0), (100, 0), (0, 0), (0, 0)])],
}
f.font["gvar"] = gvar
f.font["maxp"].maxStackElements = 1
f.font["maxp"].maxSizeOfInstructions = 3
f.save(Path(__file__).with_name("tt-zero-contour-instructions.ttf"))

# Requires FontTools with zero-contour instruction roundtripping.
assert list(
    TTFont(Path(__file__).with_name("tt-zero-contour-instructions.ttf"))["glyf"][
        "space"
    ].program.getBytecode()
) == [0xB0, 0, 0x2F]
