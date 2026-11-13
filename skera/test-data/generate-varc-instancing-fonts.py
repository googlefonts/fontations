"""Regenerate VARC instancing fixtures using FontTools with the revised format.

Adapted from HarfBuzz test/api/generate-varc-instancing-fonts.py (#6265).
"""

from copy import deepcopy
from pathlib import Path

from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables import otTables
from fontTools.ttLib.tables._f_v_a_r import Axis
from fontTools.ttLib.tables.TupleVariation import TupleVariation
from fontTools.varLib.builder import buildVarData, buildVarRegionList, buildVarStore


def condition(format, **fields):
    result = otTables.ConditionTable()
    result.Format = format
    result.__dict__.update(fields)
    return result


directory = Path(__file__).parent / "fonts"
font = TTFont(
    Path(__file__).parents[2] / "font-test-data/test_data/ttf/varc-ac01-conditional.ttf",
    recalcTimestamp=False,
)
# Load gvar before changing the axis order.
variations = font["gvar"].variations
for tag in reversed(("DUMY", "COND")):
    axis = Axis()
    axis.axisTag = tag
    axis.minValue, axis.defaultValue, axis.maxValue = -1, 0, 1
    axis.axisNameID = font["fvar"].axes[0].axisNameID
    font["fvar"].axes.insert(0, axis)

varc = font["VARC"].table
varc.AxisIndicesList.Item = [[i + 2 for i in indices]
                            for indices in varc.AxisIndicesList.Item]
for region in varc.MultiVarStore.SparseVarRegionList.Region:
    for axis in region.SparseVarRegionAxis:
        axis.AxisIndex += 2

# COND is used only by nested conditions; DUMY is absent from VARC.
weight_condition = varc.ConditionList.ConditionTable[0]
def remap_axes(c):
    if c.Format == 1:
        c.AxisIndex += 2
    elif c.Format in (3, 4):
        for child in c.ConditionTable:
            remap_axes(child)
    elif c.Format == 5:
        remap_axes(c.ConditionTable)


for c in varc.ConditionList.ConditionTable:
    remap_axes(c)
low = condition(1, AxisIndex=1, FilterRangeMinValue=-1, FilterRangeMaxValue=0)
middle = condition(1, AxisIndex=1, FilterRangeMinValue=0, FilterRangeMaxValue=0.25)
either = condition(4, ConditionTable=[low, middle])
negated = condition(5, ConditionTable=either)
varc.ConditionList.ConditionTable[0] = condition(
    3, ConditionTable=[weight_condition, negated])

# Give the unrelated axis a visible effect in an underlying outline.
name = "glyph00003"
points = len(font["glyf"][name].getCoordinates(font["glyf"])[0])
variations[name].append(TupleVariation(
    {"DUMY": (0, 1, 1)}, [(20, 0)] * points + [(0, 0)] * 4))
# This tent is reached by a VARC override, but not by the font-level
# default of the private axis. Font-level avar2 culling must preserve it.
variations[name].append(TupleVariation(
    {"0000": (-1, -0.5, -0.25)}, [(0, 20)] * points + [(0, 0)] * 4))
font.save(directory / "varc-unrelated-axis.ttf")

font = deepcopy(font)
for axis in font["fvar"].axes:
    if axis.axisTag.startswith("000"):
        axis.flags = 1  # Hidden component axes.
avar = font["avar"] = newTable("avar")
avar.majorVersion, avar.minorVersion = 2, 0
avar.segments = {a.axisTag: {-1: -1, 0: 0, 1: 1} for a in font["fvar"].axes}
avar.table = otTables.avar()
avar.table.VarIdxMap = None
avar.table.VarStore = None
font.save(directory / "varc-unrelated-axis-avar2.ttf")

# A non-null store also exercises skera's coupled-axis and reachability paths.
axes = [a.axisTag for a in font["fvar"].axes]
avar.table.VarStore = buildVarStore(
    buildVarRegionList([], axes),
    [buildVarData([], [[] for _ in axes], optimize=False)],
)
font.save(directory / "varc-unrelated-axis-avar2-store.ttf")
