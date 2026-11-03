"""Generate rounding boundaries for tuple scalars and residual IUP deltas."""

from array import array
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import newTable
from fontTools.ttLib.tables.TupleVariation import TupleVariation

builder = FontBuilder(1000, isTTF=True)
builder.font.recalcTimestamp = False
builder.font["head"].created = builder.font["head"].modified = 2082844800
names = [".notdef", "scalar", "sparse", "rebase"]
builder.setupGlyphOrder(names)
builder.setupCharacterMap({65: "scalar", 66: "sparse"})
glyphs = {}
for name, points in zip(
    names,
    [
        [],
        [(0, 0), (10, 0), (0, 10)],
        [(-2920, 0), (-871, 10), (9361, 0)],
        [(0, 0), (10, 0), (0, 10)],
    ],
):
    pen = TTGlyphPen(None)
    if points:
        pen.moveTo(points[0])
        for point in points[1:]:
            pen.lineTo(point)
        pen.closePath()
    glyphs[name] = pen.glyph()
builder.setupGlyf(glyphs)
builder.setupHorizontalMetrics(
    {name: (500, -1000 if name == "sparse" else 0) for name in names}
)
builder.setupHorizontalHeader(ascent=800, descent=-200)
builder.setupNameTable({"familyName": "Tuple precision", "styleName": "Regular"})
builder.setupOS2()
builder.setupPost()
axes = ["AXA1", "AXB2", "AXC3", "AXD4"]
builder.setupFvar([(tag, 0, 0, 1, tag) for tag in axes], [])
tent = {
    tag: (0, peak / 16384, peak / 16384) for tag, peak in zip(axes, [15656, 10041, 998])
}
gvar = newTable("gvar")
gvar.version = 1
gvar.reserved = 0
gvar.variations = {
    ".notdef": [],
    "scalar": [TupleVariation(tent, [(28443, 0)] * 3 + [(0, 0)] * 4)],
    "rebase": [
        TupleVariation(
            {"AXA1": (0.18243408203125, 0.64068603515625, 0.76654052734375)},
            [(1000, 0)] * 3 + [(0, 0)] * 4,
        )
    ],
    "sparse": [
        TupleVariation(
            {"AXD4": (0, 1, 1)},
            [(2878, 0), None, (3822, 0)] + [(0, 0)] * 4,
        )
    ],
}
builder.font["gvar"] = gvar
cvt = newTable("cvt ")
cvt.values = array("h", [0])
builder.font["cvt "] = cvt
cvar = newTable("cvar")
cvar.version = 1
cvar.variations = [TupleVariation(tent, [28443])]
builder.font["cvar"] = cvar
builder.save(Path(__file__).with_name("tt-tuple-precision.ttf"))
