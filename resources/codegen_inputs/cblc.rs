#![parse_module(read_fonts::tables::cblc)]

extern record BitmapSize;

/// The [Color Bitmap Location](https://learn.microsoft.com/en-us/typography/opentype/spec/cblc) table
#[tag = "CBLC"]
table Cblc {
    /// Major version of the CBLC table, = 3.
    #[compile(3)]
    major_version: u16,
    /// Minor version of CBLC table, = 0.
    #[compile(0)]
    minor_version: u16,
    /// Number of BitmapSize records.
    #[compile(array_len($bitmap_sizes))]
    num_sizes: u32,
    /// BitmapSize records array.
    #[count($num_sizes)]
    bitmap_sizes: [BitmapSize],
}
