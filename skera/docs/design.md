# Skera subsetting and instancing

Axis restrictions belong to the subset plan. Skera reads the original font,
collects retained glyphs and variation references, and serializes one output
font. Input variation tables remain available when the caller requests their
removal from the output.

## Public API

Use `Plan::new_with_axis_limits` to plan selection and instancing together, then
call `subset_font` with the same source font. It accepts the ordinary subset
selectors plus axis pins or retained ranges. `Plan::with_axis_limits` replans an
existing selection; apply it before setting a custom glyph mapping.

`instance_font` and `instance_font_with_flags` use the same pipeline with all
glyphs selected and glyph IDs retained. They preserve layout lookup identities
for callers that inspect those tables. Combined subsetting can prune lookups,
features, classes, paints, and variation rows.

The CLI passes its selectors and axis restrictions to one plan. VARC uses the
retained component closure to determine which axes can change. Glyph mappings
and requested table drops apply to the final output.

## Planning and table dependencies

`Plan` owns axis planning and the maps used by borrowed table subsetters.
`SubsetState` owns outline-dependent output buffers during serialization.

1. `AxisPlan` validates user requests and records retained axes, coordinate
   mappings, normalized restrictions, and metric coordinates. Unchanged range
   requests leave the variation space intact.
2. Feature conditions are evaluated or rebased before layout closure. Active
   alternate features participate in glyph and lookup selection. Retained
   alternate parameters keep their original feature tags.
3. Existing cmap, MATH, layout, COLR, VARC, and composite closures determine the
   retained glyph space. Custom output IDs are assigned after closure.
4. Layout and color plans collect referenced rows, evaluate default deltas, and
   build residual stores. GDEF is shared by GPOS, feature conditions, carets,
   MATH, and JSTF. BASE and COLR have independent stores.
5. Outline processing produces final glyph-order outlines and metrics for the
   retained glyphs. Metadata and MVAR update dependent headers. Native table
   subsetters apply the planned value and index changes to borrowed inputs.
6. `FontBuilder` assembles the finished tables once. CFF2-to-CFF1 downgrade
   converts the retained CFF table directly using the final advances and bounds.

Outline results include glyf/loca, residual gvar/cvar, CVT values, metrics,
bounds, and headers. A tag has one pending `Keep`, `Replace`, or `Remove` change.
Only the coordinator can replace arbitrary output tables. `Instance` contexts
name a table's numerical dependencies; they do not expose that output map.

## Borrowed layout and paint graphs

GPOS, GSUB, GDEF, BASE, MATH, and JSTF use their regular Skera subset serializers
for both ordinary subsets and instances. The layout map associates each source
variation index with its residual index and default delta. Value records,
anchors, carets, and BASE coordinates add that delta while writing their scalar
fields. Constant residual rows lose their variation devices. Ordinary ppem
devices retain their separate semantics.

A device-only positioning record can acquire a scalar field after instancing.
Its output value format is computed from the transformed fields. Zero-byte
SinglePos arrays and PairPos matrices use declared source counts and dimensions;
subsetting can compact their retained glyphs and classes.

Feature metadata is prepared separately from lookup graphs because closure
needs the selected lookup lists. This preparation serializes borrowed feature
parameters and simplified conditions with Skera's serializer. It does not own
GPOS or GSUB lookup graphs. Variable-value conditions can require private rows
to preserve an unrounded sign without changing a shared positioning row's
integer rounding. These rows join the GDEF transaction before optimization.

COLR uses its existing glyph, palette, layer, clip, and paint subsetters.
Variable paints are evaluated one node at a time. Numerical conversion can use
a local paint value and flat stop or affine data; child paints remain borrowed
and are linked through the serializer. Static conversion preserves large scales,
negative radial-gradient handling, angle periodicity, Fixed precision, and
outward clip rounding.

The serializer caches opted-in borrowed objects by source identity. Repeated
paint, positioning-subtable, lookup, and JSTF references link to an existing
serialized object. Active references enforce the nesting bound. Cache entries
are invalidated when their serialized objects are rolled back or the buffer is
reset. Subsetting uses the same cache, offset graph, deduplication, and overflow
repacker as instancing.

## Variation stores and coordinate spaces

`StorePlan` represents each source region as a default contribution plus sparse
residual contributions. `rebase` shares the tent solver between tuples, CFF2
blends, and item stores. Only retained item rows are rebuilt for combined
subsetting. HVAR and VVAR select rows with their existing subset plans before
applying the transform. MVAR retains its referenced rows.

