# Skera subsetting and instancing design

Skera has two operations: subsetting selects and remaps glyphs and their
dependencies; instancing changes the variation space while preserving glyph
IDs. Table operations implement traits. Font coordinators resolve dependencies
between their results and assemble the font. Only the instancing coordinator
owns the font-wide map of pending table changes.

This design covers the Skera feature stack: CFF and TrueType outlines, full and
partial instancing, coupled avar2 mappings, layout and color variations, VARC,
JSTF, and CLI selection. The separate reader-format and Skrifa performance
changes provide supporting functionality.

## Public operations

[`Plan` and `subset_font`](../src/lib.rs) describe and execute a subset. Planning
collects glyph dependencies, layout references, names, and variation indices
before serialization. Custom glyph mappings belong to this operation.

[`instance_font` and `instance_font_with_flags`](../src/instance/mod.rs) accept
axis pins or retained ranges. Unspecified axes remain variable. A full instance
removes the variation data it has consumed; a partial instance changes the
default and rebuilds variation data for the remaining space. Instancing
preserves glyph IDs, including composite references. Callers create a subset
plan from the returned font when they also want a subset.

The CLI usually instances before its final subset. With VARC, it first subsets
while retaining source glyph IDs and the source glyph count. This removes
unused component and axis references and preserves inputs needed for instancing
and final Unicode selection. Requested table drops and custom glyph IDs take
effect at the appropriate later stage. See [the CLI](../src/main.rs) and
[the usage guide](../README.md).

## Table operations and font coordination

The internal [`Instance` trait](../src/instance/table.rs) defines the boundary:

```rust
pub(crate) trait Instance<C> {
    type Output;

    fn instance(&self, context: &mut C) -> Result<Self::Output, SubsetError>;
}
```

The receiver is a borrowed read-fonts table. Its context names its dependencies,
and its associated output describes its result. There is no default
implementation: handlers explicitly implement their operation. This trait is
internal and does not add a public extension API.

`FontContext` contains an immutable source font, an immutable axis plan, and
flags. Operations needing mutable dependencies use narrower contexts:
`GvarGlyphContext` updates one glyph's coordinates; `CvarContext` updates CVT
values; `MvarContext` borrows only hhea, vhea, OS/2, post, and gasp through
`MvarHeaders`; and `LayoutContext` holds a transaction for one shared store.
None contains the font's output map.

Most operations return bytes or a `TableChange`. `Keep` leaves the current
candidate in place, `Replace` supplies bytes, and `Remove` drops it. Richer
results express dependencies: fvar returns both bytes and the original axis
records needed by avar; layout handlers return owned tables awaiting a shared
store remap. When the source has no avar, metadata rebuilding supplies identity
maps for retained axes. This differs from preserving a present empty map.

`FontOutput` stores one pending change per tag. Replacement and removal are
mutually exclusive states. A later `Keep` preserves the previous phase's
candidate, including a removal; a later replacement can restore a removed
table. The assembly tests exercise these transitions and unknown-table
preservation.

| Table operations | Context and result | Implementation |
| --- | --- | --- |
| CFF2 | Source and axes; compiled bytes | [CFF entry points](../src/cff/mod.rs), [blend instancing](../src/cff/instance.rs) |
| glyf and gvar | Outline dependencies and bounded metric state; glyphs, loca, residual gvar | [TrueType](../src/instance/truetype.rs) |
| cvar | Axes and mutable CVT values; residual cvar or removal | [TrueType](../src/instance/truetype.rs) |
| HVAR and VVAR | Axes; rebuilt metric stores | [Metrics](../src/instance/metrics.rs) |
| MVAR | Axes and metric headers; header deltas and residual MVAR | [Metrics](../src/instance/metrics.rs) |
| GPOS, GSUB, GDEF, JSTF | GDEF store transaction; owned tables awaiting remapping | [Layout](../src/instance/layout.rs) |
| BASE | Its own store and axes; bytes preserving its version | [Layout](../src/instance/layout.rs) |
| COLR | Its own store, index map, and axes; serialized paints | [Color](../src/instance/color.rs) |
| fvar and avar | Original axes and restrictions; retained metadata and mappings | [Metadata](../src/instance/metadata.rs), [avar2](../src/instance/avar2.rs) |
| STAT | Original fvar axes and user ranges; retained STAT records | [STAT](../src/instance/stat.rs) |
| VARC | Axes and retained component references; remapped table or removal | [VARC](../src/varc.rs) |

