"""Regenerate cubic glyf instancing fixtures and HarfBuzz references.

Run with an experimental-API-enabled hb-subset binary as the first argument.
HarfBuzz recognizes cubic flags in GLYF/LOCA. Fontations uses the same simple
outline encoding with glyf/loca and its spec_next feature, so translate only
the table tags when preparing and reading the HarfBuzz references.
"""

from pathlib import Path
import subprocess
import sys

from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.DefaultTable import DefaultTable
from fontTools.ttLib.tables._f_v_a_r import Axis
from fontTools.ttLib.tables.TupleVariation import TupleVariation


def rename_tables(font, mapping):
    result = TTFont(recalcTimestamp=False)
    for tag in font.keys():
        if tag == "GlyphOrder":
            continue
        renamed = mapping.get(tag, tag)
        table = DefaultTable(renamed)
        table.data = font.getTableData(tag)
        result[renamed] = table
    return result


directory = Path(__file__).parent
font = TTFont(
    directory.parents[1] / "font-test-data/test_data/ttf/cubic_glyf.ttf",
    recalcTimestamp=False,
)
font["name"] = newTable("name")
font["name"].names = []
font["fvar"] = newTable("fvar")
font["fvar"].axes = []
font["fvar"].instances = []
for i, tag in enumerate(["TEST", "AXIS"]):
    axis = Axis()
    axis.axisTag = tag
    axis.minValue, axis.defaultValue, axis.maxValue = -1, 0, 1
    axis.axisNameID = 256 + i
    font["fvar"].axes.append(axis)
    font["name"].setName(tag, 256 + i, 3, 1, 0x409)
font["gvar"] = newTable("gvar")
font["gvar"].variations = {g: [] for g in font.getGlyphOrder()}
font["gvar"].variations[font.getGlyphOrder()[2]] = [
    TupleVariation(
        {"TEST": (0, 1, 1)},
        [(0, 0), (0, 0), (40, 60), (-40, 60), (0, 0), (0, 0)] + [(0, 0)] * 4,
    ),
    TupleVariation({"AXIS": (0, 1, 1)}, [(20, 0)] * 6 + [(0, 0)] * 4),
]
font.save(directory / "fonts/cubic-glyf-variable.ttf")

# Use a task-local directory to keep temporary fonts out of the fixture tree.
temporary = directory.parents[1] / "target/cubic-instance-validation"
temporary.mkdir(parents=True, exist_ok=True)
source = temporary / "hb-source.ttf"
rename_tables(font, {"glyf": "GLYF", "loca": "LOCA"}).save(source)
for mode, request in [("full", "TEST=0.5,AXIS=0.5"), ("partial", "TEST=0:0.5:1")]:
    output = temporary / f"hb-{mode}.ttf"
    subprocess.run(
        [sys.argv[1], str(source), "--keep-everything",
         f"--instance={request}", f"--output-file={output}"],
        check=True,
    )
    # Preserve the reference's bytes when translating its table tags.
    reference = TTFont(output, recalcTimestamp=False)
    result = TTFont(recalcTimestamp=False)
    for tag in reference.keys():
        if tag == "GlyphOrder":
            continue
        renamed = {"GLYF": "glyf", "LOCA": "loca"}.get(tag, tag)
        table = DefaultTable(renamed)
        table.data = reference.reader[tag]
        result[renamed] = table
    result.save(directory / f"expected/cubic-instancing/{mode}.ttf")
