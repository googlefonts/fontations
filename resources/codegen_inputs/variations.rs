#![parse_module(read_fonts::tables::variations)]

extern scalar TupleIndex;

/// [TupleVariationHeader](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#tuplevariationheader)
#[read_args(axis_count: u16)]
#[skip_constructor]
table TupleVariationHeader {
    /// The size in bytes of the serialized data for this tuple
    /// variation table.
    variation_data_size: u16,
    /// A packed field. The high 4 bits are flags (see below). The low
    /// 12 bits are an index into a shared tuple records array.
    #[traverse_with(traverse_tuple_index)]
    tuple_index: TupleIndex,
    /// Peak tuple record for this tuple variation table — optional,
    /// determined by flags in the tupleIndex value.  Note that this
    /// must always be included in the 'cvar' table.
    #[skip_getter]
    #[count(tuple_len($tuple_index, $axis_count, 0))]
    peak_tuple: [F2Dot14],
    /// Intermediate start tuple record for this tuple variation table
    /// — optional, determined by flags in the tupleIndex value.
    #[skip_getter]
    #[count(tuple_len($tuple_index, $axis_count, 1))]
    intermediate_start_tuple: [F2Dot14],
    /// Intermediate end tuple record for this tuple variation table
    /// — optional, determined by flags in the tupleIndex value.
    #[skip_getter]
    #[count(tuple_len($tuple_index, $axis_count, 1))]
    intermediate_end_tuple: [F2Dot14],
}

/// A [Tuple Record](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#tuple-records)
///
/// The tuple variation store formats reference regions within the font’s
/// variation space using tuple records. A tuple record identifies a position
/// in terms of normalized coordinates, which use F2DOT14 values.
#[read_args(axis_count: u16)]
#[capabilities(hash, equality, order)]
record Tuple<'a> {
    /// Coordinate array specifying a position within the font’s variation space.
    ///
    /// The number of elements must match the axisCount specified in the
    /// 'fvar' table.
    #[count($axis_count)]
    values: [F2Dot14],
}

/// The [DeltaSetIndexMap](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data) table format 0
table DeltaSetIndexMapFormat0 {
    /// DeltaSetIndexMap format: set to 0.
    #[format = 0]
    format: u8,
    /// A packed field that describes the compressed representation of
    /// delta-set indices. See details below.
    entry_format: EntryFormat,
    /// The number of mapping entries.
    map_count: u16,
    /// The delta-set index mapping data. See details below.
    #[count(delta_set_index_data($entry_format, $map_count))]
    map_data: [u8],
}

/// The [DeltaSetIndexMap](https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats#associating-target-items-to-variation-data) table format 1
table DeltaSetIndexMapFormat1 {
    /// DeltaSetIndexMap format: set to 1.
    #[format = 1]
    format: u8,
    /// A packed field that describes the compressed representation of
    /// delta-set indices. See details below.
    entry_format: EntryFormat,
    /// The number of mapping entries.
    map_count: u32,
    /// The delta-set index mapping data. See details below.
    #[count(delta_set_index_data($entry_format, $map_count))]
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
    #[compile(self.compute_axis_count())]
    axis_count: u16,
    /// The number of variation region tables in the variation region
    /// list. Must be less than 32,768.
    #[compile(array_len($variation_regions))]
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
    #[compile(array_len($item_variation_data_offsets))]
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
    #[compile(array_len($region_indexes))]
    region_index_count: u16,
    /// Array of indices into the variation region list for the regions
    /// referenced by this item variation data table.
    #[count($region_index_count)]
    region_indexes: [u16],
    /// Delta-set rows.
    #[count(..)]
    delta_sets: [u8],
}