Two dependencies need font coordination:

* Outlines determine bounds, phantom-point metrics, loca offsets, and header
  summaries. These results must agree about glyph count, point order, and
  rounding. `TrueTypeTables` and `MetricTables` name the affected outputs.
  Outline helpers address named metric and header buffers. MVAR's borrowed
  header view cannot change advance arrays, outline bytes, or axis metadata;
  its record targets use a closed enum rather than arbitrary font-table tags.
* GPOS, GSUB variation conditions, GDEF caret values, and JSTF can reference
  GDEF's store. They must share one rebuild and one row-index remapping.
  `LayoutTables` names the outputs of this transaction.

Subsetting uses [`Subset` and `SubsetTable`](../src/lib.rs), with `Plan` and
`SubsetState` for dependencies. Instancing follows the same table-oriented
organization. Shared numerical algorithms remain functions: traits do not
replace the rebasing solver or the font coordinator.

## Instancing phases and coordinate spaces

The order in [the coordinator](../src/instance/mod.rs) is part of the design:

1. **Plan axes.** [`AxisPlan`](../src/instance/axes.rs) validates requests,
   records user ranges and pins, computes normalized coordinates, and records
   axis removal and remapping. `coords` and `metric_coords` preserve the
   different normalization paths used by outline/store and metric consumers.
2. **Check VARC.** Retained component references constrain which axes can
   change. Unrelated axes can be instanced; unsupported changes to referenced
   axes return an error. The coordinator delegates this phase to
   `instance_varc`.
3. **Handle avar2 reachability.** Compute reachable final coordinates and
   constant final pins. [Culling](../src/instance/cull.rs) removes unreachable
   regions and tuples in the original final-coordinate space while preserving
   live row indices. A temporary font view exposes the pruned source tables.
4. **Instance outlines and metrics.** CFF2 evaluates charstrings and private
   dictionaries. TrueType evaluates gvar, preserves components, computes
   bounds and phantom metrics, and applies cvar. Coupled mappings use the
   final-coordinate plan for these consumers. `instance_outlines` dispatches
   through `OutlineKind` and commits the declared outline results.
5. **Instance layout and color.** Layout consumers share the GDEF transaction;
   BASE and COLR use their own stores. `instance_layout_and_color` commits
   their declared outputs.
6. **Finish metadata.** Partial instances rebuild fvar and avar from original
   axis records. Full instances remove fvar, avar, HVAR, VVAR, and MVAR. STAT
   filtering uses original user-axis metadata. `finish_metadata` uses the
   original source context; DSIG is removed because the font has changed.
7. **Assemble.** `FontOutput` applies changes and passes raw bytes to
   `FontBuilder`. Unchanged tables stay borrowed until assembly. Optional full
   CFF2-to-CFF1 conversion runs afterward.

avar1 maps axes independently. avar2 can couple them: a pinned user axis does
not necessarily have a constant final coordinate. Such an axis can remain
hidden in fvar while avar carries the restriction. Constant final pins can be
baked into consumers and removed. Compensation remains subject to F2Dot14
quantization. VARC component overrides can reach coordinates outside the
font-level range, so culling is disabled where those overrides invalidate the
reachability bound.

## Shared variation stores

[`StorePlan`](../src/instance/store.rs) and
[the rebasing solver](../src/instance/rebase.rs) express a source region as its
contribution at the new default plus residual regions over retained axes. The
same transformation supports item rows, CFF2 blends, and glyph tuples, with
each consumer's encoding and rounding rules. The solver is generic over its
scalar type: TrueType tuples use `Triple<f32>`, while the existing CFF2 and
item-store consumers retain their double representation. The algorithm has
one implementation.

