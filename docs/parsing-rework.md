# Reworking the parsing framework

*For the short version, see [parsing-rework-summary.md](parsing-rework-summary.md).*

Codegen emits this today, for real tables, behind a third mode. Nothing
existing uses it: the output goes to a parallel tree, so the framework can be
developed against tables that actually exist without touching the 80,000 lines
of generated code the crate ships.

**Where the code is**

| | |
| --- | --- |
| the framework | `read-fonts/src/exp/parse/` — `Bytes`, the traits, one array over four stores, `WithParent` |
| checking it | `read-fonts/src/exp/parse/sanitize.rs` and `fast_sanitize.rs`, behind features |
| the emitter | `font-codegen/src/exp.rs`, reached by `mode = "exp"` |
| generated output | `read-fonts/generated/exp/` — GPOS, layout, variations, fvar, CFF |
| the modules that include it | `read-fonts/src/exp/tables/` |
| tests | `read-fonts/src/exp/tables/tests.rs`, plus `exp/shapes.rs` for what the generated tables do not reach |

```
cargo run -p font-codegen -- resources/codegen_plan.toml   # regenerates everything, old and new
cargo test -p read-fonts --features sanitize
```

The existing 80,000 lines of generated output are byte-identical afterwards, and
write-fonts, skrifa and skera build unchanged.

**What is generated so far**: 100-odd types from codegen inputs that were not
modified for it, except where the input could not say what the spec says — see
below. That covers every shape in the design: value records, records nested two
deep, offset arrays that need read args, format groups, generic groups, a
variably sized record, and a table whose header shrinks when a count is zero.

The tests parse the spec's own example bytes and check the results against the
parser the crate ships, field for field. Traversal is deliberately not emitted.

## Contents

