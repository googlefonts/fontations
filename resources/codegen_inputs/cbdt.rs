#![parse_module(read_fonts::tables::cbdt)]

/// The [Color Bitmap Data](https://learn.microsoft.com/en-us/typography/opentype/spec/cbdt) table
#[tag = "CBDT"]
table Cbdt {
    /// Major version of the CBDT table, = 3.
    #[compile(3)]
    major_version: u16,
    /// Minor version of CBDT table, = 0.
    #[compile(0)]
    minor_version: u16,
}
