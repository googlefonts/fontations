#![parse_module(read_fonts::tables::varc)]

/// [VARC](https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md) (Variable Composites / Components Table)
/// 
/// [FontTools VARC](https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otData.py#L3459-L3476)
#[tag = "VARC"]
table Varc {
    /// Major/minor version number. Set to 1.0.
    // Do not annotate #[version] as that produces unused var warnings.
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,

    coverage_offset: Offset32<CoverageTable>,
    #[nullable]
    multi_var_store_offset: Offset32<MultiItemVariationStore>,
    #[nullable]
    condition_list_offset: Offset32<ConditionList>,
    #[nullable]
    axis_indices_list_offset: Offset32<Index2>,
    var_composite_glyphs_offset: Offset32<Index2>,
}

table MultiItemVariationStore {
    // TODO(rsheeter) Doing VARC incrementally, haven't got here yet.
}

table ConditionList {
    condition_count: u32,
    #[count($condition_count)]
    condition_offsets: [Offset32<Condition>],
}

/// Flags used in the [VarcComponent] byte stream
/// 
/// <https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md#variable-component-flags>
flags u32 VarcFlags {
    RESET_UNSPECIFIED_AXES      = 0b0000_0000_0000_0001,
    HAVE_AXES                   = 0b0000_0000_0000_0010,
    AXIS_VALUES_HAVE_VARIATION  = 0b0000_0000_0000_0100,
    TRANSFORM_HAS_VARIATION     = 0b0000_0000_0000_1000,
    HAVE_TRANSLATE_X            = 0b0000_0000_0001_0000,
    HAVE_TRANSLATE_Y            = 0b0000_0000_0010_0000,
    HAVE_ROTATION               = 0b0000_0000_0100_0000,
    HAVE_CONDITION              = 0b0000_0000_1000_0000,
    HAVE_SCALE_X                = 0b0000_0001_0000_0000,
    HAVE_SCALE_Y                = 0b0000_0010_0000_0000,
    HAVE_TCENTER_X              = 0b0000_0100_0000_0000,
    HAVE_TCENTER_Y              = 0b0000_1000_0000_0000,
    GID_IS_24BIT                = 0b0001_0000_0000_0000,
    HAVE_SKEW_X                 = 0b0010_0000_0000_0000,
    HAVE_SKEW_Y                 = 0b0100_0000_0000_0000,
    // Bits 15 through 31 inclusive
    RESERVED_MASK               = 0xFFFF8000,
}