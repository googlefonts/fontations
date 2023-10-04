#![parse_module(read_fonts::tables::ebdt)]

/// The [Embedded Bitmap Data](https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt) table
#[tag = "EBDT"]
table Ebdt {
    /// Major version of the EBDT table, = 2.
    #[compile(2)]
    major_version: u16,
    /// Minor version of EBDT table, = 0.
    #[compile(0)]
    minor_version: u16,
}