The plan also accumulates pinned region factors in source float32 order. Once
that product reaches zero, every retained coordinate keeps the region at zero;
carrying a tiny double residual into condition rescaling would incorrectly
revive it. Nonzero subnormal products have a different problem: multiplying by
a retained factor can cross a source underflow boundary that positive rescaling
cannot preserve. Partial variable-value conditions referencing those regions
return an error; full instancing remains available. The regression covers the
zero product, nonzero defaults, and explicit subnormal rejection. The matching
upstream fix is [HarfBuzz #6441](https://github.com/harfbuzz/harfbuzz/pull/6441).
FontTools currently lacks these condition formats; [#4348](https://github.com/fonttools/fonttools/pull/4348)
rejects them before mutating the font instead of discarding a matching rule.

Store optimization has independent budgets for its dense row matrix and its
pairwise column scans. It checks the merge budget after deduplication so many
duplicate groups can still be cheap. If the optional search would exceed the
budget, it retains the source store and identity index map. Pair costs use
borrowed column iterators and allocate no temporary column vector. Encoding
ties follow HarfBuzz's forward column ordering. See the
[optimizer regressions](../src/instance/optimize.rs).

The layout transaction has two passes. First, table handlers apply default
deltas, simplify references and feature conditions, and collect any additional
condition rows needed in the residual store. The coordinator rebuilds and
optimizes that store once. It then remaps every consumer before serializing
GDEF and its dependents. Saving or optimizing GDEF before processing JSTF
could discard rows that table still uses.

The value walker receives an explicit pass:

```rust
enum ValuePass<'a, 'b> {
    Instance(&'a LayoutContext<'b>),
    Remap(&'a IndexMap),
}
```

The remapping pass has no source store or coordinates. It can change or remove
variation references, but cannot evaluate default deltas again. Both passes
walk the same value fields and preserve ordinary ppem devices. Feature
condition evaluation and row-index remapping remain separate operations.

The layout implementation separates review responsibilities:

| Boundary | Code | What to check |
| --- | --- | --- |
| Table operations and shared store transaction | [layout.rs](../src/instance/layout.rs) | Every consumer finishes before store optimization; all results use the same remap |
| Value and device walking | [values.rs](../src/instance/layout/values.rs) | Only instancing applies deltas; value formats, anchors, and ppem devices remain consistent |
| Feature conditions and substitutions | [features.rs](../src/instance/layout/features.rs) | Condition order, Boolean boundaries, private condition rows, and feature parameters |
| Borrowed-to-owned conversion repairs | [owned.rs](../src/instance/layout/owned.rs) | Zero-byte record counts and matrix dimensions survive conversion; expansion stays bounded |
| Shared-store regressions | [tests.rs](../src/instance/layout/tests.rs) | Precision, remapping, sentinels, conditions, and empty records |

BASE and COLR own independent stores. An explicit COLR index map permits
remapping optimized rows. Without a map, consecutive paint fields address
consecutive rows directly, so their sequence must remain stable.

`NO_VARIATION_INDEX` denotes zero variation. Resolve a mapped sentinel before
requiring a store or looking up a row. Empty or absent stores differ from live
references to missing data. Encoders also respect LONG_WORDS and its 15-bit
word-count field: narrowing must keep headers and byte widths consistent.

## Format and arithmetic invariants

The architecture refactor preserves existing numerical and serialization
algorithms. Table handlers and shared helpers maintain these rules:

* Preserve TrueType points, component references, curve-kind bits, and
  instructions. Phantom metrics and bounds describe the same outline. CFF2
  preserves nested blends and independent rounding boundaries. Retained
  subroutine calls require immediate literal targets; desubroutinization handles
  caller-dependent targets.
* Preserve each consumer's precision. CFF folding, metric rounding, tuple
  scalar accumulation, Fixed transforms, and F2Dot14 fields do not share one
  interchangeable rounding rule.
* Preserve ppem device corrections while resolving variation devices. Retain
  feature parameters and condition semantics, including meaningful null
  references. Preserve the source BASE version when its store is removed.
* Retain matrix dimensions including unused classes. ClassDef maxima specify
  minimum dimensions. Readers can omit zero-byte records, which need a defined
  reconstruction rule.
* Preserve COLR geometry and colors. Large scales can require affine paints;
  negative derived radial radii require reparameterizing circles and stops;
  minimum and maximum clip edges round differently. Both the Skrifa renderer
  and the new read-fonts renderer round rendering clips outward in font units.
  The read-fonts glyph-extents path retains its separate delta-rounding policy.
  Store remapping covers paint, stop, and clip references.

Composition tests exercise both axis orders and the supported precision rules.
Quantized mappings and intermediate rounding prevent a universal claim of exact
mathematical equivalence for every sequence of restrictions.

CFF2 VariationStores can exceed 65,535 bytes. OFF 2026 section 5.3.3.11.2
requires the length field to be 65535 in that case. Both intermediate and
final instancing writes preserve the full store. Ordinary subsetting finds
its extent from the referenced region list and VarData byte ranges, rather
than truncating at the saturated field.

The shared `VariationRegion::compute_scalar_f32` reader helper computes float
region gains without an intermediate 16.16 quantization. CFF2 operands and
feature-condition signs use double products and accumulation; metric consumers
retain their own accumulation policy. Continuous source bounds resolve nested
blends without operand rounding. Full instancing rounds at each blend boundary;
partial operands, residual deltas, and cumulative Private DICT values have
separate rounding rules.

Variation-store width reduction cannot encode more than 32767 short columns
without setting LONG_WORDS. When that boundary is crossed, Skera emits a long
representation with a 16-bit tail so the header and payload agree. The existing
#2338 correction and boundary regression now live in #2276. HarfBuzz and
FontTools preserve the same decoded deltas in the public-font counterexamples.

GPOS ownership preserves zero-byte value-record counts in SinglePos formats
2 and 4, including extension lookups. Empty PairPos matrices charge their row
allocations even when the column count is zero; a zero-row matrix does not
allocate a discarded temporary column vector. The corrections live at #2274,
where empty-record recovery is introduced.

STAT design-axis byte ranges use checked multiplication and addition before
borrowing the source slice. Oversized declared counts or offsets return a
table error, including on 32-bit hosts.

TrueType tuple scalars reuse `TupleVariation::compute_scalar_f32`. Residual
IUP interpolation and tent splitting also use float arithmetic, matching
current HarfBuzz. Changed avar segment maps renormalize in float and encode
half-unit ties toward positive infinity. The public regressions cover cases
where double intermediates changed stored outline/CVT values, inferred deltas,
region boundaries, and a second-stage instance.

Zero-contour glyphs can contain instructions for phantom points. Instancing,
ordinary subsetting, shared glyph conversion, and write-fonts serialization
preserve those programs unless hint removal is requested. A composition test
covers both operation orders and overlap/hint flags. Point counts use `usize`,
including a contour ending at point 65535. Cubic flags come from the existing
owned `CurvePoint.cubic` field; no parallel point-flag vector is needed.

Empty composites still participate in header metric statistics with a zero
bounding box. All-empty TrueType instances use zero global bounds; the
corresponding HarfBuzz fix is [#6435](https://github.com/harfbuzz/harfbuzz/pull/6435).
A later instance recomputes glyph bounds even at the new default, where stored
child points and component offsets may have rounded independently. The
HarfBuzz shortcut retained stale glyph headers in that case; [#6436](https://github.com/harfbuzz/harfbuzz/pull/6436)
recomputes them.

## CFF interpreter passes

The [Type 2 interpreter](../src/cff/charstring.rs) has separate actions for
closure, retained programs, flattened commands, bounds, and blend inspection.
Callbacks borrow operands. Closure builds neither tokens nor commands; retained
output borrows the source program and records only edits; flattening records
only the commands it emits. Unchanged byte spans are copied together.

Literal provenance is one bounded source span. Arithmetic does not build or
copy producer collections. Removing hints with shared or computed operands
flattens the affected glyph. Retained recording also compares repeated program
edits and mask boundaries before sharing them between glyphs. This keeps one
glyph's hint deletion from changing another glyph's drawing operands.

Retained calls require an immediately preceding literal in the same program.
Caller-supplied or computed targets return an error in retained mode; use
`--desubroutinize` for those programs. Global subroutines can be shared between
Font DICTs when their rewritten programs agree and they do not reach an
FD-local namespace. FD-dependent programs retain separate specializations.

Private DICT entries preserve their complete borrowed source spans independently
of their live operand stack. Each blend consumes defaults, deltas, and its count
and leaves only its results. INDEX layouts compute checked sizes and write into
the final pre-sized table buffer. Fixture and outline regressions live in
[CFF tests](../tests/cff_test.rs) and
[CFF review regressions](../tests/cff_regression_test.rs).

## Serialization and performance

read-fonts provides borrowed inputs. Ordinary subsetting uses Skera's
[serializer](../src/serialize.rs) and calls its
[overflow repacker](../src/repack.rs) when offsets overflow. Instancing uses
direct serialization for glyphs, tuples, and selected metadata, and write-fonts
owned tables for complex layout, color, and store rebuilding. There is one
generated table model in write-fonts.

Borrowed readers and owned writer tables are generated from the shared
[`codegen inputs`](../../resources/codegen_inputs) with the current font-codegen.
Skera has no additional generated table tree. Layout subsetting uses the shared
coverage-intersection callbacks in [`layout.rs`](../src/layout.rs) and the
visitation and recursion limits in [`read_fonts::limits`](../../read-fonts/src/limits.rs).

`Instance` does not select a writer or require an owned result.
[`dump_table`](../../write-fonts/src/write.rs) validates, builds an object graph,
packs, and serializes. [Packing](../../write-fonts/src/graph.rs) always runs its
initial sort and overflow checks; further recovery is conditional.
`FontBuilder::add_raw` accepts finished bytes without calling `dump_table`.

Owned GPOS or COLR rebuilding can be expensive even if performed once. Writer
choices need measurements for the operation and font size: conversion,
allocations, validation, packing, assembly, and output size. The current
boundary reflects existing serializers and implementation complexity; it is
not a measured guarantee that owned paths are cheap. Table boundaries permit
changing a handler's writer without changing font coordination.

Retained and closure passes do not materialize blend expressions. The nested
value representation shares one identity node per interpreter in those modes.
A synthetic interpreter-only benchmark (1,000 glyphs, each with 100 two-result
blends over 40 regions) uses 8,167 allocation calls, compared with 307,167 before
the allocation correction, and allocates 16,190,632 rather than 742,842,544
bytes. Profiling traced the initial slowdown to the operand push helper being
called out of line. Marking that small helper inline reduced the nested
implementation from 69.5–73.8 ms to 38.0–39.4 ms; the earlier allocation-heavy
implementation took 49.7–51.7 ms in the same local synthetic comparison. These
interpreter-only measurements exclude table serialization and font assembly.
See the [profile and fix on #2264](https://github.com/googlefonts/fontations/pull/2264#issuecomment-6330408690).

All 36 sampled nested full instances agree with HarfBuzz. Independent source
expressions check partial variable-delta products and per-blend rounding in
both pin orders, including quarter and half locations. HarfBuzz partial
instancing lost a variable-delta product in these cases; [HarfBuzz #6434](https://github.com/harfbuzz/harfbuzz/pull/6434)
rejects that unsupported operation instead of silently dropping its product.
It does not implement arbitrary symbolic products. Re-encoding residuals
still has the quantization limits described above; see the
[review findings](https://github.com/googlefonts/fontations/pull/2264#issuecomment-6330089655).

## Review order for the stack

Start with the trait, result types, and coordinator. Then review algorithms by
family and their format and precision regressions. These representative PRs
identify the main slices; later fixes in the same stack refine them.

| Area | Main stack slices | Tests and focus |
| --- | --- | --- |
| CFF | [#2248](https://github.com/googlefonts/fontations/pull/2248), [#2249](https://github.com/googlefonts/fontations/pull/2249), [#2250](https://github.com/googlefonts/fontations/pull/2250), [#2252](https://github.com/googlefonts/fontations/pull/2252) | [CFF](../tests/cff_test.rs), [review regressions](../tests/cff_regression_test.rs): interpreter state, blends, subroutines, bounds, widths |
| TrueType | [#2268](https://github.com/googlefonts/fontations/pull/2268), [#2269](https://github.com/googlefonts/fontations/pull/2269), [#2284](https://github.com/googlefonts/fontations/pull/2284) | [TrueType](../tests/truetype_instance_test.rs), [IUP](../tests/iup_test.rs), [empty glyphs](../tests/empty_glyph_test.rs): components, instructions, phantoms, residual tuples |
| Axes and avar2 | [#2253](https://github.com/googlefonts/fontations/pull/2253), [#2291](https://github.com/googlefonts/fontations/pull/2291), [#2296](https://github.com/googlefonts/fontations/pull/2296) | [Axis planning](../src/instance/axes.rs), [avar2](../src/instance/avar2.rs): coordinate spaces, reachability, hidden pins |
| Layout | [#2255](https://github.com/googlefonts/fontations/pull/2255), [#2274](https://github.com/googlefonts/fontations/pull/2274), [#2281](https://github.com/googlefonts/fontations/pull/2281), [#2328](https://github.com/googlefonts/fontations/pull/2328) | [Layout](../tests/layout_instance_test.rs), [BASE](../tests/base_test.rs), [JSTF](../tests/jstf_test.rs): shared rows, conditions, device adjustments |
| VARC | [#2270](https://github.com/googlefonts/fontations/pull/2270), [#2317](https://github.com/googlefonts/fontations/pull/2317) | [VARC](../tests/varc_test.rs): closure and supported axis changes |
| Color | [#2282](https://github.com/googlefonts/fontations/pull/2282), [#2295](https://github.com/googlefonts/fontations/pull/2295), [#2316](https://github.com/googlefonts/fontations/pull/2316) | [Instances](../tests/color_instance_test.rs), [stores](../tests/color_store_test.rs), [scales](../tests/color_scale_test.rs), [radial gradients](../tests/color_negative_radii_test.rs), [affines](../tests/color_affine_test.rs), [clips](../tests/color_clip_test.rs) |
| Selection | [#2287](https://github.com/googlefonts/fontations/pull/2287), [#2298](https://github.com/googlefonts/fontations/pull/2298), [#2313](https://github.com/googlefonts/fontations/pull/2313) | [Mappings](../tests/glyph_mapping_test.rs), [CLI](../tests/cli_selection_test.rs), [presets](../tests/keep_everything_test.rs): ordering and original glyph identities |

The audit follow-ups [#2337](https://github.com/googlefonts/fontations/pull/2337),
[#2338](https://github.com/googlefonts/fontations/pull/2338),
[#2339](https://github.com/googlefonts/fontations/pull/2339), and
[#2340](https://github.com/googlefonts/fontations/pull/2340) provide small
reproductions for empty metric stores, width boundaries, trailing PairPos
classes, and COLR sentinel maps without stores.

## Integration with beyond64k tables

Layout instancing follows the source header's selected script, feature, and
lookup lists. A non-null extended offset takes precedence; the ignored original
owned list is cleared before rebuilding the shared variation store. Both
positioning widths use the same value and anchor passes. Empty value records
need explicit source counts, including wide SinglePos records and PairPos class
matrices, because their serialized records occupy zero bytes. Reconstruction
charges row allocations even when there are no columns; a zero-row matrix
does not allocate a temporary column vector.

Feature variations retain both classic alternate features and conditional lookup
lists. Full instancing applies the first matching alternate feature, then unions
the matching lookup lists with the defaults when requested. Partial instancing
simplifies each condition and remaps its surviving store indices along with the
classic conditions. Parsing uses the existing condition-depth and expansion
budgets. Before owning feature variations, the layout handler rejects unsupported
major versions and checks that the declared record array fits in the table. This
check precedes generated range access so malformed counts return an error on
32-bit hosts as well. The native and i686 regressions exercise both malformed
counts and unsupported versions. The shared generated-reader overflow has its
own [font-codegen/read-fonts fix](https://github.com/googlefonts/fontations/pull/2392).
See [the integration regressions](../src/instance/layout/beyond64k_tests.rs).

Alternate feature parameters are resolved using their original feature tag, and
names referenced by retained alternates remain in the font. All eight GSUB/GPOS
reference comparisons now include complete parameter tables; their ordinary and
partial-subset references were refreshed after the upstream HarfBuzz fixes.

JSTF's `JstfMax` uses the original Offset16 positioning lookup list independently
of GPOS header selection. This alias is generated through the shared codegen
plan; Skera has no separate generated tables. Widened COLR glyph paints recurse
through the same paint traversal, and composite serialization checks the glyph
ID width before writing it.

## Validation and remaining limits

```sh
cargo test -p skera -p skrifa -p write-fonts -p read-fonts \
  -p font-types -p font-codegen --all-features
cargo clippy -p skera -p skrifa -p write-fonts -p read-fonts \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Tests include reference outputs, synthetic format regressions, composed
instances, and CLI cases. For the architecture refactor, also compare complete
outputs against the preceding stack tip for full instances, partial ranges,
unchanged ranges, and composition. Exact bytes are useful for a refactor check;
cross-project comparisons can require equivalent outlines, metrics,
references, and colors because packing can differ.

Cubic outlines use the upstream reader support; `spec_next` is retained as a
compatibility feature. Instancing the extended GLYF/GVAR and uppercase metrics families is not
implemented by this stack. Requests that change axes reject fonts containing
these tables, including mixed fonts with original tables, so authoritative
extended data cannot remain stale. A request with no axis limits still copies
them unchanged. Changes to axes referenced by retained VARC remain
restricted. Unknown tables pass through; private or
future variation formats need their own handlers. Owned rebuilding performance
remains an operation-specific review question.
