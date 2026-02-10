#![parse_module(read_fonts::tables::silf)]

#[tag = "Silf"]
table Silf {
    /// (major, minor) Version for the Silf table
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,

    #[since_version(3)]
    compiler_version: MajorMinor,

    sub_tables: u16,

    #[since_version(2)]
    #[skip_getter]
    #[compile(0)]
    _padding: u16,

    #[compile(self.compute_header_length())]
    start_offset: Offset32<SilfSubtable>,
}

table SilfSubtable {

}
