#![parse_module(read_fonts::variation)]

/// The [DeltaSetIndexMap](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data) table format 0
table DeltaSetIndexMapFormat0 {
    /// DeltaSetIndexMap format: set to 0.
    #[format = 0]
    format: u8,
    /// A packed field that describes the compressed representation of 
    /// delta-set indices. See details below.
    entry_format: u8,
    /// The number of mapping entries.
    map_count: u16,
    /// The delta-set index mapping data. See details below.
    #[count(((($entry_format & 0x30) >> 4) + 1) as usize * $map_count as usize)]
    map_data: [u8],
}

/// The [DeltaSetIndexMap](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data) table format 1
table DeltaSetIndexMapFormat1 {
    /// DeltaSetIndexMap format: set to 1.
    #[format = 1]
    format: u8,
    /// A packed field that describes the compressed representation of 
    /// delta-set indices. See details below.
    entry_format: u8,
    /// The number of mapping entries.
    map_count: u32,
    /// The delta-set index mapping data. See details below.
    #[count(((($entry_format & 0x30) >> 4) + 1) as usize * $map_count as usize)]
    map_data: [u8],
}

/// The [DeltaSetIndexMap](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data) table
format u8 DeltaSetIndexMap {
    Format0(DeltaSetIndexMapFormat0),
    Format1(DeltaSetIndexMapFormat1),
}

/// Entry format for a [DeltaSetIndexMap].
flags u8 EntryFormat {
    /// Mask for the low 4 bits, which give the count of bits minus one that are used in each entry for the inner-level index.    
    INNER_INDEX_BIT_COUNT_MASK = 0x0F,
    /// Mask for bits that indicate the size in bytes minus one of each entry.
    MAP_ENTRY_SIZE_MASK = 0x30,
}

/// The [VariationRegionList](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#variation-regions) table
table VariationRegionList {
    /// The number of variation axes for this font. This must be the 
    /// same number as axisCount in the 'fvar' table.
    axis_count: u16,
    /// The number of variation region tables in the variation region 
    /// list. Must be less than 32,768.
    region_count: u16,
    /// Array of variation regions.
    #[count($region_count)]
    #[read_with($axis_count)]
    variation_regions: ComputedArray<VariationRegion<'a>>,
}

/// The [VariationRegion](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#variation-regions) record
#[read_args(axis_count: u16)]
record VariationRegion<'a> {
    /// Array of region axis coordinates records, in the order of axes 
    /// given in the 'fvar' table.
    #[count($axis_count)]
    region_axes: [RegionAxisCoordinates],
}

/// The [RegionAxisCoordinates](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#variation-regions) record
record RegionAxisCoordinates {
    /// The region start coordinate value for the current axis.
    start_coord: F2Dot14,
    /// The region peak coordinate value for the current axis.
    peak_coord: F2Dot14,
    /// The region end coordinate value for the current axis.
    end_coord: F2Dot14,
}

/// The [ItemVariationStore](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#item-variation-store-header-and-item-variation-data-subtables) table
table ItemVariationStore {
    /// Format— set to 1
    format: u16,
    /// Offset in bytes from the start of the item variation store to 
    /// the variation region list.
    variation_region_list_offset: Offset32<VariationRegionList>,
    /// The number of item variation data subtables.
    item_variation_data_count: u16,
    /// Offsets in bytes from the start of the item variation store to 
    /// each item variation data subtable.
    #[nullable]
    #[count($item_variation_data_count)]
    item_variation_data_offsets: [Offset32<ItemVariationData>],
}

/// The [ItemVariationData](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#item-variation-store-header-and-item-variation-data-subtables) subtable
table ItemVariationData {
    /// The number of delta sets for distinct items.
    item_count: u16,
    /// A packed field: the high bit is a flag—see details below.
    word_delta_count: u16,
    /// The number of variation regions referenced.
    region_index_count: u16,
    /// Array of indices into the variation region list for the regions 
    /// referenced by this item variation data table.
    #[count($region_index_count)]
    region_indexes: [u16],
    /// Delta-set rows.
    #[count(..)]
    delta_sets: [u8],
}

