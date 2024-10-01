#![parse_module(read_fonts::tables::hdmx)]

/// The [Horizontal Device Metrics](https://learn.microsoft.com/en-us/typography/opentype/spec/hdmx) table.
#[read_args(num_glyphs: u16)]
#[tag = "hdmx"]
table Hdmx {
    /// Table version number (set to 0).
    version: u16,
    /// Number of device records.
    num_records: u16,
    /// Size of device record, 32-bit aligned.
    size_device_record: u32,
    /// Array of device records.
    #[count($num_records)]
    #[read_with($num_glyphs)]
    records: ComputedArray<DeviceRecord<'a>>,
}

#[read_args(num_glyphs: u16)]
record DeviceRecord<'a> {
    /// Pixel size for following widths (as ppem).
    pixel_size: u8,
    /// Maximum width.
    max_width: u8,
    /// Array of glyphs (numgGlyphs is from the 'maxp' table).
    #[count($num_glyphs)]
    widths: [u8],
}
