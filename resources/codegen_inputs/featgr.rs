#![parse_module(read_fonts::tables::featgr)]

/// The graphite feature table - this is similar but not identical to apple's feature table.
#[tag = "Feat"]
table Feat {
    /// (major, minor) Version for the Feat table
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,

    #[compile(array_len($features))]
    num_features: u16,

    #[skip_getter]
    #[compile(0)]
    _padding1: u16,
    #[skip_getter]
    #[compile(0)]
    _padding2: u32,

    #[count($num_features)]
    features: [Feature]
}

record Feature {
    #[since_version(3)]
    feat_id: u32,
    #[before_version(3)]
    feat_id: u16,

    #[compile(array_len($settings))]
    num_settings: u16,

    #[since_version(2)]
    #[skip_getter]
    #[compile(0)]
    _padding: u16,

    #[read_offset_with($num_settings)]
    settings_offset: Offset32<[Setting]>,

    flags: FeatureFlags,

    name_idx: NameId,
}

record Setting {
    feature_id: u32,
    value: u16,
    #[skip_getter]
    #[compile(0)]
    _padding: u16,
}

flags u16 FeatureFlags {
    HIDDEN = 0x0800,
    EXCLUSIVE = 0x8000,
}


