#![parse_module(read_fonts::codegen_test::conditions)]

#[skip_constructor] // because we don't use it, this avoids an unused warning
table MajorMinorVersion {
    #[version]
    #[default(MajorMinor::VERSION_1_1)]
    version: MajorMinor,
    always_present: u16,
    #[since_version(1.1)]
    if_11: u16,
    #[since_version(2.0)]
    if_20: u32,
}


flags u16 GotFlags {
    FOO = 0x0001,
    BAR = 0x0002,
    BAZ = 0x0004,
}

table FlagDay {
    volume: u16,
    flags: GotFlags,
    #[if_flag($flags, GotFlags::FOO)]
    foo: u16,
    #[if_flag($flags, GotFlags::BAR)]
    bar: u16,
    #[if_cond(any_flag($flags, GotFlags::BAZ, GotFlags::FOO))]
    baz: u16,
    #[if_cond(not_flag($flags, GotFlags::FOO))]
    qux: u16,
}

table FieldsAfterConditionals {
    flags: GotFlags,
    #[if_flag($flags, GotFlags::FOO)]
    foo: u16,
    always_here: u16,
    #[if_flag($flags, GotFlags::BAR)]
    bar: u16,
    #[if_flag($flags, GotFlags::BAZ)]
    baz: u16,
    also_always_here: u16,
    and_me_too: u16,
}
