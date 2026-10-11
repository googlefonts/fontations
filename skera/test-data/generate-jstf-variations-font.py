"""Generate JSTF-only variation references and justification lookup closures."""

from copy import deepcopy
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import newTable
from fontTools.ttLib.tables import otTables as ot
from fontTools.ttLib.tables.otBase import ValueRecord
from fontTools.varLib.builder import (
    buildVarDevTable,
    buildVarData,
    buildVarStore,
    buildVarRegionList,
)


def table(typ, **fields):
    obj = getattr(ot, typ)()
    obj.__dict__.update(fields)
    obj.populateDefaults()
    return obj


def build():
    fb = FontBuilder(1000, isTTF=True)
    names = [".notdef", "unused", "a", "b", "extender"]
    fb.setupGlyphOrder(names)
    fb.setupCharacterMap({0x61: "a", 0x62: "b"})
    pen = TTGlyphPen(None)
    fb.setupGlyf({name: pen.glyph() for name in names})
    fb.setupHorizontalMetrics({name: (600, 0) for name in names})
    fb.setupHorizontalHeader(ascent=800, descent=-200)
    fb.setupNameTable({"familyName": "JSTF Variations", "styleName": "Regular"})
    fb.setupOS2(
        sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200
    )
    fb.setupPost()
    fb.setupFvar([("wght", 0, 0, 1, "Weight"), ("wdth", 0, 0, 1, "Width")], [])
    f = fb.font
    addOpenTypeFeaturesFromString(
        f,
        "feature kern { pos a <10 20 30 0>; } kern; feature curs { pos cursive a <anchor 100 200> <anchor 300 400>; } curs;",
    )
    lookups = f["GPOS"].table.LookupList.Lookup
    single = lookups[0].SubTable[0]
    single.Value.XPlaDevice = buildVarDevTable(2)
    single.Value.YPlaDevice = buildVarDevTable(3)
    single.ValueFormat |= 0x30
    anchor = lookups[1].SubTable[0].EntryExitRecord[0].EntryAnchor
    anchor.Format = 3
    anchor.XDeviceTable = buildVarDevTable(4)
    anchor.YDeviceTable = buildVarDevTable(5)
    exit_anchor = lookups[1].SubTable[0].EntryExitRecord[0].ExitAnchor
    exit_anchor.Format = 3
    exit_anchor.XDeviceTable = table(
        "Device", StartSize=12, EndSize=14, DeltaFormat=2, DeltaValue=[1, 0, -1]
    )
    exit_anchor.YDeviceTable = None
    empty = table(
        "SinglePos",
        Format=2,
        Coverage=table("Coverage", glyphs=["a", "b"]),
        ValueFormat=0,
        Value=[ValueRecord(), ValueRecord()],
    )
    lookups.append(table("Lookup", LookupType=1, LookupFlag=0, SubTable=[empty]))
    priority = table(
        "JstfPriority",
        ShrinkageJstfMax=table("JstfMax", Lookup=deepcopy(lookups)),
        ExtensionJstfMax=table("JstfMax", Lookup=deepcopy(lookups)),
    )
    second = deepcopy(priority)
    for maximum in (second.ShrinkageJstfMax, second.ExtensionJstfMax):
        maximum.Lookup[0].SubTable[0].Value.XPlacement += 3
    lang = table("JstfLangSys", JstfPriority=[priority, second])
    script = table(
        "JstfScript",
        DefJstfLangSys=lang,
        JstfLangSysRecord=[
            table(
                "JstfLangSysRecord", JstfLangSysTag="ENG ", JstfLangSys=deepcopy(lang)
            )
        ],
    )
    f["JSTF"] = newTable("JSTF")
    f["JSTF"].table = table(
        "JSTF",
        Version=0x00010000,
        JstfScriptRecord=[
            table("JstfScriptRecord", JstfScriptTag="latn", JstfScript=script)
        ],
    )
    del f["GPOS"]
    if "GDEF" not in f:
        f["GDEF"] = newTable("GDEF")
    f["GDEF"].table = table("GDEF", Version=0x00010003)
    f["GDEF"].table.VarStore = buildVarStore(
        buildVarRegionList(
            [{"wght": (0, 1, 1)}, {"wdth": (0, 1, 1)}], ["wght", "wdth"]
        ),
        [
            buildVarData(
                [0, 1], [[999, -999], [0, 0], [10, 20], [-20, 30], [30, -10], [40, -20]]
            )
        ],
    )
    return f


def save(font, name):
    font.recalcTimestamp = False
    font["head"].created = font["head"].modified = 2082844800
    font.save(Path(__file__).parent / "fonts" / name)


font = build()
save(font, "jstf-variations.ttf")
gdef = deepcopy(font["GDEF"])
addOpenTypeFeaturesFromString(
    font,
    "feature salt { sub a by b; } salt; feature kern { pos a <10 20 30 0>; } kern; feature curs { pos cursive a <anchor 100 200> <anchor 300 400>; } curs;",
)
font["GDEF"] = gdef
script = font["JSTF"].table.JstfScriptRecord[0].JstfScript
script.ExtenderGlyph = table("ExtenderGlyph", ExtenderGlyph=["extender"])
for language in (script.DefJstfLangSys, script.JstfLangSysRecord[0].JstfLangSys):
    for priority in language.JstfPriority:
        priority.ShrinkageEnableGSUB = table("JstfGSUBModList", GSUBLookupIndex=[0])
        priority.ExtensionDisableGSUB = table("JstfGSUBModList", GSUBLookupIndex=[0])
        priority.ShrinkageEnableGPOS = table("JstfGPOSModList", GPOSLookupIndex=[1])
        priority.ShrinkageDisableGPOS = table("JstfGPOSModList", GPOSLookupIndex=[0])
save(font, "jstf-lookup-closure.ttf")
