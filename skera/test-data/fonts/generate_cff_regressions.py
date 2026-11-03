#!/usr/bin/env python3
"""Generate small synthetic CFF regression fonts.

Requires FontTools with the split Private DICT blend fixes from
https://github.com/fonttools/fonttools/pull/4346.
"""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.ttLib import TTFont
from fontTools.misc.psCharStrings import T2CharString
from fontTools.cffLib import SubrsIndex, FDSelect

OUT = Path(__file__).parent


def builder():
    f = FontBuilder(1000, isTTF=False)
    f.font.recalcBBoxes = False
    f.font.recalcTimestamp = False
    f.font["head"].created = f.font["head"].modified = 2082844800
    gs = [".notdef", "A", "B"]
    f.setupGlyphOrder(gs)
    f.setupCharacterMap({65: "A", 66: "B"})
    f.setupHorizontalMetrics({g: (500, 0) for g in gs})
    f.setupHorizontalHeader(ascent=800, descent=-200)
    f.setupNameTable({"familyName": "CFF regression", "styleName": "Regular"})
    f.setupOS2()
    f.setupPost()
    return f


for cff2 in [False, True]:
    for kind in ["caller-dependent", "shared-hints"]:
        if kind == "shared-hints" and cff2:
            continue
        f = builder()
        end = [] if cff2 else ["endchar"]
        ret = [] if cff2 else ["return"]
        # An unused first subroutine forces renumbering of caller-supplied IDs.
        if kind == "caller-dependent":
            progs = [
                ret,
                [10, 0, "rlineto"] + ret,
                [20, 0, "rlineto"] + ret,
                ["callsubr"] + ret,
            ]
            glyphs = {
                ".notdef": end,
                "A": [0, 0, "rmoveto", -106, -104, "callsubr"] + end,
                "B": [0, 0, "rmoveto", -105, -104, "callsubr"] + end,
            }
        elif kind == "shared-hints":
            progs = [[0, 10] + ret]
            glyphs = {
                ".notdef": end,
                "A": [-107, "callsubr", "hstem", 0, 0, "rmoveto", 10, 0, "rlineto"]
                + end,
                "B": [-107, "callsubr", "rmoveto", 20, 0, "rlineto"] + end,
            }
        cs = {g: T2CharString(program=p) for g, p in glyphs.items()}
        if cff2:
            f.setupCFF2(cs)
            top = f.font["CFF2"].cff.topDictIndex[0]
            private = top.FDArray[0].Private
        else:
            f.setupCFF("CFFRegression", {}, cs, {})
            top = f.font["CFF "].cff.topDictIndex[0]
            private = top.Private
        private.Subrs = SubrsIndex()
        for p in progs:
            private.Subrs.append(
                T2CharString(program=p, private=private, globalSubrs=top.GlobalSubrs)
            )
        f.save(
            OUT
            / (
                ("cff2" if cff2 else "cff1")
                + (
                    "_caller_subr.otf"
                    if kind == "caller-dependent"
                    else "_shared_hint_operands.otf"
                )
            )
        )
# Multiple blend groups accumulate fourteen live results, not 574 operands.
f = builder()
f.setupFvar([("wght", 100, 400, 900, "Weight")], [])
f.setupCFF2(
    {
        g: T2CharString(
            program=[0, 0, "rmoveto", 10, 0, "rlineto"] if g != ".notdef" else []
        )
        for g in [".notdef", "A", "B"]
    },
    regions=[{"wght": (0, 1, 1)}] * 40,
)
private = f.font["CFF2"].cff.topDictIndex[0].FDArray[0].Private
private.BlueValues = [[i] + [0] * 40 for i in range(14)]
f.save(OUT / "cff2_long_private_blends.otf")
with TTFont(OUT / "cff2_long_private_blends.otf") as check:
    assert check["CFF2"].cff.topDictIndex[0].FDArray[0].Private.BlueValues == [
        [i] + [0] * 40 for i in range(14)
    ], "Private DICT blend groups did not preserve the requested hint values"

# Two FDs reference the same drawing-only global subroutine.
f = builder()
f.setupCFF2(
    {
        g: T2CharString(
            program=[] if g == ".notdef" else [0, 0, "rmoveto", -107, "callgsubr"]
        )
        for g in [".notdef", "A", "B"]
    },
    fdArrayList=[{}, {}],
)
top = f.font["CFF2"].cff.topDictIndex[0]
sel = FDSelect(format=0)
sel.gidArray = [0, 0, 1]
top.FDSelect = sel
for i, g in enumerate([".notdef", "A", "B"]):
    top.CharStrings[g].fdSelectIndex = sel.gidArray[i]
f.font["CFF2"].cff.GlobalSubrs.append(T2CharString(program=[10, 0, "rlineto"]))
f.save(OUT / "cff2_shared_global.otf")
# dotsection is a no-op even when live operands precede it.
f = builder()
f.setupCFF(
    "CFFDotsection",
    {},
    {
        ".notdef": T2CharString(program=["endchar"]),
        # fontTools does not accept the deprecated opcode's source spelling.
        "A": T2CharString(bytecode=bytes([189, 199, 12, 0, 21, 149, 149, 5, 14])),
        "B": T2CharString(program=[0, 0, "rmoveto", 20, 20, "rlineto", "endchar"]),
    },
    {"defaultWidthX": 500},
)
f.save(OUT / "cff1_dotsection_live_args.otf")

# The same bytes have two legal mask tokenizations depending on stem count.
# Both glyph orders must work, including when one glyph falls back to flattening.
for order, suffix in [([".notdef", "A", "B"], "forward"), ([".notdef", "B", "A"], "reverse")]:
    f = builder()
    f.setupGlyphOrder(order)
    programs = {".notdef": ["endchar"]}
    for name, stems in [("A", 16), ("B", 8)]:
        programs[name] = [0, 10] * stems + [
            "hstem", 50, 60, "rmoveto", -107, "callsubr", 10, 10, "rlineto", "endchar"
        ]
    f.setupCFF(
        "CFFContextualMasks",
        {},
        {name: T2CharString(program=programs[name]) for name in order},
        {"defaultWidthX": 500},
    )
    top = f.font["CFF "].cff.topDictIndex[0]
    private = top.Private
    private.Subrs = SubrsIndex()
    private.Subrs.append(
        T2CharString(bytecode=bytes([19, 0, 19, 11, 11]), private=private, globalSubrs=top.GlobalSubrs)
    )
    f.save(OUT / f"cff1_contextual_masks_{suffix}.otf")

f = builder()
f.setupFvar([("wght", 0, 0, 1, "Weight")], [])
group = [10, 20] + [1] * 80 + [2, "blend"]
f.setupCFF2(
    {
        ".notdef": T2CharString(program=[]),
        "A": T2CharString(program=[0, 0, "rmoveto"] + group + ["rlineto"]),
        "B": T2CharString(program=[0, 0, "rmoveto"] + group + [0, "blend"] + group + ["rlineto"]),
    },
    regions=[{"wght": (0, 1, 1)}] * 40,
)
f.save(OUT / "cff2_blend_identity.otf")
print(OUT)
