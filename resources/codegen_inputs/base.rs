#![parse_module(read_fonts::tables::base)]

/// The [BASE](https://learn.microsoft.com/en-us/typography/opentype/spec/base) (Baseline) table
#[tag = "BASE"]
table Base {
    /// (major, minor) Version for the BASE table (1,0) or (1,1)
    #[version]
    #[compile(self.compute_version())]
    version: MajorMinor,
    /// Offset to horizontal Axis table, from beginning of BASE table (may be NULL)
    #[nullable]
    horiz_axis_offset: Offset16<Axis>,
    /// Offset to vertical Axis table, from beginning of BASE table (may be NULL)
    #[nullable]
    vert_axis_offset: Offset16<Axis>,
    /// Offset to Item Variation Store table, from beginning of BASE table (may be null)
    #[since_version(1.1)]
    #[nullable]
    item_var_store_offset: Offset32<ItemVariationStore>,
}

/// [Axis Table](https://learn.microsoft.com/en-us/typography/opentype/spec/base#axis-tables-horizaxis-and-vertaxis)
table Axis {
    /// Offset to BaseTagList table, from beginning of Axis table (may
    /// be NULL)
    #[nullable]
    base_tag_list_offset: Offset16<BaseTagList>,
    /// Offset to BaseScriptList table, from beginning of Axis table
    base_script_list_offset: Offset16<BaseScriptList>,
}

/// [BaseTagList Table](https://learn.microsoft.com/en-us/typography/opentype/spec/base#basetaglist-table)
table BaseTagList {
    /// Number of baseline identification tags in this text direction
    /// — may be zero (0)
    #[compile(array_len($baseline_tags))]
    base_tag_count: u16,
    /// Array of 4-byte baseline identification tags — must be in
    /// alphabetical order
    #[count($base_tag_count)]
    baseline_tags: [Tag],
}

/// [BaseScriptList Table](https://learn.microsoft.com/en-us/typography/opentype/spec/base#basescriptlist-table)
table BaseScriptList {
    /// Number of BaseScriptRecords defined
    #[compile(array_len($base_script_records))]
    base_script_count: u16,
    /// Array of BaseScriptRecords, in alphabetical order by
    /// baseScriptTag
    #[count($base_script_count)]
    base_script_records: [BaseScriptRecord],
}

/// [BaseScriptRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/base#basescriptrecord)
record BaseScriptRecord {
    /// 4-byte script identification tag
    base_script_tag: Tag,
    /// Offset to BaseScript table, from beginning of BaseScriptList
    #[offset_from(BaseScriptList)]
    base_script_offset: Offset16<BaseScript>,
}

/// [BaseScript Table](https://learn.microsoft.com/en-us/typography/opentype/spec/base#basescript-table)
table BaseScript {
    /// Offset to BaseValues table, from beginning of BaseScript table (may be NULL)
    #[nullable]
    base_values_offset: Offset16<BaseValues>,
    /// Offset to MinMax table, from beginning of BaseScript table (may be NULL)
    #[nullable]
    default_min_max_offset: Offset16<MinMax>,
    /// Number of BaseLangSysRecords defined — may be zero (0)
    #[compile(array_len($base_lang_sys_records))]
    base_lang_sys_count: u16,
    /// Array of BaseLangSysRecords, in alphabetical order by
    /// BaseLangSysTag
    #[count($base_lang_sys_count)]
    base_lang_sys_records: [BaseLangSysRecord],
}

/// [BaseLangSysRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/base#baselangsysrecord)
record BaseLangSysRecord {
    /// 4-byte language system identification tag
    base_lang_sys_tag: Tag,
    /// Offset to MinMax table, from beginning of BaseScript table
    #[offset_from(BaseScript)]
    min_max_offset: Offset16<MinMax>,
}

/// [BaseValues](https://learn.microsoft.com/en-us/typography/opentype/spec/base#basevalues-table) table
table BaseValues {
    /// Index number of default baseline for this script — equals
    /// index position of baseline tag in baselineTags array of the
    /// BaseTagList
    default_baseline_index: u16,
    /// Number of BaseCoord tables defined — should equal
    /// baseTagCount in the BaseTagList
    #[compile(array_len($base_coord_offsets))]
    base_coord_count: u16,
    /// Array of offsets to BaseCoord tables, from beginning of
    /// BaseValues table — order matches baselineTags array in the
    /// BaseTagList
    #[count($base_coord_count)]
    base_coord_offsets: [Offset16<BaseCoord>],
}

/// [MinMax](https://learn.microsoft.com/en-us/typography/opentype/spec/base#minmax-table) table
table MinMax {
    /// Offset to BaseCoord table that defines the minimum extent
    /// value, from the beginning of MinMax table (may be NULL)
    #[nullable]
    min_coord_offset: Offset16<BaseCoord>,
    /// Offset to BaseCoord table that defines maximum extent value,
    /// from the beginning of MinMax table (may be NULL)
    #[nullable]
    max_coord_offset: Offset16<BaseCoord>,
    /// Number of FeatMinMaxRecords — may be zero (0)
    #[compile(array_len($feat_min_max_records))]
    feat_min_max_count: u16,
    /// Array of FeatMinMaxRecords, in alphabetical order by
    /// featureTableTag
    #[count($feat_min_max_count)]
    feat_min_max_records: [FeatMinMaxRecord],
}

/// [FeatMinMaxRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/base#baselangsysrecord)
record FeatMinMaxRecord {
    /// 4-byte feature identification tag — must match feature tag in
    /// FeatureList
    feature_table_tag: Tag,
    /// Offset to BaseCoord table that defines the minimum extent
    /// value, from beginning of MinMax table (may be NULL)
    #[nullable]
    #[offset_from(MinMax)]
    min_coord_offset: Offset16<BaseCoord>,
    /// Offset to BaseCoord table that defines the maximum extent
    /// value, from beginning of MinMax table (may be NULL)
    #[nullable]
    #[offset_from(MinMax)]
    max_coord_offset: Offset16<BaseCoord>,
}

format u16 BaseCoord {
    Format1(BaseCoordFormat1),
    Format2(BaseCoordFormat2),
    Format3(BaseCoordFormat3),
}

/// [BaseCoordFormat1](https://learn.microsoft.com/en-us/typography/opentype/spec/base#basecoord-format-1)
table BaseCoordFormat1 {
    /// Format identifier — format = 1
    #[format = 1]
    base_coord_format: u16,
    /// X or Y value, in design units
    coordinate: i16,
}

/// [BaseCoordFormat2](https://learn.microsoft.com/en-us/typography/opentype/spec/base#basecoord-format-2)
table BaseCoordFormat2 {
    /// Format identifier — format = 2
    #[format = 2]
    base_coord_format: u16,
    /// X or Y value, in design units
    coordinate: i16,
    /// Glyph ID of control glyph
    reference_glyph: u16,
    /// Index of contour point on the reference glyph
    base_coord_point: u16,
}

/// [BaseCoordFormat3](https://learn.microsoft.com/en-us/typography/opentype/spec/base#basecoord-format-3)
table BaseCoordFormat3 {
    /// Format identifier — format = 3
    #[format = 3]
    base_coord_format: u16,
    /// X or Y value, in design units
    coordinate: i16,
    /// Offset to Device table (non-variable font) / Variation Index
    /// table (variable font) for X or Y value, from beginning of
    /// BaseCoord table (may be NULL).
    #[nullable]
    device_offset: Offset16<DeviceOrVariationIndex>,
}

