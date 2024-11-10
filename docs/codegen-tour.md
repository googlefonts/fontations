# Code generation in oxidize

This document is an attempt to describe in reasonable detail the general
architecture of the [`read-fonts`][] and [`write-fonts`][] crates, focusing
specifically on parts that are auto-generated.

> ***note***:
>
> at various points in this document I will make use of blockquotes (like this
one) to highlight
> particular aspects of the design that may be interesting, confusing, or
require refinement.

## contents

- [overview](#overview)
- [`read-fonts`](#read-fonts)
    - [the code we don't generate](#what-we-dont-generate)
        - [scalars and `BigEndian<T>`](#scalars-detour)
        - [`FontData`](#font-data)
    - [tables and records](#tables-and-records)
    - [tables](#read-tables)
        - [`FontRead` and `FontReadWithArgs`](#font-read-args)
        - [versioned tables](#versioned-tables)
        - [multi-format tables](#multi-format-tables)
        - [getters](#table-getters)
        - [offset getters](#offset-getters)
        - [offset data](#offset-data)
    - [records](#records)
        - [zerocopy](#zerocopy)
        - [copy-on-read](#copy-on-read)
        - [offsets in records](#offsets-in-records)
    - [arrays](#arrays)
    - [flags and enums](#flags-and-enums)
    - [traversal](#traversal)
- [`write-fonts`](#write-fonts)
    - [tables and records](#write-tables-records)
    - [fields and `#[compile(..)]`](#table-fields)
    - [offsets](#write-offsets)
    - [parsing and `FromTableRef`](#write-parsing)
    - [validation](#validation)
    - [compilation and `FontWrite`](#compilation)

## <a id="overview"></a> overview

These two crates can be thought of as siblings, and they both follow the same
basic high-level design pattern: they contain a set of generated types, mapping
*as closely as possible* to the types in the [OpenType spec][opentype],
alongside hand-written code that uses and is used by those types.

The [`read-fonts`][] crate is focused on efficient read access and parsing, and
the [`write-fonts`][] crate is focused on compilation. The two crates contain a
parallel `tables` module, with a nearly identical set of type definitions: for
instance, [both crates][read-name-record] [contain a][write-name-record] `tables::name::NameRecord` type.

We will examine each of these crates separately.

## <a id="read-fonts"></a> `read-fonts`

### <a id="what-we-dont-generate"></a> The code we *don't* generate

Although this writeup is focused specifically on the code we generate, that code
is closely entwined with code that we hand-write. This is a general pattern: we
manually implement some set of types and traits, which are then used in our
generated code.

All of the types which are used in codegen are reexported in the
[`codegen_prelude`][read-prelude] module; this is glob imported at the top of
every generated file.

We will describe various of these manually implemented types as we encounter
them throughout this document, but before we get started it is worth touching on
two cases: `FontData` and scalars / `BigEndian<T>`.

#### <a id="scalars-detour"></a> Scalars and `BigEndian<T>`

Before we dive into the specifics of the tables and records in `read-fonts`, I
want to talk briefly about how we represent and handle the [basic data types](ot-data-types)
of which records and tables are composed.

In the font file, these values are all represented in [big-endian][endianness]
byte order. When we access them, we will need to convert them to the native
endianness of the host platform. We also need to have some set of types which
exactly match the memory layout (including byte ordering) of the underlying font
file; this is necessary for us to take advance of zerocopy semantics (see the
[zerocopy section](#zerocopy) below.)

In addition to endianness, it is also sometimes the case that types will be
represented by a different number of bytes in the raw file than when are
manipulating them natively; for instance `Offset24` is represented as three
bytes on disk, but represented as a `u32` in native code.

This leads us to a situation where we require two distinct types for each
scalar: a native type that we will use in our program logic, and a
'raw' type that will represent the bytes in the font file (as well as some
mechanism to convert between them.)

There are various ways we could express this in Rust. The most straightforward
would be to just have two parallel sets of types: for instance alongside the
`F2Dot14` type, we might have `RawF2Dot14`, or `F2Dot14Be`. Another option might
be to have types that are generic over byte-order, such that you end up with
types like `U16<BE>` and `U16<LE>`.

I have taken a slightly different approach, which tries to be more ergonomic and
intuitive to the user, at the cost of having a slightly more complicated
implementation.

##### `BigEndian<T>` and `Scalar`

Our design has two basic components: a trait, `Scalar` and a type
`BigEndian<T>`, which look like this:

```rust
/// A trait for font scalars.
pub trait Scalar {
    /// The raw byte representation of this type.
    type Raw: Copy + AsRef<[u8]>;

    /// Create an instance of this type from raw big-endian bytes
    fn from_raw(raw: Self::Raw) -> Self;
    /// Encode this type as raw big-endian bytes
    fn to_raw(self) -> Self::Raw;
}

/// A wrapper around raw big-endian bytes for some type.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct BigEndian<T: Scalar>(T::Raw);
```

The `Scalar` trait handles conversion of a type to and from its raw representation
(a fixed-size byte array) and the `BigEndian` type is way of representing some
fixed number of bytes, and associating them with a concrete type; it has `get`
and `set` methods which read or write the underlying bytes, relying on the
`from_raw` and `to_raw` methods on Scalar.

This is a compromise. The `Raw` associated type is expected to always be a
fixed-size byte array; say `[u8; 2]` for a `u16`, or `[u8; 3]` for an `Offset24`.

Ideally, the scalar trait would look like,

```rust
trait Scalar {
    const RAW_SIZE: usize;
    fn from_raw(bytes: [u8; Self::RAW_SIZE]) -> Self;
    fn to_raw(self) -> [u8; Self::RAW_SIZE];
}
```

But this is not currently something we can express with Rust's generics,
although [it should become possible eventually](generic-const-exprs).

In any case: what this lets us do is avoid having two separate sets of types for
the 'raw' and 'native' cases; we have a single wrapper type that we use anytime
we want to indicate that a type is in its raw form. This has the additional
advantage that we can define new types in our generated code that implement
`Scalar`, and then those types can automatically work with `BigEndian`; this is
useful for things like custom enums and flags that are defined at various points
in the spec.

##### `FixedSize`

In addition to these two traits, we also have a [`FixedSize`][] trait, which is
implemented for all scalar types (and later, for structs consisting only of
scalar types). This trait consists of a single associated constant:

```rust
/// A trait for types that have a known, constant size.
pub trait FixedSize: Sized {
    /// The raw (encoded) size of this type, in bytes.
    const RAW_BYTE_LEN: usize;
}
```

This is implemented for both all the scalar values, as well as all their
`BigEndian` equivalents; and in both cases, the value of `RAW_BYTE_LEN` is the
size of the raw (big-endian) representation.

#### <a id="font-data"></a> `FontData`

The [`FontData`][] struct is at the core of all of our font reading code. It
represents a pointer to raw bytes, augmented with a bunch of methods for safely
reading scalar values from that raw data.

It looks approximately like this:

```rust
pub struct FontData<'a>(&'a [u8]);
```

And can be thought of as a specialized interface on top of a Rust byte
slice.This type is used extensively in the API, and will show up frequently in
subsequent code snippets.

### <a id="tables-and-records"></a> tables and records

In the [`read-fonts`][] crate, we make a distinction between *table* objects and
*record* objects, and we generate different code for each.

The distinction between a *table* and a *record* is blurry, but the
specification offers two "general criteria":

> - Tables are referenced by offsets. If a table contains an offset to a
> sub-structure, the offset is normally from the start of that table.
> - Records occur sequentially within a parent structure, either within a
> sequence of table fields or within an array of records of a given type. If a
> record contains an offset to a sub-structure, that structure is logically a
> subtable of the record’s parent table and the offset is normally from the start
> of the parent table.
>
> ([The OpenType font file][otff])

### <a id="read-tables"></a> tables

Conceptually, a table object is additional type information laid over a
`FontData` object (a wrapper around a rust byte slice (`&[u8]`), essentially
a pointer plus a length). It provides typed access to that tables fields.

Conceptually, this looks like:

```rust
pub struct MyTable<'a>(FontData<'a>);

impl MyTable<'_> {
    /// Read the table's first field
    pub fn format(&self) -> u16 {
        self.0.read_at(0)
    }
}
```

In practice, what we generate is slightly different: instead of
generating a struct for the table itself (and wrapping the data directly)
we generate a 'marker' struct, which defines the type of the table, and then we
combine it with the data via a `TableRef` struct.

The `TableRef` struct looks like this:

```rust
/// Typed access to raw table data.
pub struct TableRef<'a, T> {
    shape: T,
    data: FontData<'a>,
}
```

And the definition of the table above, using a marker type, would look something
like:

```rust
/// A marker type
pub struct MyTableMarker;

/// Instead of generating a struct for each table, we define a type alias
pub type MyTable<'a> = TableRef<'a, MyTableMarker>;

impl MyTableMarker {
    fn format_byte_range(&self) -> Range<usize> {
        0..u16::RAW_BYTE_LEN
    }
}

impl MyTable<'_> {
    fn format(&self) -> u16 {
        let range = self.shape.format_byte_range();
        self.data.read_at(range.start)
    }
}
```

To the user these two API are equivalent (you have a type `MyTable`, on which
you can call methods to read fields) but the 'marker' pattern potentially allows
for us to do some fancy things in the future (involving various cases where we
want to store a type separate from a lifetime).

> ***note:***
>
> there are also downsides of the marker pattern; in particular, currently
> the code we generate will only compile if it is part of the `read-fonts` crate
> itself. This isn't a major limitation, except that it makes certain kinds of
> testing harder to do, since we can't do fancy things like generate code that
> treated as a separate compilation unit, e.g. for use with the [`trybuild`][]
crate.

#### <a id="font-read-args"></a> `FontRead` & `FontReadWithArgs`

After generating the type definitions, the next thing we generate is an
implementation of one of [`FontRead`][] or [`FontReadWithArgs`][]. The
`FontRead` trait is used if a table is self-describing: that is, if the data in
the table can be fully interpreted without any external information. In some
cases, however, this is not possible. A simple example is the [`loca` table][loca-spec]:
the data for this table cannot be interpreted correctly without knowing the
number of glyphs in the font (stored in the `maxp` table) as well as whether the
format is long or short, which is stored in the `head` table.

> ***note***:
>
> The `FontRead` trait is similar the 'sanitize' methods in HarfBuzz: that is to
> say that it does not parse the data, but only ensures that it is well-formed.
> Unlike 'sanitize', however, `FontRead` is not recursive (it does not chase
> offsets) and it does not in anyway modify the structure; it merely returns an
> error if the structure is malformed.
>
> We will likely want to change the name of this method at some point, to
> clarify the fact that it is not exactly *reading*.

In either case, the generated table code is very similar.

For the purpose of illustration, let's imagine we have a table that looks like
this:

```rust
table Foob {
    #[version]
    version: BigEndian<u16>,
    some_val: BigEndian<u32>,
    other_val: BigEndian<u32>,
    flags_count: BigEndian<u16>,
    #[count($flags_count)]
    flags: [BigEndian<u16>],
    #[since_version(1)]
    versioned_value: BigEndian<u32>,
}
```

This generates the following code:

```rust
impl<'a> FontRead<'a> for Foob<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        let version: u16 = cursor.read()?;
        cursor.advance::<u32>(); // some_val
        cursor.advance::<u32>(); // other_val
        let flags_count: u16 = cursor.read()?;
        let flags_byte_len = flags_count as usize * u16::RAW_BYTE_LEN;
        cursor.advance_by(flags_byte_len); // flags
        let versioned_value_byte_start = version
            .compatible(1)
            .then(|| cursor.position())
            .transpose()?;
        version.compatible(1).then(|| cursor.advance::<u32>());
        cursor.finish(FoobMarker {
            flags_byte_len,
            versioned_value_byte_start,
        })
    }
}
```

Let's walk through this. Firstly, the whole process is based around a 'cursor'
type, which is simply a way of advancing through the input data on a
field-by-field basis. Where we need to know the value of a field in order to
validate subsequent fields, we read that field into a local variable.
Additionally, values that we have to compute based on other fields are currently
cached in the marker struct, although this is an implementation detail and may
change. Let's walk through this code, field by field:

- **version**: as this is marked with the `#[version]` attribute, we read the
  value into a local variable, since we will need to know the version when
  reading any versioned fields.
- **some_val**: this is a simple value, and we do not need to know what it is,
  only that it exists. We `advance` the cursor by the appropriate number of
  bytes.
- **other_val**: ditto. The compiler will be able to combine these two
  `advances` into a single operation.
- **flags_count**: This value is referenced in the `#[count]` attribute on the
  following field, and so we bind it to a local variable.
- **flags**: the `#[count]` attribute indicates that the length of this array is
  stored in the `flags_count` field. We determine the array length by
  multiplying that value by the size of the array member, and we advance the
  cursor by that number of bytes.
- **versioned_value**: this field is only available if the `version` field is >=
  to `1` (this is specified via the `#[since_version]` attribute). We record the
  current cursor position (as an `Option`, which will be `Some` only if the
  version is compatible) and then we advance the cursor by the size of the
  field's type.

Finally, having finished with each field, we call the `finish` method on the
cursor: this performs a final bounds check, and instantiates the table with the
provided marker.

> ***note***:
>
> The `FontRead` trait is currently doing a bit of a double duty: in the case of
> tables, it is expected to perform a very minimal validation (essentially just
> bounds checking) but in the case of records it serves as an actual parse
> function, returning a concrete instance of the type. It is possible that these
> two roles should be separated?

#### <a id="versioned-tables"></a> versioned tables

As hinted at above, for tables that are versioned (which have a version field,
and which have more than one known version value we do not generate a distinct
table per version; instead we generate a single table. For fields that are
available on all versions of a table, we generate getters as usual. For fields
that are only available on certain versions, we generate getters that return an
`Option` type, which will be `Some` in the case where that field is present for
the current version.

> ***note***:
>
> The way we determine availability is crude: it is based on the
> [`Compatible`][] trait, which is implemented for the various types which are
> used to represent versions. For types that represent their version as a
> (major, minor) pair, we consider a version to be compatible with another version
> if it has the same major number and a greater-than-or-equal minor number. For
> versions that are a single value, we consider them compatible if they are
> greater-than-or-equal. If this ends up being inadequate, we can revisit it.

#### <a id="multi-format-tables"></a> multi-format tables

Some tables have multiple possible 'formats'. The various formats of a table
will all share an initial 'format' field (generally a `u16`) which identifies
the format, but the rest of their fields may differ.

For tables like this, we generate an enum that contains a variant for each of
the possible formats. For this to work, each different table format
must declare its table field in the input file:

```rust
table MyTableFormat1 {
    #[format = 1]
    table_format: BigEndian<u16>,
    my_val: BigEndian<u16>,
}
```

The `#[format = 1]` attribute on the field of `MyTableFormat1` is an important
detail, here. This causes us to implement a private trait, `Format`, like this:

```rust
impl Format<u16> for MyTableFormat1 {
    const FORMAT: u16 = 1;
}
```

You then also declare that you want to create an enum, providing an explicit
format, and listing which tables should be included:

```rust
format u16[@N] MyTable {
    Format1(MyTableFormat1),
    Format2(MyTableFormat2),
}
```

the 'format' keyword is followed by the type that represents the format, and
optionally a position  at which to read it (indicated by the '@' token, followed
by an unsigned integer literal.) In the vast majority of cases this can be
omitted, and the format will be read from the first position in the table.

We will then generate an enum, as well as a `FontRead` implementation: this
implementation will read the format off of the front of the input data, and then
instantiate the appropriate variant based on that value. The generated
implementation looks like this:

```rust
impl<'a> FontRead<'a> for MyTable<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let format: u16 = data.read_at(0)?;
        match format {
            MyTableFormat1::FORMAT => Ok(Self::Format1(FontRead::read(data)?)),
            MyTableFormat2::FORMAT => Ok(Self::Format2(FontRead::read(data)?)),
            other => Err(ReadError::InvalidFormat(other.into())),
        }
    }
}
```

This trait-based approach has a few nice properties: we ensure that
we don't accidentally have formats declared with different types, and we also
ensure that if we accidentally provide the sae format value for two different
tables, we will at least see a compiler warning.


#### <a id="table-getters"></a> getters

For each field in the table, we generate a getter method. The exact behaviour of
this method depends on the type of the field. If the field is a *scalar* (that
is, if it is a single raw value, such as an offset, a `u16`, or a [`Tag`][])
then this getter reads the raw bytes, and then returns a value of the
appropriate type, handling big-endian conversion. If it is an array, then the
getter returns an array type that wraps the underlying bytes, which will be read
lazily on access.

Alongside the getters we also generate, for each field, a
method on the marker struct that returns the start and end positions of each
field. These are defined in terms of one another: the end position of field `N`
is the start of field `N+1`. These fields are defined in a process that echoes
how the table is validated, where we build up the offsets as we advance through
the fields. This means we avoid the case where we are calculating offsets from
the start of the table, which should lead to more auditable code.

#### <a id="offset-getters"></a> offset getters

For fields that are either offsets or arrays of offsets, we generate *two*
getters: a raw getter that returns the raw offset, and an 'offset getter' that
resolves the offset into the concrete type that is referenced. If the field is
an array of offsets, this returns an *iterator* of resolved offsets. (This is a
detail that I would like to change in the future, replacing it with some sort of
lazy array-like type.)

For instance, if we have a table which contains the following:

```rust
table CoverageContainer {
    coverage_offset: BigEndian<Offset16<CoverageTable>>,
    class_count: BigEndian<u16>,
    #[count($class_count)]
    class_def_offsets: [BigEndian<Offset16<ClassDef>>],
}
```

we will generate the following methods:

```rust
impl<'a> ClassContainer<'a> {
    pub fn coverage_offset(&self) -> Offset16 { .. }
    pub fn coverage(&self) -> Result<CoverageTable<'a>, ReadError> { .. }
    pub fn class_def_offsets(&self) -> &[BigEndian<Offset16>] { .. }
    pub fn class_defs(&self) ->
        impl Iterator<Item = Result<ClassDef<'a>, ReadError>> + 'a { .. }
```

#####  custom offset getters, #[read_offset_with]

Every offset field requires an offset getter, but the getters generated by
default only work with types that implement `FontRead`. For types that require
args, you can use the `#[read_offset_with($arg1, $arg1)]` attribute to indicate
that this offset needs to be resolved with `FontReadWithArgs`, which will be
passed the arguments specified; these can be either the names of fields on the
containing table, or the name of arguments passed into this table through its
*own* `FontReadWithArgs` impl.

In special cases, you can also manually implement this getter by using the
`#[offset_getter(method)]` attribute, where `method` will be a method you
implement on the type that handles resolving the offset via whatever custom
logic is required.

##### <a id="offset-data"></a> offset data

How do we keep track of the data from which an offset is resolved? A happy
byproduct of how we represent tables makes this generally trivial: because a
table is just a wrapper around a chunk of bytes, and since most offsets are
resolved relative to the start of the containing table, we can resolve offsets
from directly from our inner data.

In tricky cases, where offsets are not relative to the start of the table, we
there is a custom `#[offset_data]` attribute, where the user can specify a
method that should be called to get the data against which a given offset should
be resolved.

### <a id="records"></a> records

Records are components of tables. With a few exceptions, they almost always
exist in arrays; that is, a table will contain an array with some number of
records.

When generating code for records, we can take one of two paths. If the record
has a fixed size, which is known at compile time, we generate a "zerocopy"
struct; and if not, we generate a "copy on read" struct. I will describe these
separately.

#### <a id="zerocopy"></a> zerocopy

When a record has a known, constant size, we declare a struct which has fields
which exactly match the raw memory layout of the record.

As an example, the root *TableDirectory* of an OpenType font contains a
*TableRecord* type, defined like this:

| Type       | Name     | Description                         |
| ---------- | -------- | ----------------------------------- |
| `Tag`      | tableTag | Table identifier.                   |
| `uint32`   | checksum | Checksum for this table.            |
| `Offset32` | offset   | Offset from beginning of font file. |
| `uint32`   | length   | Length of this table.               |

For this type, we generate the following struct:

```rust
#[repr(C)]
#[repr(packed)]
pub struct TableRecord {
    /// Table identifier.
    pub tag: BigEndian<Tag>,
    /// Checksum for the table.
    pub checksum: BigEndian<u32>,
    /// Offset from the beginning of the font data.
    pub offset: BigEndian<Offset32>,
    /// Length of the table.
    pub length: BigEndian<u32>,
}

impl FixedSize for TableRecord {
    const RAW_BYTE_LEN: usize = Tag::RAW_BYTE_LEN
        + u32::RAW_BYTE_LEN
        + Offset32::RAW_BYTE_LEN
        + u32::RAW_BYTE_LEN;
}
```
Some things to note:

- The `repr` attribute specifies the layout and and alignment of the struct.
  `#[repr(packed)]` means that the generated struct has no internal padding,
  and that the alignment is `1`. (`#[repr(C)]` is required in order to use
  `#[repr(packed)]`, and it basically means "opt me out of the default
  representation").
- All of the fields are `BigEndian<_>` types. This means that their internal
  representation is raw, big-endian bytes.
- The `FixedSize` trait acts as a marker, to ensure that this type's fields
  are themselves all also `FixedSize`.

Taken altogether, we get a struct that can be 'cast' from any slice of bytes
of the appropriate length. More specifically, this works for arrays: we can take
a slice of bytes, ensure that its length is a multiple of `T::RAW_BYTE_LEN`,
and then convert that to a Rust slice of the appropriate type.

#### <a id="copy-on-read"></a> copy-on-read

In certain cases, there are records which do not have a size known at compile
time. This happens frequently in the GPOS table. An example is the
[`PairValueRecord`][] type: this contains two `ValueRecord` fields, and the size
(in bytes) of each of these fields depends on a `ValueFormat` that is stored in
the parent table.

As such, we cannot know the size of `PairValueRecord` at compile time, which
means we cannot cast it directly from bytes. Instead, we generate a 'normal'
struct, as well as an implementation of `FontReadWithArgs` (discussed in the
table section.) This looks like,

```rust
pub struct PairValueRecord {
    /// Glyph ID of second glyph in the pair
    pub second_glyph: BigEndian<GlyphId>,
    /// Positioning data for the first glyph in the pair.
    pub value_record1: ValueRecord,
    /// Positioning data for the second glyph in the pair.
    pub value_record2: ValueRecord,
}

impl<'a> FontReadWithArgs<'a> for PairValueRecord {
    fn read_with_args(
        data: FontData<'a>,
        args: &(ValueFormat, ValueFormat),
    ) -> Result<Self, ReadError> {
        let mut cursor = data.cursor();
        let (value_format1, value_format2) = *args;
        Ok(Self {
            second_glyph: cursor.read()?,
            value_record1: cursor.read_with_args(&value_format1)?,
            value_record2: cursor.read_with_args(&value_format2)?,
        })
    }
}
```

Here, in our 'read' impl, we are actually instantiating an instance of our type,
copying the bytes as needed.

In addition, we also generate an implementation of the `ComputeSize` trait; this
is analogous to the `FixedSize` trait, which represents the case of a type that
has a size which can be computed at runtime from some set of arguments.

#### <a id="offsets-in-records"></a> offsets in records

Records, like tables, can contain offsets. Unlike tables, records do not have
access to the raw data against which those offsets should be resolved. For the
purpose of consistency across our geneerated code, however, it *is* important
that we have a consistent way of resolving offsets contained in records, and we
do: you have to pass it in.

Where an offset getter on a table might look like,

```rust
fn coverage(&self) -> Result<CoverageTable<'a>, ReadError>;
```

The equivalent getter on a record looks like,

```rust
fn coverage(&self, data: FontData<'a>) -> Result<CoverageTable<'a>, ReadError>;
```

This... honestly, this is not great ergonomics. It is, however, simple, and is
relied on by codegen in various places, and when we're generating code we aren't
too bothered by how ergonomic it is. We might want to revisit this at some
point; one simple improvement would be to have the caller pass in the parent
table, but I'm not sure how this would work in cases where a type might be
referenced by multiple parents. Another option would be to have some kind of
fancy `RecordData` struct that would be a thin wrapper around a record plus the
parent data, and which would implement the record getters, but deref to the
record otherwise.... I'm really not sure.

### <a id="arrays"></a> arrays

The code we generate to represent an array varies based on what we know about
the size and contents of the array:

- if the contents of an array have a fixed uniform size, known at compile time, then we
  represent the array as a rust slice: `&[T]`. This is true for all scalars
  (including offsets) as well as records that are composed of a fixed number of
  scalars.
- if the contents of an array have a uniform size, but the size can only be
  determined at runtime, we represent the array using the [`ComputedArray`][] type.
  This requires the inner type to implement [`FontReadWithArgs`][], and the
  array itself wraps the raw bytes and instantiates its elements lazily as they
  are accessed. As an example, the length of a `ValueRecord` depends on the
  specific associated `ValueFormat`.
  ```rust
  table SinglePosFormat2 {
      // some fields omitted
      value_format: BigEndian<ValueFormat>,
      value_count: BigEndian<u16>,
      #[count($value_count)]
      #[read_with($value_format)]
      value_records: ComputedArray<ValueRecord>,
  }
  ```
- finally, if an array contains elements of non-uniform sizes, we use the
  [`VarLenArray`][] type. This requires the inner type to have a leading field
  which contains the length of the item, and this array does not allow for
  random access; an example is the array of Pascal-style strings in the ['post'
  table][pstring]. The inner type must implement the implement the [`VarSize`][]
  trait, via which it indicates the type of its leading length field. An example
  of this pattern is the array of Pascal-style strings in the 'post' table;
  the first byte of these strings encodes the length, and so we represent them
  in a `VarLenArray`:

  ```rust
  table Post {
      // some fields omitted
      #[count(..)]
      #[since_version(2.0)]
      string_data: VarLenArray<PString<'a>>,
  }
  ```

### <a id="flags-and-enums"></a> flags and enums

On top of tables and records, we also generate code for various defined flags
and enums. In the case of flags, we generate implementations based on the
[`bitflags`][] crate, and in the case of enums, we generate a rust enum.
These code paths are not currently very heavily used.

### <a id="traversal"></a> traversal

There is one last piece of code that we generate in `read-fonts`, and that is
our 'traversal' code.

This is experimental and likely subject to significant change, but the general
idea is that it is a mechanism for recursively traversing a graph of
tables, without needing to worry about the specific type of any *particular* table. It
does this by using [trait objects][trait-objects], which allow us to refer to
multiple distinct types in terms of a trait that they implement. The core of this is the
[`SomeTable`][] trait, which is implemented for each table; through this, we can
get the name of a table, as well as iterate through that tables fields.

For each field, the table returns the name of the field (as a string) along with
some *value*; the set of possible values is covered by the [`FieldType`][]
enum. Importantly, the table resolves any contained offsets, and returns the
referenced tables as `SomeTable` trait objects as well, which can then also be
traversed recursively.

We do not currently make very heavy use of this mechanism, but it *is* the basis
for the generated implementations of the `Debug` trait, and it is used in the
[otexplorer][] sample project.

## <a id="write-fonts"></a> `write-fonts`

The `write-fonts` crate is significantly simpler than the `read-fonts` crate
(currently less than half the total lines of generated code) and because it does
not have to deal with the specifics of the memory layout or worry about avoiding
allocation, the generated code is generally more straightforward.

### <a id="write-tables-records"></a> tables and records

Unlike in `read-fonts`, which generates significantly different code for tables
and records (as well as very different code based on whether a record is
zerocopy or not) the `write-fonts` crate treats all tables and records as basic
Rust structs.

As in `read-fonts` we generate enums for tables that have multiple formats, and
likewise we generate a single struct for tables that have versioned fields, with
version-dependent fields represented as `Option` types.

> ***note***:
>
> This pattern is a bit more annoying in write-fonts, and we may want to revisit
> it at some point, or at least improve the API with some sort of builder
> pattern.

#### <a id="table-fields"></a> fields and `#[compile(..)]`

Where the types in `read-fonts` generally contain the exact fields described in
the spec, this does not always make sense for the `write-types`. A simple
example is fields that contain the count of an array. This is useful in
`read-fonts`, but in `write-fonts` it is redundant, since we can determine the
count from the array itself. The same is true of things like the `format` field,
which we can determine from the type of the table, as well as version numbers,
which we can choose based on the fields present on the table.

In these cases, the `#[compile(..)]` attribute can be used to provide a computed
value to be written in the place of this field. The provided value can be a
literal or an expression that evaluates to a value of the field's type.

If a field has a `#[compile(..)]` attribute, then that field will be omitted in
the generated struct.

#### <a id="write-offsets"></a> offsets

Fields that are of the various offset types in the spec are represented in
`write-fonts` as [`OffsetMarker`] types. These are a wrapper around an
`Option<T>` where `T` is the type of the referenced subtable; they also have a
const generic param `N` that represents the width of the offset, in bytes.

During compilation (see the section on [`FontWrite`][#fontwrite], below) we use
these markers to record the position of offsets in a table, and to associate
those locations with specific subtables.

#### <a id="write-parsing"></a> parsing and [`FromTableRef`][]

There is generally 1:1 relationship between the generated types in `read-fonts` and
`write-fonts`, and you can convert a type in `read-fonts` to a corresponding
type in `write-fonts` (assuming the default "parsing" feature is enabled) via
the [`FromObjRef`][] and [`FromTableRef`][] traits. These are modeled on the
[`From` trait][from-trait] in the Rust prelude, down to having a pair of
companion `IntoOwnedObj` and `IntoOwnedTable` traits with blanket impls.

The basic idea behind this approach is that we do not generate separate parsing
code for the types in `write-fonts`; we leave the parsing up to the types in `read-fonts`,
and then we just handle conversion from these to the write types.

The more general of these two traits is [`FromObjRef`][], which is implemented
for every table and record. It has one method, `from_obj_ref`, which takes some
type from `read-fonts`, as well as `FontData` that is used to resolve any
offsets. If the type is a table, it can ignore the provided data, since it
already has a reference to the data it will use to resolve any contained
offsets, but if it is a record than it must use the input data in order to
recursively convert any contained offsets.

In their `FromObjRef` implementation, tables provide pass their own data down to
any contained records as required.

The `FromTableRef` trait is simply a marker; it indicates that a given object
does not require any external data.

In any case, all of these traits are largely implementation details, and you
will rarely need to interact with them directly: if because if a type implements
`FromTableRef`, then we *also* generate an implementation of the `FontRead`
trait from `read-fonts`. This means that all of the self-describing tables in
`write-fonts` can be instantiated directly from raw bytes in a font file.

#### <a id="validation"></a> Validation

One detail of `FromObjRef` and family is that these traits are *infallible*;
that is, if we can parse a table at all, we will always successfully convert it
to its owned equivalent, even if it contains unexpected null offsets, or has
subtables which cannot be read. This means that you can read and modify a table
that is malformed.

We do not want to *write* tables that are malformed, however, and we also want
an opportunity to enforce various other constraints that are expressed in the
spec, and for this we have the [`Validate`][] trait. An implementation of this
trait is generated for all tables, and we automatically verify a number of
conditions: for instance that offsets which should not be null contain a value,
or that the number of items in a table does not overflow the integer type that
stores that table's length. Additional validation can be performed on a
per-field basis by providing a method name to the `#[validate(..)]` attribute;
this should be an instance method (having a `&self` param) and should also
accept an additional 'ctx' argument, of type [`&mut ValidateCtx`][validation-ctx] which is used
to report errors.

### <a id="compilation"></a> compilation and [`FontWrite`][]

Finally, for each type we generate an implementation of the [`FontWrite`][] trait,
which looks like:

```rust
pub trait FontWrite {
    fn write_into(&self, writer: &mut TableWriter);
}
```

The `TableWriter` struct has two jobs: it records the raw bytes representing the
data in this table or record, as well as recording the position of offsets, and
the entities they point do.

The implementation of this type is all hand-written, and out of the scope of
this document, but the implementations of `FontWrite` that we generate are
straight-forward: we walk the struct's fields in order (computing a value if the
field has a `#[compile(..)]` attribute) and recursively call `write_into` on
them. This recurses until it reaches either an `OffsetMarker` or a scalar type;
in the first case we record the position and size of the offset in the current
table, and then recursively write out the referenced object; and in the latter
case we record the big-endian bytes themselves.


## fin

This document represents a best effort at capturing the most important details
of the code we generate, as of October 2022. It is likely that things will
change over time, and I will endeavour to keep this document up to date. If
anything is unclear or incorrect, please open an issue and I will try to
clarify.




[`read-fonts`]: https://docs.rs/read-fonts/
[`write-fonts`]: https://docs.rs/write-fonts/
[opentype]: https://learn.microsoft.com/en-us/typography/opentype/spec/
[read-name-record]: https://docs.rs/read-fonts/latest/read_fonts/tables/name/struct.NameRecord.html
[write-name-record]: https://docs.rs/write-fonts/latest/write_fonts/tables/name/struct.NameRecord.html
[`trybuild`]: https://docs.rs/trybuild/latest/trybuild/
[`FontRead`]: https://docs.rs/read-fonts/latest/read_fonts/trait.FontRead.html
[`FontReadWithArgs`]: https://docs.rs/read-fonts/latest/read_fonts/trait.FontReadWithArgs.html
[loca-spec]: https://learn.microsoft.com/en-us/typography/opentype/spec/loca
[`Tag`]: https://learn.microsoft.com/en-us/typography/opentype/spec/ttoreg
[otff]: https://learn.microsoft.com/en-us/typography/opentype/spec/otff
[`PairValueRecord`]: https://learn.microsoft.com/en-us/typography/opentype/spec/gpos#pairValueRec
[`bitflags`]: https://docs.rs/bitflags/latest/bitflags/
[ot-data-types]: https://learn.microsoft.com/en-us/typography/opentype/spec/otff#data-types
[endianness]: https://en.wikipedia.org/wiki/Endianness
[`Compatible`]: https://docs.rs/font-types/latest/font_types/trait.Compatible.html
[trait-objects]: http://doc.rust-lang.org/1.64.0/book/ch17-02-trait-objects.html
[`SomeTable`]: https://docs.rs/read-fonts/latest/read_fonts/traversal/trait.SomeTable.html
[`FieldType`]: https://docs.rs/read-fonts/latest/read_fonts/traversal/enum.FieldType.html
[otexplorer]: https://github.com/cmyr/fontations/tree/main/otexplorer
[`OffsetMarker`]: https://docs.rs/write-fonts/latest/write_fonts/struct.OffsetMarker.html
[`FromObjRef`]: https://docs.rs/write-fonts/latest/write_fonts/from_obj/trait.FromObjRef.html
[`FromTableRef`]: https://docs.rs/write-fonts/latest/write_fonts/from_obj/trait.FromTableRef.html
[from-trait]: http://doc.rust-lang.org/1.64.0/std/convert/trait.From.html
[`Validate`]: https://docs.rs/write-fonts/latest/write_fonts/validate/trait.Validate.html
[validation-ctx]: https://docs.rs/write-fonts/latest/write_fonts/validate/struct.ValidationCtx.html
[`FontWrite`]: https://docs.rs/write-fonts/latest/write_fonts/trait.FontWrite.html
[`FixedSize`]: https://docs.rs/font-types/latest/font_types/trait.FixedSize.html
[generic-const-exprs]: https://github.com/rust-lang/rust/issues/60551#issuecomment-917511891
[read-prelude]: https://github.com/cmyr/fontations/blob/main/read-fonts/src/lib.rs#L42
[`FontData`]: https://docs.rs/read-fonts/latest/read_fonts/struct.FontData.html
[`ComputedArray`]: https://docs.rs/read-fonts/latest/read_fonts/array/struct.ComputedArray.html
[`VarLenArray`]: https://docs.rs/read-fonts/latest/read_fonts/array/struct.VarLenArray.html
[`VarSize`]: https://docs.rs/read-fonts/latest/read_fonts/trait.VarSize.html
[pstring]: https://learn.microsoft.com/en-us/typography/opentype/spec/post#version-20

