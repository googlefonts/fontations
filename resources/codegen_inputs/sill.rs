#![parse_module(read_fonts::tables::sill)]

#[tag = "Sill"]
table Sill {
    /// (major, minor) Version for the Sill table
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,

    #[compile(array_len($languages))]
    num_langs: u16,
    /// A power of two > num_langs
    next_power_of_two: u16,
    /// Rounded base-2 log of num_langs
    log: u16,
    /// Difference between next_power_of_two and num_langs
    power_diff: i16,

    #[count($num_langs)]
    languages: [Language],
}

record Language {
    language: Tag,
    #[compile(array_len($settings))]
    num_settings: u16,
    #[read_offset_with($num_settings)]
    settings_offset: Offset32<[SettingName]>,
}

record SettingName {
    value: u16,
    name: NameId,
}
