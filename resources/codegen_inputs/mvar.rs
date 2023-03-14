#![parse_module(read_fonts::tables::mvar)]

/// The [MVAR (Metrics Variations)](https://docs.microsoft.com/en-us/typography/opentype/spec/mvar) table
#[tag = "MVAR"]
table Mvar {
    /// Major version number of the horizontal metrics variations table — set to 1.
    /// Minor version number of the horizontal metrics variations table — set to 0.
    version: MajorMinor,
    /// Not used; set to 0.
    #[skip_getter]
    #[compile(0)]
    _reserved: u16,
    /// The size in bytes of each value record — must be greater than zero.
    value_record_size: u16,
    /// The number of value records — may be zero.
    value_record_count: u16,
    /// Offset in bytes from the start of this table to the item variation store table. If valueRecordCount is zero, set to zero; if valueRecordCount is greater than zero, must be greater than zero.
    #[nullable]
    item_variation_store_offset: Offset32<ItemVariationStore>,
    /// Array of value records that identify target items and the associated delta-set index for each. The valueTag records must be in binary order of their valueTag field.
    #[count($value_record_count)]
    value_records: [ValueRecord],
}

/// [ValueRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/mvar#table-formats) metrics variation record
record ValueRecord {
    /// Four-byte tag identifying a font-wide measure.
    value_tag: Tag,
    /// A delta-set outer index — used to select an item variation data subtable within the item variation store.
    delta_set_outer_index: u16,
    /// A delta-set inner index — used to select a delta-set row within an item variation data subtable.
    delta_set_inner_index: u16,
}
