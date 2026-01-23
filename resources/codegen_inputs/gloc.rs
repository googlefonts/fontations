#![parse_module(read_fonts::tables::gloc)]

#[tag = "Gloc"]
table Gloc {
    /// (major, minor) Version for the Gloc table
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,

    flags: GlocFlags,

    num_attrs: u16,

    #[if_flag($flags, GlocFlags::NEED_LONG_FORMAT)]
    #[count(..)]
    offsets: [u32],

    #[if_cond(not_flag($flags, GlocFlags::NEED_LONG_FORMAT))]
    #[count(..)]
    offsets: [u16],
}

flags u16 GlocFlags {
    NEED_LONG_FORMAT = 0x0001,
    ATTR_NAMES = 0x0002,
}
