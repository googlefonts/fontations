#![parse_module(read_fonts::tables::eblc)]

extern record BitmapSize;

/// The [Embedded Bitmap Location](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc) table
#[tag = "EBLC"]
table Eblc {
    /// Major version of the EBLC table, = 2.
    #[compile(2)]
    major_version: u16,
    /// Minor version of EBLC table, = 0.
    #[compile(0)]
    minor_version: u16,
    /// Number of BitmapSize records.
    #[compile(array_len($bitmap_sizes))]
    num_sizes: u32,
    /// BitmapSize records array.
    #[count($num_sizes)]
    bitmap_sizes: [BitmapSize],
}
