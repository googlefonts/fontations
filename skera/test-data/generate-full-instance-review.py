"""Generate small fonts for full-instancing boundary regressions."""

from pathlib import Path

from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.fontBuilder import FontBuilder
from fontTools.misc.psCharStrings import T2CharString
from fontTools.varLib.builder import buildVarData, buildVarRegionList, buildVarStore


OUT = Path(__file__).parent / "fonts"


def build(programs, regions):
    builder = FontBuilder(1000, isTTF=False)
    builder.font.recalcBBoxes = False
    builder.font.recalcTimestamp = False
    builder.font["head"].created = builder.font["head"].modified = 2082844800
    names = list(programs)
    builder.setupGlyphOrder(names)
    builder.setupCharacterMap({65 + i: name for i, name in enumerate(names[1:])})
    builder.setupHorizontalMetrics({name: (500, 0) for name in names})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({"familyName": "CFF2 full instance review", "styleName": "Regular"})
    builder.setupOS2()
    builder.setupPost()
    builder.setupFvar([("wght", 0, 0, 1, "Weight")], [])
    builder.setupCFF2(
        {name: T2CharString(program=program) for name, program in programs.items()},
        regions=regions,
    )
    return builder


outline = [100, 0, 0, 100, -100, 0, "rlineto"]
builder = build(
    {
        ".notdef": [],
        "A": [0.5, 1, 0, 1, "blend", 0, "rmoveto"] + outline,
        "B": [0, 0, 1.5, 1, "blend", 0, "rmoveto"] + outline,
    },
    [{"wght": (0, 1, 1)}, {"wght": (0, 3 / 16384, 3 / 16384)}],
)
builder.save(OUT / "cff2_fractional_blends.otf")

builder = build(
    {".notdef": [], "A": [0, 0, "rmoveto"] + outline},
    [{"wght": (0, 1, 1)}],
)
store = buildVarStore(
    buildVarRegionList([{"wght": (0, 1, 1)}], ["wght"]),
    [buildVarData([0], [[-8192]], optimize=False)],
)
builder.font["CPAL"] = buildCPAL([[(1, 0, 0, 1)]])
builder.font["COLR"] = buildCOLR(
    {"A": dict(Format=10, Glyph="A", Paint=dict(Format=3, PaletteIndex=0, Alpha=1, VarIndexBase=0))},
    version=1,
    glyphMap=builder.font.getReverseGlyphMap(),
    varStore=store,
)
builder.save(OUT / "cff2_variable_color.otf")
