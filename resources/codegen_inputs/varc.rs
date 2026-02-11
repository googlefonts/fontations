#![parse_module(read_fonts::tables::varc)]

extern record Index2;

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

/// * <https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otData.py#L3451-L3457>
/// * <https://github.com/harfbuzz/harfbuzz/blob/7be12b33e3f07067c159d8f516eb31df58c75876/src/hb-ot-layout-common.hh#L3517-L3520C3>
table MultiItemVariationStore {
    #[format = 1]
    format: u16,
    region_list_offset: Offset32<SparseVariationRegionList>,
    variation_data_count: u16,
    #[count($variation_data_count)]
    variation_data_offsets: [Offset32<MultiItemVariationData>],
}

table SparseVariationRegionList {
  region_count: u16,
  #[count($region_count)]
  region_offsets: [Offset32<SparseVariationRegion>],
}

table SparseVariationRegion {
    region_axis_count: u16,
    #[count($region_axis_count)]
    region_axes: [SparseRegionAxisCoordinates],
}

record SparseRegionAxisCoordinates
{
  axis_index: u16,
  start: F2Dot14,
  peak: F2Dot14,
  end: F2Dot14,
}

table MultiItemVariationData {
    #[format = 1]
    format: u8,
    region_index_count: u16,
    #[count($region_index_count)]
    region_indices: [u16],
    delta_set_count: u32,
    #[present_if($delta_set_count)]
    delta_set_off_size: u8,
    #[present_if($delta_set_count)]
    #[count(add_multiply($delta_set_count, 1, $delta_set_off_size))]
    delta_set_offsets: [u8],
    #[present_if($delta_set_count)]
    #[count(..)]
    delta_set_data: [u8],
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