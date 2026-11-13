"""Generate a small CFF2 font with nested defaults and variable deltas."""

from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.misc.psCharStrings import T2CharString


builder = FontBuilder(1000, isTTF=False)
glyphs = [".notdef", "linear", "quadratic", "rounded", "sum"]
builder.setupGlyphOrder(glyphs)
builder.setupCharacterMap(dict(zip(range(65, 69), glyphs[1:])))
builder.setupHorizontalMetrics({glyph: (500, 0) for glyph in glyphs})
builder.setupHorizontalHeader(ascent=800, descent=-200)
builder.setupNameTable(
    {
        "familyName": "Nested Blends Test",
        "styleName": "Regular",
        "uniqueFontIdentifier": "Nested Blends Test",
        "fullName": "Nested Blends Test",
        "psName": "NestedBlendsTest",
    }
)
builder.setupOS2(
    sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200
)
builder.setupPost()
builder.setupFvar([("wght", 0, 0, 1, "Weight"), ("wdth", 0, 0, 1, "Width")], [])
move = [0, 0, "rmoveto"]
finish = ["hlineto", 100, "vlineto", -100, "hlineto", -100, "vlineto"]
programs = {
    ".notdef": [],
    # 100 + 20*wght + 30*wdth: nested default.
    "linear": move + [100, 20, 0, 1, "blend", 0, 30, 1, "blend"] + finish,
    # 100 + (10 + 20*wght)*wdth: variable delta.
    "quadratic": move + [100, 0, 10, 20, 0, 1, "blend", 1, "blend"] + finish,
    # Two successive folds of wght: full instancing rounds each blend.
    "rounded": move + [0, 1, 0, 1, "blend", 1, 0, 1, "blend"] + finish,
    # (100 + 20*wght) + (10 + 40*wght)*wdth: sum of variable terms.
    "sum": move
    + [100, 20, 0, 1, "blend", 0, 10, 40, 0, 1, "blend", 1, "blend"]
    + finish,
}
builder.setupCFF2(
    {glyph: T2CharString(program=program) for glyph, program in programs.items()},
    regions=[{"wght": (0, 1, 1)}, {"wdth": (0, 1, 1)}],
)
builder.font.recalcTimestamp = False
builder.font["head"].created = builder.font["head"].modified = 2082844800
builder.save(Path(__file__).with_name("cff2-nested-blends.otf"))
