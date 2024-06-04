#![parse_module(read_fonts::tables::varc)]

/// [VARC](https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md) (Variable Composites / Components Table)
/// 
/// [FontTools VARC](https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otData.py#L3459-L3476)
#[tag = "VARC"]
table Varc {
    /// Major/minor version number. Set to 1.0.
    #[version]
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,

    coverage_offset: Offset32<CoverageTable>,
}