- [What codegen could not express](#what-codegen-could-not-express)
  - [GPOS `ValueRecord` — a record that needs its parent](#gpos-valuerecord--a-record-that-needs-its-parent)
  - [fvar `InstanceRecord` — a size the font declares](#fvar-instancerecord--a-size-the-font-declares)
  - [fvar `AxisInstanceArrays` — a table invented for the emitter](#fvar-axisinstancearrays--a-table-invented-for-the-emitter)
  - [CFF `INDEX` — a header that is sometimes shorter](#cff-index--a-header-that-is-sometimes-shorter)
  - [The pattern](#the-pattern)
- [The model](#the-model)
  - [1. One trait for tables, three for records](#1-one-trait-for-tables-three-for-records)
  - [2. Accessors return `Option`, and it does not nest](#2-accessors-return-option-and-it-does-not-nest)
  - [3. `WithParent<R>`](#3-withparentr)
  - [4. One array](#4-one-array)
  - [Where validation happens](#where-validation-happens)
- [What it looks like generated](#what-it-looks-like-generated)
  - [fvar `InstanceRecord`](#fvar-instancerecord)
  - [fvar `AxisInstanceArrays`](#fvar-axisinstancearrays)
  - [CFF `INDEX`](#cff-index)
- [What comes out](#what-comes-out)
- [Sanitize](#sanitize)
  - [Two passes, not one flag](#two-passes-not-one-flag)
  - [Bounds](#bounds)
  - [One check, not one per field](#one-check-not-one-per-field)
- [New DSL](#new-dsl)
- [What is left](#what-is-left)
- [Open questions](#open-questions)
- [Settled](#settled)

## What codegen could not express

The clearest measure of what was wrong is the list of tables that had to be
written by hand, or bent into a shape codegen could swallow. Each one is now
generated, and each needed one small, specific thing.

### GPOS `ValueRecord` — a record that needs its parent

Its size and contents come from a `ValueFormat` held by an ancestor, and its
device offsets are measured from the start of the enclosing *subtable*. So it
can be neither a zerocopy struct nor a record read from data sliced to itself:
slicing to the record throws away the base its offsets are measured against.

It was hand-written in two incompatible forms — an eager `ValueRecord` and a
resolved `Value` — with a `ValueContext` carrying what neither could reach.
Anything walking value records paid for fields it never read: a subsetter
scanning `PairPosFormat2` built two whole records per class pair just to OR
together a format.

**What it needed:** records located by `(parent, position)` rather than by a
slice. That is the centre of this design, not a special case — every record with
read args is read that way now, so `ValueRecord` is an ordinary `ComputedSize`
record whose size happens to be a popcount, and it is lazy for free.

### fvar `InstanceRecord` — a size the font declares

The spec says an instance record occupies `instanceSize` bytes, which may be
*larger* than the fields it holds, and that `postScriptNameID` is present only
when `instanceSize` leaves room for it. Neither is something a size computed by
adding up fields can say. It was hand-written in full.

**What it needed:** `#[record_size($instance_size)]`, and `#[if_fits]` for the
trailing field. The second only became expressible once a record knows its own
position — the condition is not about another field's value, it is about whether
*this* field's extent fits.

Still hand-written: `0xFFFF` means "no PostScript name", which no emitter could
infer. It is now a three-line wrapper over a generated accessor rather than a
reason to hand-write the record.

### fvar `AxisInstanceArrays` — a table invented for the emitter

fvar points at two consecutive arrays with a single offset, and codegen's offset
targets were "a table" or "one array". So a table was invented to be the
offset's target: a shim with no counterpart in the spec, which every caller had
to go through.

**What it needed:** `#[at_offset($field)]` — a field starts where another
field's offset points, and the fields after it chain on from there. The shim is
gone; `fvar.axes()` and `fvar.instances()` hang off the table.

### CFF `INDEX` — a header that is sometimes shorter

An INDEX is a count, and then — *only if the count is non-zero* — an `offSize`,
an offsets array and the data. An empty INDEX is two bytes. The declaration put
`offSize` in the fixed header, so `MIN_SIZE` came out as 3 and a legal empty
INDEX would not parse at all. `read-fonts/src/ps/cff/index.rs` carries a
hand-written `Empty` variant that never goes near the generated table.

**What it needed:** `#[if_nonzero($count)]`. Nothing else: `MIN_SIZE` already
stops at the first conditional field, so marking `offSize` conditional drops it
to 2 on its own.

### The pattern

Three of the four are the same thing said differently: **a record is not a small
table.** A table is read from data that begins at its own first byte and runs to
the end of its parent. A record lives at a position inside data it does not own,
and its offsets are measured from that data's start, not from its own. One trait
was describing the first and being made to describe the second.

The fourth — the CFF INDEX — is the other recurring shape: a table whose header
is not a fixed run of bytes. `MIN_SIZE` was already equipped for it; the DSL
just had no way to say so.

There is a related cost this design also removes, which is not about
expressiveness. Reading eagerly is expensive, and the things that walk font data
mostly want one field.

## The model

Four pieces. The trait definitions below are the real ones from
`read-fonts/src/exp/`. There are fewer traits than there are pieces: a good part
of the design turned out to be deciding what *not* to introduce.

### 1. One trait for tables, three for records

```rust
pub trait Table<'a>: Sized {
    type Args: Copy;
    const MIN_SIZE: usize;
    fn read_with_args(data: Bytes<'a>, args: Self::Args) -> Option<Self>;
}
```

**A table is a `Table`.** It is handed data beginning at its own first byte, so
its offsets are measured from byte zero of what it was given.

**A record is one of `FixedSize`, `ComputedSize` or `VariableSize`** — and nothing
else. There is no `Record` trait, because a record's two questions already have
homes: *how big is it* is the size taxonomy the crate already has, and *how do I
build one* is `ArrayElement::read`, which every array element has anyway.

How the size is known is the whole design. It decides what the reader is handed
and which store finds it:

| byte length | trait | given | store |
| --- | --- | --- | --- |
| compile time | `FixedSize` (+ `AnyBitPattern`) | `&'a Self`, borrowed | `SliceStore` |
| computed from read args | `ComputedSize` | parent + position + args | `StridedStore` |
| read from the data | `VariableSize` | a slice at itself | `VariableSizeStore` |

The middle row is the interesting one, and the only one that needs its parent:
a `ComputedSize` record is handed the enclosing table's data and its own
position within it, which is what lets it hold offsets measured against that
table. Earlier attempts bolted this on as a second arrangement beside the
ordinary one, which then had to be propagated up through containment — a record
holding such a record became one too. Here it is simply how a record with args
is read, so there is nothing to propagate.

```rust
pub trait ComputedSize {
    type Args: Copy;
    fn computed_size(args: Self::Args) -> usize;
}
```

The crate's existing trait with the `Result` dropped: a length too large to
represent saturates, and then fails the one extent check made by whoever locates
the elements.

`computed_size` takes only `Args` and never the data, and that is exactly what
separates it from `VariableSize`: a computed run is uniform, so it can be addressed
in `O(1)`, where a `VariableSize` run can only be walked. Be exact about which size
is meant — what varies is the length *in the font*, never the size of the Rust
type. Every one of these is an ordinary `Sized` value, which is why neither
"unsized" nor "dyn" would describe the middle row.

#### The one place the two categories touch

A `VariableSize` record is handed a slice at itself — which is exactly what a `Table`
is handed. So it implements `Table` as well, and reads through it.

That is not a mechanism borrowed for convenience. What makes a record different
from a table is *needing its parent*, and a self-describing element does not:
every variably sized thing in the crate holds no offsets at all. `morx`/`mort`'s
`Chain` and `Subtable` and `kerx`'s `Subtable` are already declared as tables;
`avar`'s `SegmentMaps`, `post`'s `PString` and `meta`'s `ScriptLangTag` are
declared records but hold nothing that needs a base. `exp/shapes.rs` walks an
`avar`-shaped one to check it.

If a variably sized record ever did hold a parent-relative offset, that would be
a genuine fourth case, since walking and offset resolution would have to happen
together. Nothing in OpenType asks for it.

#### What codegen emits for a computed record

Three items, none of which mention a record trait:

```rust
impl ComputedSize for ValueRecord<'_> {
    type Args = ValueFormat;
    fn computed_size(format: ValueFormat) -> usize {
        format.bits().count_ones() as usize * u16::RAW_BYTE_LEN
    }
}

impl<'a> ValueRecord<'a> {
    /// Inherent, because it is only ever called concretely: by the enclosing
    /// table, or by the preceding field locating the next one.
    pub fn at(parent: Bytes<'a>, pos: usize, format: ValueFormat) -> Self {
        Self { parent, pos, format }   // no reads at all
    }
}

impl<'a> ArrayElement<'a> for ValueRecord<'a> {
    type Args = ValueFormat;
    type Store = StridedStore<'a>;
    type Output = Self;
    fn read(store: StridedStore<'a>, item: usize, args: ValueFormat) -> Self {
        Self::at(store.data(), item, args)
    }
}
```

An earlier draft had a `ComputedRecord` trait carrying `computed_size` and `at`
together. It was dropped: `at` is only ever called concretely, so it does not
need to be dispatched, and the array's one generic use is covered by the two
bounds it already needs.

```rust
impl<'a, E> Array<'a, E>
where
    E: ArrayElement<'a, Store = StridedStore<'a>> + ComputedSize<Args = E::Args>,
{
    pub fn of_computed(data, start, count, args) -> Option<Self> { .. }
}
```

Neither the constructor nor `computed_size` can fail. Whether the bytes are
really there is established once, by whoever located the element, and asked
again by each field accessor, which returns `Option`.

Only **11** types in the crate would implement `ComputedSize` — every
`ComputedSize` impl that exists today, all of them GPOS, `variations`, or the
codegen tests. A `record` in the codegen DSL becomes one of four things:

| | given | how |
| --- | --- | --- |
| fixed size, no offsets | `&'a R` out of a slice | `FixedSize`, nothing else |
| fixed size, holds offsets | `&'a R` plus the base | `WithParent<'a, R>` |
| computed size | parent + position + args | `ComputedSize` + `ArrayElement` |
| variable size | its own slice | `Table` |

### 2. Accessors return `Option`, and it does not nest

Today an accessor can hand back any of `T`, `Option<T>`, `Result<T, ReadError>`,
`Option<Result<T, ReadError>>`, or `Result<Option<T>, ReadError>` depending on
whether the field is versioned, an offset, and nullable.

The new model keeps the crate's existing three-way rule and drops the `Result`
from it:

| field | returns | on short data |
| --- | --- | --- |
| covered by `MIN_SIZE` | bare `T` | cannot happen: `read` checked it |
| beyond `MIN_SIZE`, not conditional | bare `T` | empty or zero (`unwrap_or_default`) |
| conditional | `Option<T>` | `None` |

Only offset *resolution* adds an `Option` of its own, since an offset can be
null or unreadable whatever shape the field has.

The middle row is the one worth being deliberate about. An array whose extent is
not there reads as having no elements rather than as an error, which is what the
crate does today and what callers are written against: it keeps `?` out of every
loop that walks a table. It also means a `len()` can disagree with the count the
header claimed — that is already true and is the price of the row.

An offset accessor returns `Option<T>` whether or not the offset is declared
nullable, because **declaring an offset non-nullable says what a well formed
font contains, not what this one does.** A null in a non-nullable field is a
thing fonts in the wild actually do, and every caller already has to handle it:
**10 of the 11 places in skrifa, skera and write-fonts that read a `ReadError`
variant read `NullOffset`**, and every one of them discards the entry —
`=> continue` or `=> None`. The eleventh is a skrifa test, asserting on an error
skrifa itself substituted a few lines earlier.

There are 25 mentions of a `ReadError` variant in those crates, but the other 14
*construct* one rather than read it: skrifa raising an error of its own from an
`Option::ok_or` or from checked arithmetic, which this rework does not touch.
Six of those turn an `Option` into `Err(NullOffset)` — as in
`var_store.ok_or(ReadError::NullOffset)?` — which is this rework run backwards,
and gets shorter when the accessor hands back the `Option` to begin with.

So the two nullabilities have the same read-side signature, and `Nullable<O>`
survives only as the raw accessor's type, which is what the compile side reads.

The costs of a `Result` are not abstract. `Bytes` is a slice reference, so:

| | size |
| --- | --- |
| `Anchor<'a>` | 16 |
| `Option<Anchor<'a>>` | 16 |
| `Result<Anchor<'a>, ReadError>` | 24 |
| `Option<Result<Anchor<'a>, ReadError>>` | 24 |

`Option` rides the null-pointer niche and comes back in registers; `Result` does
not, because `ReadError` carries an `i64` and a `Tag`. These numbers are
asserted in a test in `exp/shapes.rs` rather than claimed here.

What is given up is diagnosis. `ReadError` stays at the boundary,
`FontRef::new` and `TableProvider`, where a caller does something with it.
Inside the accessors nobody reads the variant, and the ones who do read
`NullOffset` are better served by `None`.

### 3. `WithParent<R>`

Twenty five records hold an offset measured from the start of an enclosing
table. Most of them are fixed-size and zerocopy (`ScriptRecord`,
`FeatureRecord`, `MarkRecord`, `NameRecord`, `EncodingRecord`, `BaseGlyphPaint`)
and are handed out as `&'a [R]`, which is the fastest thing we have and worth
keeping. The price today is that their offset accessors take a `data` argument
the caller has to fetch from the correct ancestor and pass down by hand, and
getting it wrong is silent: the offset resolves against the wrong base and yields
a plausible, wrong table.

```rust
pub struct WithParent<'a, R> { record: &'a R, parent: Bytes<'a> }
```

The record is **borrowed**, and that turns out to decide where the bounds check
lives. Holding `R` by value means copying it out of the parent's data, which
needs a check, which the wrapper then has to be able to fail at, or to fabricate
a zeroed record to stand in for. A `&'a R` cannot be produced without the check
having already passed, so the check moves out to where the records are located
and happens once for the whole run. The wrapper is also a constant 24 bytes
however large the record is.

It derefs to `R`, so the plain accessors are unchanged; codegen emits the offset
accessors on the wrapper, where the base is already at hand.

```rust
impl MarkRecord {                       // plain fields
    pub fn mark_class(&self) -> u16;
    pub fn mark_anchor_offset(&self) -> Offset16;
}
impl<'a> WithParent<'a, MarkRecord> {   // offsets, no argument
    pub fn mark_anchor(&self) -> Option<Anchor<'a>>;
}
```

Note what this does *not* have to cover. A record whose size depends on read
args, a value record or a `Class1Record`, already holds the parent, because that
is how a computed record is read. `WithParent` is only the bridge for the zerocopy
case, and only for the ones that hold an offset:

- **a zerocopy record with no offsets** stays a plain `&'a [R]`, indexed and
  iterated as a slice, exactly as today. Nothing wraps it, because there is
  nothing to wrap it with;
- **a zerocopy record with offsets** is reached through
  `Array<'a, WithParent<'a, R>>`, and the underlying `&'a [R]` is still there.
  It is what the array's store holds, so the bulk view costs nothing and is not
  a second read.

The rule is per record type, decided by whether the record declares an offset,
not by where the record appears.

### 4. One array

An `ArrayStore` knows where the elements are; an `ArrayElement` knows how to
read one; `Array` is the pair.
Four stores cover everything: `StridedStore` (elements end to end at a computed
size), `SliceStore` (elements borrowed out of a zerocopy slice), `OffsetStore`
(elements behind a slice of offsets), `VariableSizeStore` (elements that describe
their own length, and so have no `len`).

`SliceStore` is what makes the borrow in `WithParent` work. Its `Item` is
already a `&'a R`, because the whole run was read and bounds checked as a slice
when the store was built, so the element impl has nothing left to read and
nothing to fail at. `StridedStore` could locate the same elements, but only by
handing out a position for the element to read from, which is a bounds check and
a copy per access and leaves the element owing an answer for the case it cannot
satisfy. Where the elements are is the store's business, and these two kinds of
element are found differently.

`ArrayElement::read` is infallible, and fallibility lives in
`ArrayElement::Output`:

| element | store | `Output` | `iter()` yields |
| --- | --- | --- | --- |
| a computed-size record | `StridedStore` | `Self` | records |
| a zerocopy record with offsets | `SliceStore` | `Self` | records |
| an offset, nullable or not | `OffsetStore` | `Option<T>` | `Option<T>` |
| a variably sized element | `VariableSizeStore` | `Option<T>` | `Option<T>` |

A record array's elements are all present, because the store checked the extent
when it was built, so saying `Result` per element was always noise. An offset's
target may genuinely be missing, so it says so, and the old
`Option<Result<T, ReadError>>` for nullable offset arrays becomes `Option<T>`.

Scalar fields, and zerocopy records that hold no offsets, stay `&'a [T]`.
"Unify the array types" means unify the lazy ones.

### Where validation happens

Two checks, both at construction:

1. `Table::read` checks that `MIN_SIZE` bytes are present. Nothing else, and
   nothing is read.
2. Building an array checks its whole extent once.

Everything downstream is total. That is not a new rule, it is what the table
accessors already do with `.ok().unwrap()`, but making it the stated contract is
what lets `at` be infallible and array iteration be a pointer bump.

**The framework never fabricates.** `Table::read`, `WithParent::at` and
`Array::of_computed` all return `Option`, and no constructor invents a record
that is not in the bytes.

**The generated accessors then apply the crate's three-way rule on top**, and
the middle row of that rule does default: a non-conditional field beyond
`MIN_SIZE` reads as empty or zero rather than as an `Option`. That is a
deliberate choice about ergonomics at the call site, not a claim about the data,
and it is what the crate does today. The emitter reaches for
`of_computed_or_empty` and `unwrap_or_default`; the framework underneath still
has the honest constructor, and a caller who wants it can use it.

The one `.unwrap()` the emitter writes is for a record embedded directly in a
table, where `MIN_SIZE` covers the field — the same assertion every scalar
accessor in the crate already makes.

A caller who has an `Option` and would rather have a value picks the fallback
themselves:

```rust
let record: MarkRecord = records.get(i).map(|rec| *rec).unwrap_or_default();
let record: RangeRecord = slice.get(i).copied().unwrap_or_default();
```

That idiom needs `Default` on the record, which needs `Default` on the raw
offset types; only `Nullable<O>` had one. `font-types` now gives `Offset16`,
`Offset24` and `Offset32` a null `Default`, which is the same premise the read
side already runs on and which that module's own doc comment already states:
*"Specific offset fields may or may not permit NULL values; however we assume
that errors are possible, and expect the caller to handle the `None` case."*

## What it looks like generated

All of the following is real output from `read-fonts/generated/exp/`, produced
from codegen inputs that were not changed for it.

A GPOS value record, hand-written today in two incompatible forms plus a
`ValueContext` to carry what neither can reach. Its size and contents come from
a `ValueFormat` held by an ancestor and its device offsets are measured from the
enclosing subtable, so it can be neither zerocopy nor read from data sliced to
itself:

```rust
impl<'a> SinglePosFormat1<'a> {
    pub fn value_record(&self) -> ValueRecord<'a> {
        let range = self.value_record_byte_range();
        ValueRecord::at(self.data, range.start, self.value_format())
    }
}
```

Nothing about that is special-cased. `ValueRecord` is still declared `extern`,
because the popcount and the `0x8000` device format are semantics the DSL cannot
state — but it is now an ordinary `ComputedSize` record that codegen calls like
any other, rather than a shape the framework has no room for.

Records nested two deep, the case that used to need positioned-ness propagated
through containment. There is nothing left to propagate:

```rust
impl<'a> PairPosFormat2<'a> {
    pub fn class1_records(&self) -> Array<'a, Class1Record<'a>> { .. }
}
impl<'a> Class1Record<'a> {
    pub fn class2_records(&self) -> Array<'a, Class2Record<'a>> { .. }
}
```

A record that holds an offset, reached through `WithParent` so the accessor
takes no `data` argument — and the raw `&'a [MarkRecord]` is still there, as the
array's store:

```rust
impl<'a> MarkArray<'a> {
    pub fn mark_records(&self) -> Array<'a, WithParent<'a, MarkRecord>> { .. }
}
impl<'a> WithParent<'a, MarkRecord> {
    pub fn mark_anchor(&self) -> Option<AnchorTable<'a>> { .. }
}
```

And what iterating looks like from outside. No `?`, no `unwrap`, no `Result` per
element:

```rust
table.value_records().iter().map(|r| (r.x_placement(), r.x_advance()))
```

### fvar `InstanceRecord`

The condition `#[if_fits]` compiles to, which is only expressible because a
record knows its own position:

```rust
pub fn post_script_name_id_byte_range(&self) -> Range<usize> {
    let start = self.coordinates_byte_range().end;
    let end = if start + NameId::RAW_BYTE_LEN
        <= self.pos + <Self as ComputedSize>::computed_size(self.args)
    {
        start + NameId::RAW_BYTE_LEN
    } else {
        start
    };
    start..end
}
```

Checked against the hand-written record on Vazirmatn and Amstelvar, field for
field. Neither font carries a `postScriptNameID`, so those only prove the field
is correctly *absent*; four synthetic records cover the present case, the
`0xFFFF` sentinel, and an `instanceSize` padded beyond the optional field.

### fvar `AxisInstanceArrays`

What replaced the shim:

```rust
#[at_offset($axis_instance_arrays_offset)]
#[count($axis_count)]
axes: [VariationAxisRecord],
#[count($instance_count)]
#[read_with($axis_count, $instance_size)]
instances: ComputedArray<InstanceRecord<'a>>,
```

The shim is gone; `fvar.axes()` and `fvar.instances()` hang off the table. The
byte-range chain just continues past the jump:

```rust
pub fn instances_byte_range(&self) -> Range<usize> {
    let start = self.axes_byte_range().end;
    ..
}
```

One wart: the offset still needs a declared target, so it is written
`Offset16<[VariationAxisRecord]>` with `#[offset_getter(axes)]` to suppress the
now-duplicate resolver. A bare untargeted offset would be cleaner and is
probably worth adding on its own account — several tables have offsets that are
resolved by hand.

### CFF `INDEX`

`#[if_nonzero($count)]` needed no new machinery beyond parsing: `MIN_SIZE`
already stops at the first conditional field, so it drops from 3 to 2 on its
own, and the range reads the way the spec does.

```rust
pub fn off_size_byte_range(&self) -> Range<usize> {
    let start = self.count_byte_range().end;
    let end = if self.count() != 0 { start + u8::RAW_BYTE_LEN } else { start };
    start..end
}
```

A two-byte empty INDEX now parses, with `off_size`, `offsets` and `data` all
`None`; a truncated non-empty one gives `None` from the accessor and is reported
by [sanitize](#sanitize) as `offsets: field extends past the end`.

## What comes out

- `FontRead` and `ReadArgs` become `Table`. `ComputedSize`, `VariableSize` and
  `FixedSize` stay as they are, and become the thing that decides how a record
  is read.
- `FontData` becomes `Bytes`: the same reads, returning `Option`, and only the
  eight the framework asks for.
- `ComputedArray`, `VarLenArray`, `ArrayOfOffsets`, `ArrayOfNullableOffsets` and
  `&[T]`-as-`FontRead` become `Array` plus four stores. There are no
  `ArrayOfOffsets` aliases either: an array of offsets is
  `Array<'a, OffsetTo<Target, Offset16>>`, and the offset type carries the
  declared nullability.
- `ResolveOffset` and `ResolveNullableOffset` become one `Resolve`.
- `ValueRecord`, `Value` and `ValueContext` become one generated record, and
  about 400 hand-written lines go with them.
- Every fabricated stand-in value: nothing invents a record, and no accessor
  reports a zero it did not read.
- Codegen's positioned-ness propagation pass.
- The "declare a single record as an array of one" workaround.
- `Option<Result<T, ReadError>>` everywhere.

## Sanitize

The read path gave up `Result`, and this is where it is bought back — not as
error handling threaded through every accessor, but as a pass that walks the
whole graph and reports everything wrong with it at once.

Nothing had to be invented for it. A field's extent *is* its generated
`*_byte_range`, and whether an offset resolves is what its resolved accessor
answers, so the pass is a mechanical walk of the same field list the accessors
come from and cannot drift from them.

```text
MarkBasePosFormat1.mark_array_offset → MarkArray.mark_records[0].mark_anchor_offset:
  offset 65520 does not resolve to a readable table

PairPosFormat2.class1_records:
  field extends past the end of the table (needs 261144, have 60)
```

That second one is the case the read path swallows on purpose: beyond
`MIN_SIZE`, a truncated array reads as empty and the accessor says nothing.
Something has to, and this is it.

### Two passes, not one flag

| | answers | strings | feature |
| --- | --- | --- | --- |
| `sanitize` | every problem, with the path and field name | 9.5kB of literals | `sanitize` |
| `is_sound` | yes or no, stopping at the first problem | none at all | `fast_sanitize` |

They are separate walks because the difference is not runtime behaviour, it is
what gets linked. Naming every table and field costs 568 string literals across
the four modules generated so far; a caller that only wants a yes or no should
link none of them, and with the walks split it does not. `sanitize` implies
`fast_sanitize`: a build that can diagnose should also be able to answer
quickly. A test asserts the two always agree on the verdict.

Both are off by default — together they are about a third of the framework's
generated code.

### Bounds

Offsets are a graph, so a walk needs bounds, and all of them have to hold
against a font built to be hostile.

- **Cycles and depth** are the [`Decycler`], a fixed-size array borrowed from
  skrifa (itself from HarfBuzz's `hb_decycler_t`). It allocates nothing, which
  is what keeps the fast path allocation-free, and bounding depth bounds the
  *Rust* stack too — the walk recurses through generated frames.
- **Fan-out** is a per-array element budget. The array's own extent is always
  checked, so a count larger than the data is still caught; the budget caps only
  how many elements are walked. A font claiming a million subtables costs one
  check.
- **Total work** is a table budget. A walk stopped by any limit reports the font
  as unsound rather than sound: a walk that did not finish cannot vouch for what
  it did not see.

Records take no part in the cycle guard. A record is reached by position inside
a table already entered, never through an offset, so it cannot be a node in the
graph.

### One check, not one per field

A field's `*_byte_range` is defined as the previous field's end, so evaluating
every field's range walks the chain once per field — quadratic in the number of
fields. Fields are laid out in order, so the last one's end is the largest, and
one comparison against it clears the whole table. Only when that fails is it
worth finding out which fields are short.

`#[at_offset]` breaks the ordering, so fields are split into runs at each jump
and each run is cleared separately.

[`Decycler`]: ../read-fonts/src/exp/parse/decycler.rs

## New DSL

Four attributes, each added for one table and each narrow:

| | for | |
| --- | --- | --- |
| `#[record_size($arg)]` | fvar | the record occupies what a read arg says, not the sum of its fields |
| `#[if_fits]` | fvar | a trailing field is present when the record's declared size leaves room |
| `#[at_offset($field)]` | fvar | this field starts where another field's offset points; later fields chain on |
| `#[if_nonzero($field)]` | CFF | this field, and everything after it, exists only when another is non-zero |

`#[if_fits]` and `#[if_nonzero]` are rejected with a `compile_error!` by the
existing emitter, so the two cannot drift.

## What is left

- **skrifa, skera, ift.** Nothing consumes the new tree yet. This is where the
  ~1,230 accessor call sites get tested, and the next thing worth doing.
- **write-fonts.** Not touched, and no longer expected to be a problem. An
  earlier draft called the compile side the one real risk, on the theory that
  `from_obj` distinguishes "malformed" from "absent" and would stop being able
  to. It does not. Every one of the 222 `ReadError` mentions in
  `write-fonts/generated` is the same line — the top-level `read_with_args`
  entry point — and inside `from_obj_ref` the error is already discarded three
  ways:

  ```rust
  Err(_) => OffsetMarker::default(),          // malformed offset → default table
  _      => NullableOffsetMarker::new(None),  // malformed nullable → None
  .filter_map(|x| x.map(..).ok())             // malformed array element → dropped
  ```

  A null offset and an unreadable one already produce the same output, which is
  the collapse the read side is making explicit. Minimal validate had already
  settled it: a truncated table parses, so a truncated font already round-trips
  as a smaller valid one.

  The migration should be a small net simplification. Two blanket impls,
  `FromObjRef<Result<U, ReadError>>` and `FromObjRef<Option<Result<U,
  ReadError>>>`, collapse into one `FromObjRef<Option<U>>`, and
  `.filter_map(|x| x.map(..).ok())` becomes `.flatten()`.

  Whether the compile side *should* reject malformed input is a real question,
  but it is one that exists today and is independent of this rework.
- **Traversal.** Deliberately not emitted, and not planned.
- **Upstreaming the new attributes.** `#[if_fits]` and `#[if_nonzero]` are
  exp-only. Fixing CFF's empty INDEX in the shipping crate means the compile
  side has to learn `#[if_nonzero]` too, since it changes how an empty INDEX is
  written back. Small and separable.
- **The other 57 codegen inputs.** Five are done. The emitter has not met
  `cmap`, `glyf`, `colr` or the AAT tables, so it will grow — and probably a
  few more narrow attributes with them.
- **Migration.** Accessors are inherent methods, so `-> Result<T, ReadError>`
  and `-> Option<T>` cannot coexist under one name, and tables cross-reference
  across modules — so a per-table flip breaks at the first cross-module offset.
  It is either flip read-fonts atomically and then chase the consumers, or emit
  both shapes with the new one suffixed and drop the suffix later.

## Open questions

- **Should a record array's `get` be `Option<T>` or `T`?** It is `Option<T>`
  here, `None` meaning "index past the end", which is the only failure left. A
  bare `T` with a clamped index would be faster still and much worse.
- **A bare, untargeted offset.** The DSL requires every offset to name a target,
  which is why fvar's has to claim one and then suppress the resolver. Several
  tables have offsets that are resolved by hand and would rather say so.
- **Coherence.** A blanket `ArrayElement` impl over "everything with a computed
  size" cannot coexist with `impl ArrayElement for OffsetTo<T, O>`, so each
  computed record carries its own three-line impl. That is fine for generated
  code and mildly annoying for a hand-written record. `WithParent` needs none:
  it is one concrete type, so its impl is blanket over `R`.
- **Does `Table` need `Args`?** About twenty tables take read args
  (`PairSet`, `Hmtx`, `Sbix`, `TupleVariationHeader`, ...). Most use them to
  size an array; `Feature`'s `feature_tag` is pure context passed through. Worth
  asking whether the second kind should be a different mechanism.
- **Should a null non-nullable offset fail sanitize?** Both passes say yes,
  because that is exactly the `Err(NullOffset)` the read side gave up. But fonts
  in the wild do it, so a caller gating on `is_sound` would reject them. Either
  the passes need a leniency knob, or `NullOffset` needs to be a warning rather
  than a problem.

## Settled

Things this document used to ask, with the answers the spike produced:

- **`Option<Array>` or an empty `Array` on truncation?** Empty. The crate
  already returns a value rather than an `Option` for a non-conditional field
  beyond `MIN_SIZE`, and preserving that keeps `?` out of every loop.
- **Is there a `Record` trait?** No. A record's two questions already have
  homes: how big it is, and `ArrayElement::read`. A `ComputedRecord` trait was
  written and then deleted, because `at` is only ever called concretely.
- **Does `WithParent` hold the record by value or by reference?** By reference.
  Borrowing moves the bounds check out to where the run is located, so nothing
  has to be fabricated for the case that cannot happen.
- **Do format groups and generic groups fall out?** Yes, both are emitted, and
  `Discriminant` came back to support the latter.
- **Can codegen handle fvar's `InstanceRecord`?** Yes, with two new attributes.
- **Can the errors be bought back after dropping `Result`?** Yes, and better
  than before: a pass that reports every problem at once with the path to it,
  rather than a `Result` that surfaces only the first field a caller happened to
  touch.
- **Should sanitize be one pass with a mode, or two?** Two. The difference is
  what gets linked, not what happens at runtime, and a mode flag cannot avoid
  linking the strings.
- **Is `demo.rs` still pulling its weight?** Not most of it. It was
  hand-written proof before the emitter existed; once GPOS was generated, most
  of it duplicated the generated tests on weaker evidence. It is now
  `shapes.rs`, a third the size, covering only what the generated tables do not
  reach.
