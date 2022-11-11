#![parse_module(read_fonts::tables::vvar)]

/// The [VVAR (Vertical Metrics Variations)](https://docs.microsoft.com/en-us/typography/opentype/spec/vvar) table
table Vvar {
    /// Major version number of the horizontal metrics variations table — set to 1.
    /// Minor version number of the horizontal metrics variations table — set to 0.
    version: MajorMinor,
    /// Offset in bytes from the start of this table to the item variation store table.
    item_variation_store_offset: Offset32<ItemVariationStore>,
    #[nullable]
    /// Offset in bytes from the start of this table to the delta-set index mapping for advance heights (may be NULL).
    advance_height_mapping_offset: Offset32<DeltaSetIndexMap>,
    /// Offset in bytes from the start of this table to the delta-set index mapping for top side bearings (may be NULL).
    #[nullable]
    tsb_mapping_offset: Offset32<DeltaSetIndexMap>,
    /// Offset in bytes from the start of this table to the delta-set index mapping for bottom side bearings (may be NULL).
    #[nullable]
    bsb_mapping_offset: Offset32<DeltaSetIndexMap>,
    /// Offset in bytes from the start of this table to the delta-set index mapping for Y coordinates of vertical origins (may be NULL).
    #[nullable]
    v_org_mapping_offset: Offset32<DeltaSetIndexMap>,
}
