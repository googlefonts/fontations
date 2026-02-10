#![parse_module(read_fonts::tables::glat)]

#[tag = "Glat"]
#[read_args(num_glyphs: u16)]
table Glat {
    /// (major, minor) Version for the Glat table
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,

    #[since_version(3.0)]
    #[compile(1)]
    output_octaboxes: u32,

    #[count($num_glyphs)]
    octaboxes: [OctaBox],

    #[count(..)]
    glyphs: [GlyphAttrRun],
}

record OctaBox<'a> {
    bitmap: u16,
    dn_min: u8,
    dn_max: u8,
    dp_min: u8,
    dp_max: u8,
    #[count(count_ones($bitmap))]
    sub_box: [SubBox],
}

record GlyphAttrRun<'a> {
    #[before_version(2)]
    start: u8,
    #[before_version(2)]
    length: u8,

    #[since_version(2)]
    start: u16,
    #[since_version(2)]
    length: u16,

    #[count($length)]
    attrs: [u16],
}

record SubBox {
    left: u8,
    right: u8,
    bottom: u8,
    top: u8,

    dn_min: u8,
    dn_max: u8,
    dp_min: u8,
    dp_max: u8,
}
