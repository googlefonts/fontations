# codegen

This crate contains utilities used to generate code for parsing and
compiling various font tables.

As a quick overview, adding a new table works like this:
- the raw text of the table/record/bitfield objects is copied from the
  [Microsoft OpenType® docs][opentype] into a text file in `resources/raw_tables`.
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

- all objects are separated by a newline, and begin with `@OBJECT_TYPE`.
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

Codegen inputs can be annotated with various table and field attributes that
inform how the code is generated. These use the same syntax as proc-macro
attributes.

#### table attributes

The following annotations are supported on top-level objects:

- `#[skip_font_write]`: if present, we will not generate a `FontWrite`
  implementation for this type. This is useful if a type needs some manual
  processing before it can be compiled.
- `#[skip_from_obj]`: if present, we will not generate a `FromObjRef`
  implementation for this type.
- `#[read_args(name: type,+)]` if present, this type will be given an
  implementation of `FontReadWithArgs`, expecting the provided arguments. The
  provided names will be available to other attributes on this type, as if they
  were fields on the type itself.
- `#[phantom(T)]` Indicate that this type has a phantom generic parameter. This
  is used for common tables that contain offsets which point to different
  concrete types depending on the containing table, such as the `Layout`
  subtable shared between GPOS and GSUB.


#### field attributes
- `#[nullable]`: only allowed on offsets or arrays of offsets, and indicates
  that this field is allowed to be null. This changes the behaviour of getters,
  as well as validation and compilation code.
- `#[available(version)]`: indicates that a field only exists in a given version
  of the table.
- `#[skip_getter]`: if present, we will not generate a getter for this field.
  Used on things like padding fields.
- `#[offset_getter(method name)]`: only allowed on offsets or arrays of offsets.
  If present, we will not generate a method that resolves this offset, but will
  instead expect that one will be implemented manually, and will have the
  provided name.
- `#[offset_data(method name)]`: only on offset fields. If present, the provided
  'method name' must be implemented, and must return `FontData` that will be
  used to resolve this offset. Used in places where offsets are not resolved
  from the base of the containing table. Uncommon.
- `#[offset_adjustment(expr)]`: related to the above, but for encoding: the
  provided expression must evaluate to a `u32`, which will be subtracted from
  the computed offset during compilation.
- `#[version]`: May only be supplied for one field. If present, this field is
  treated as the 'version', used when determining the availability of versioned
  fields.
- `#[format = x]`: Indicates that this field is the format field of a
  multi-format table, and that it has the provided format value.
- `#[count(arg)]`: Only valid (and required) on arrays: the argument is either a literal,
  an expression, or a field on the type (preceded with a `$`).
- `#[compile(arg)]`: If present, this field will not be included in the compile
  type. The value may be either the literal 'skip', or an expression that
  evalutes to the field's type: the skip case is only expected in cases where
  there is a manual `FontWrite` impl, and the field does not make sense on the
  compile type.
- `#[compile_type(type)]`: specify an alternate type to be used in the struct
  generated for this type.
- `#[read_with(args,+)]`: specify that this field's type needs to be read with
  `FontReadWithArgs`, and passed the provided args. Args is a comma separated
  list of fields or input args to the type.
- `#[read_offset_with(args,+)]`: on offsets or arrays of offsets, indicates that
  the type referenced by this offset needs to be passed the provided args when
  it is read.
- `#[validate(arg)]`: arg is either the literal 'skip' or the name of a method.
  If the name of a method, that method will be called during validation, and can
  implement custom validation logic.
- `#[traverse_with(method name)]`: uncommon/hacky: provides a method name that
  will be called in traversal code to get the `FieldType` for this field.
- `#[to_owned(expr)]`: uncommon/hacky: provide an expression that will be used
  in `FromObjRef` to convert the parse type to the compile type.


[opentype]: https://docs.microsoft.com/en-us/typography/opentype/

### codegen plans

There is also the concept of a 'codegen plan', which is a simple toml file
describing a number of different operations to be run in parallel. This is
intended to be the general mechanism by which codegen is run.

See `../resources/codegen_plan.toml` for an example.
