# codegen

This crate contains utilities used to generate code for parsing (and hopefully
compiling) various font tables.

As a quick overview, adding a new table works like this:
- the raw text of the table/record/bitfield objects is copied from the [Microsoft OpenType® docs][opentype] into a text file in `resources/raw_tables`.
- the `preprocessor` binary is run, converting this into a DSL with 'rust-like syntax'.
  This is written into a file in `resources/codegen_inputs`.
- if necessary, this syntax is annotated by hand with various attributes which
  instruct the codegen tool on how to treat various objects and fields.
- *this* file is then run through the actual `codegen` binary, which outputs
  (hopefully) valid rust code.
- the generated file is (generally) then written to `read/write-fonts/generated`.
- by convention, the contents of these generated files (which are not in the
  crate's module tree) are glob-imported into some file that *is* in that tree.
  Any hand-written implementation code lives in that file, which is only edited
  manually.
- you can also write a 'codegen plan', which is a toml file describing inputs
  and targets for the codegen tool, to be run in bulk. You can see one of these
  in `resources/codegen_plan.toml`.


## preprocessor

Inputs to the preprocessor look like this:

```
/// an optional comment for each top-level item
@table Gpos1_0
uint16      majorVersion       Major version of the GPOS table, = 1
uint16      minorVersion       Minor version of the GPOS table, = 0
Offset16    scriptListOffset   Offset to ScriptList table, from beginning of GPOS table
Offset16    featureListOffset  Offset to FeatureList table, from beginning of GPOS table
Offset16    lookupListOffset   Offset to LookupList table, from beginning of GPOS table

/// Part of [Name1]
@record LangTagRecord
uint16	length	Language-tag string length (in bytes)
Offset16	langTagOffset	Language-tag string offset from start of storage area (in bytes).

/// [Axis value table flags](https://docs.microsoft.com/en-us/typography/opentype/spec/stat#flags).
@flags(u16) AxisValueTableFlags
0x0001	OLDER_SIBLING_FONT_ATTRIBUTE	If set, this axis value table provides axis value information
0x0002	ELIDABLE_AXIS_VALUE_NAME	If set, do something else

@enum(u16) GlyphClassDef
1	Base	Base glyph (single character, spacing glyph)
2	Ligature	Ligature glyph (multiple character, spacing glyph)
3	Mark	Mark glyph (non-spacing combining glyph)
4	Component	Component glyph (part of single character, spacing glyph)
```

- all objects are separated by a newilne, and begin with `@OBJECT_TYPE`.
- record & table are currently interchangeable, but this may change, and  you
  should follow the spec.
- enum & flags require an explicit format
- this does not handle lifetimes, which will need to be added manually
- it also does not add annotations, which are necessary in any non-trivial case.
- you will generally need to do some cleanup.

run this like,

```sh
$ cargo run --bin preprocessor resources/raw_tables/my_table.txt > resources/codegen_inputs/my_table.rs
```

## codegen

The codegen tool reads in a file in rust-like syntax, and generates the final
rust source.

To run the tool on a single input:

```sh
# cargo run --bin=codegen resources/codegen_inputs/my_table.rs
```

This will write the generated source to stdout; you can redirect it as desired.

### annotations

TK once this stabalizes a bit
[opentype]: https://docs.microsoft.com/en-us/typography/opentype/

### codegen plans

There is also the concept of a 'codegen plan', which is a simple toml file
describing a number of different operations to be run in parallel. This is
intended to be the general mechanism by which codegen is run.