The optimizer reads encoded rows directly. It does not serialize an owned store
and parse it again. Dense-matrix and merge-search budgets govern optional
optimization: exceeding one preserves valid rows and index mappings. These
budgets do not reject the font. LONG_WORDS distinguishes its 15-bit wide-column
count from the full 16-bit column count.

Explicit COLR maps permit row deduplication and zero-row sentinels. Implicit
consecutive indices must remain consecutive. Standalone instancing preserves
implicit indices; combined subsets can compact them using the retained row map.
Empty maps retain their identity semantics. A mapped no-variation sentinel is
resolved before requiring a store.

A user-space avar2 pin need not be constant in final coordinates. Constant
final pins can be removed; dependent pins stay hidden and avar2 compensates for
the restricted input space. Reachability filters store transforms and retained
raw gvar streams directly. It does not build a temporary pruned font. VARC
component overrides disable reachability bounds that would be invalid outside
the font-level box.

## Outlines, metrics, and serialization

TrueType instancing processes retained glyphs in output order. Components use
the final glyph map. Continuous component points determine bounds before stored
coordinates round. IUP, phantom points, instructions, zero-contour programs,
cubic flags, and residual tuples retain their established precision rules.
The shared write-fonts simple-glyph encoder supplies compressed point flags and
coordinates for one glyph at a time. Composite and tuple streams append directly
to their destination buffers.

CFF2 interprets retained programs and Private DICT blends, preserving nested
blend expressions and per-blend rounding. Metrics measure continuous source
control bounds; emitted operands have different rounding rules. Optional CFF1
downgrade uses the final advances. CFF2 VariationStore lengths saturate at 65535
without truncating the store.

Both outline families share metric-header aggregation and trailing-advance
compression. Empty glyphs participate in maximum advance but not bound-dependent
extrema. MVAR updates named metric headers. Post glyph-name data remains borrowed
while its small header receives metric changes.

Flat store and axis metadata can use shared write-fonts value types; Skera
serializes them directly. VARC has one shared subset/rewrite path for packed
records and auxiliary data. Its flat output builder is common to subsetting and
instancing. Shared reader/writer schemas are generated once in read-fonts and
write-fonts. Skera has no generated table model.

## Validation and limits

```sh
cargo +1.89 test -p skera -p read-fonts -p write-fonts -p skrifa --all-features
cargo +1.89 clippy -p skera --all-features --all-targets -- -D warnings
cargo +1.89 fmt --all --check
cargo +1.89 run --bin=codegen resources/codegen_plan.toml
cargo +1.89 bench -p skera --bench skera-benchmarks -- instance-subset
```

Regressions cover full and partial instances, composed restrictions, HarfBuzz
references, shared stores, both layout widths, CLI selection, and malformed
inputs. A compact shared-layout fixture represents 225 million subtable
occurrences without expanding them. Shared COLR composites exercise the same
property. Another regression corrupts an unretained CFF2 program and confirms
that combined instancing does not interpret it.

The benchmark compares combined planning with full-glyph instancing followed by
subsetting, using the same implementation and source fonts. It includes plan
creation and output allocation. Results depend on glyph selection, graph
sharing, store size, and coordinate restrictions.

An example local release run (Rust 1.89, 20 Criterion samples) selected ABC for
Roboto Flex and AdobeVFPrototype and abc for the affine COLR fixture. It pinned
the first axis three quarters of the way from its default to its maximum:

| Fixture | Combined | Instance all glyphs, then subset |
| --- | ---: | ---: |
| RobotoFlex-Variable.ttf | 0.774 ms | 77.877 ms |
| AdobeVFPrototype.otf | 0.251 ms | 13.891 ms |
| colr-affine-values.ttf | 0.0193 ms | 0.0928 ms |

These numbers illustrate the cost of instancing discarded glyphs; they are not
a general performance guarantee. A separate nine-run CLI comparison against
the old pipeline on current main (`b2d45627a`) selected ABC and pinned Roboto
Flex's weight to 700. Median elapsed time was 98.3 ms versus 4.88 ms, and median
peak RSS was 25,912 KiB versus 7,952 KiB. The resulting subsets were 12,564 and
12,508 bytes. CLI timing includes process startup and file I/O.

Instancing uppercase GLYF/GVAR and metric families remains unsupported. Changes
to axes referenced by retained VARC data are rejected. Format field widths,
condition/outline recursion limits, and numerical expansion bounds still apply.
F2Dot14 quantization and intermediate rounding can make composed restrictions
differ at rounding or float-underflow boundaries. Unknown tables follow the
ordinary passthrough rules.
