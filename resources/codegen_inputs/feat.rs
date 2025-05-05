#![parse_module(read_fonts::tables::feat)]

/// The [feature name](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6feat.html) table.
#[tag = "feat"]
table Feat {
    /// Version number of the feature name table (0x00010000 for the current
    /// version).
    version: MajorMinor,
    /// The number of entries in the feature name array.
    feature_name_count: u16,
    /// Reserved (set to 0).
    #[skip_getter]
    #[compile(0)]
    _reserved1: u16,
    /// Reserved (set to 0).
    #[skip_getter]
    #[compile(0)]
    _reserved2: u32,
    /// The feature name array, sorted by feature type.
    #[count($feature_name_count)]
    names: [FeatureName],
}

/// Type, flags and names for a feature.
record FeatureName {
    /// Feature type.
    feature: u16,
    /// The number of records in the setting name array.
    n_settings: u16,
    /// Offset in bytes from the beginning of this table to this feature's
    /// setting name array. The actual type of record this offset refers 
    /// to will depend on the exclusivity value, as described below.
    #[read_offset_with($n_settings)]
    #[offset_from(Feat)]
    setting_table_offset: Offset32<SettingNameArray>,
    /// Flags associated with the feature type.
    feature_flags: u16,
    /// The name table index for the feature's name.
    name_index: NameId,
}

#[read_args(n_settings: u16)]
table SettingNameArray {
    /// List of setting names for a feature.
    #[count($n_settings)]
    settings: [SettingName],
}

/// Associates a setting with a name identifier.
record SettingName {
    /// The setting.
    setting: u16,
    /// The name table index for the setting's name.
    name_index: NameId,
}
