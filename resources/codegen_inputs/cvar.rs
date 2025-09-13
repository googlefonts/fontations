#![parse_module(read_fonts::tables::cvar)]

extern scalar TupleVariationCount;
extern record TupleVariationHeader;

/// The [cvar](https://learn.microsoft.com/en-us/typography/opentype/spec/cvar) table.
#[tag = "cvar"]
#[skip_from_obj]
#[skip_constructor]
table Cvar {
    /// Major/minor version number of the CVT variations table — set to (1,0).
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// A packed field. The high 4 bits are flags, and the low 12 bits
    /// are the number of tuple variation tables for this glyph. The
    /// number of tuple variation tables can be any number between 1
    /// and 4095.
    #[traverse_with(skip)]
    tuple_variation_count: TupleVariationCount,
    /// Offset from the start of the 'cvar' table to the serialized data.
    #[traverse_with(skip)]
    #[compile(skip)]
    data_offset: Offset16<FontData>,
    /// Array of tuple variation headers.
    #[count(..)]
    #[traverse_with(skip)]
    tuple_variation_headers: VarLenArray<TupleVariationHeader<'_>>,
}
