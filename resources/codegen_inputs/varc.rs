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
    var_composite_glyphs_offset: Offset32<VarCompositeGlyphs>,
}

table MultiItemVariationStore {
    // TODO(rsheeter) Doing VARC incrementally, haven't got here yet.
}

table ConditionList {
    condition_count: u32,
    #[count($condition_count)]
    condition_offsets: [Offset32<Condition>],
}

table VarCompositeGlyphs {
    // TODO(rsheeter) Doing VARC incrementally, haven't got here yet.
}
