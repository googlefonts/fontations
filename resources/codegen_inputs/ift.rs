#![parse_module(read_fonts::tables::ift)]

extern scalar MatchModeAndCount;
extern record U8Or16;
extern record U16Or24;
extern record IdDeltaOrLength;
extern scalar CompatibilityId;

format u8 Ift {
  Format1(PatchMapFormat1),
  Format2(PatchMapFormat2),
}

/// [Patch Map Format Format 1](https://w3c.github.io/IFT/Overview.html#patch-map-format-1)
table PatchMapFormat1 {
  /// Format identifier: format = 1
  #[format = 1]
  format: u8,

  #[skip_getter]
  #[compile(0)]
  _reserved: u32,

  /// Unique ID that identifies compatible patches.
  #[traverse_with(skip)]
  compatibility_id: CompatibilityId,

  /// Largest entry index which appears in either the glyph map or feature map.
  max_entry_index: u16,

  /// Largest entry index which appears in the glyph map.
  max_glyph_map_entry_index: u16,
  
  glyph_count: Uint24,

  /// Sub table that maps glyph ids to entry indices.
  #[read_offset_with($glyph_count, $max_entry_index)]
  glyph_map_offset: Offset32<GlyphMap>,

  /// Sub table that maps feature and glyph ids to entry indices.
  #[nullable]
  #[read_offset_with($max_entry_index)]
  feature_map_offset: Offset32<FeatureMap>,

  #[count(max_value_bitmap_len($max_entry_index))]
  applied_entries_bitmap: [u8],

  // URI Template String (UTF-8 Encoded)
  uri_template_length: u16,
  #[count($uri_template_length)]
  uri_template: [u8],

  /// Patch format number for patches referenced by this mapping.
  patch_format: u8,
}

#[read_args(glyph_count: Uint24, max_entry_index: u16)]
table GlyphMap {
  first_mapped_glyph: u16,

  #[count(subtract($glyph_count, $first_mapped_glyph))]
  #[read_with($max_entry_index)]
  #[traverse_with(skip)]
  #[compile(skip)] // TODO remove this once write fonts side is implemented.
  entry_index: ComputedArray<U8Or16>,
}

#[read_args(max_entry_index: u16)]
table FeatureMap {
  feature_count: u16,

  #[count($feature_count)]
  #[read_with($max_entry_index)]
  #[traverse_with(skip)]
  #[compile(skip)] // TODO remove this once write fonts side is implemented.
  feature_records: ComputedArray<FeatureRecord>,

  // Variable sized array of EntryMapRecord's which depends on the contents of 'feature_records'
  // the array length is determined in the read-fonts impl.
  #[count(..)]
  entry_map_data: [u8],
}

#[read_args(max_entry_index: u16)]
record FeatureRecord {
  feature_tag: Tag,

  #[read_with($max_entry_index)]
  #[traverse_with(skip)]
  #[compile(skip)] // TODO remove this once write fonts side is implemented.
  first_new_entry_index: U8Or16,

  #[read_with($max_entry_index)]
  #[traverse_with(skip)]
  #[compile(skip)] // TODO remove this once write fonts side is implemented.
  entry_map_count: U8Or16,
}

#[read_args(max_entry_index: u16)]
record EntryMapRecord {
  #[read_with($max_entry_index)]
  #[traverse_with(skip)]
  #[compile(skip)] // TODO remove this once write fonts side is implemented.
  first_entry_index: U8Or16,

  #[read_with($max_entry_index)]
  #[traverse_with(skip)]
  #[compile(skip)] // TODO remove this once write fonts side is implemented.
  last_entry_index: U8Or16,
}

/// [Patch Map Format Format 2](https://w3c.github.io/IFT/Overview.html#patch-map-format-2)
table PatchMapFormat2 {
  /// Format identifier: format = 2
  #[format = 2]
  format: u8,

  #[skip_getter]
  #[compile(0)]
  _reserved: u32,

  /// Unique ID that identifies compatible patches.
  #[traverse_with(skip)]
  compatibility_id: CompatibilityId,

  /// Patch format number for patches referenced by this mapping.
  default_patch_format: u8,

  // Encoded entries
  entry_count: Uint24,
  entries_offset: Offset32<MappingEntries>,

  #[nullable]
  entry_id_string_data_offset: Offset32<IdStringData>,
  
  // URI Template String (UTF-8 Encoded)
  uri_template_length: u16,
  #[count($uri_template_length)]
  uri_template: [u8],
}

table MappingEntries {
  #[count(..)]
  entry_data: [u8],
}

#[read_args(entry_id_string_data_offset: Offset32)]
table EntryData {
  format_flags: EntryFormatFlags,

  // FEATURES_AND_DESIGN_SPACE
  #[if_flag($format_flags, EntryFormatFlags::FEATURES_AND_DESIGN_SPACE)]
  feature_count: u8,
  #[if_flag($format_flags, EntryFormatFlags::FEATURES_AND_DESIGN_SPACE)]
  #[count($feature_count)]
  feature_tags: [Tag],

  #[if_flag($format_flags, EntryFormatFlags::FEATURES_AND_DESIGN_SPACE)]
  design_space_count: u16,
  #[if_flag($format_flags, EntryFormatFlags::FEATURES_AND_DESIGN_SPACE)]
  #[count($design_space_count)]
  design_space_segments: [DesignSpaceSegment],

  // CHILD_INDICES
  #[if_flag($format_flags, EntryFormatFlags::CHILD_INDICES)]
  #[traverse_with(skip)]
  #[compile(skip)] // TODO remove this once write fonts side is implemented.]
  match_mode_and_count: MatchModeAndCount,
  #[if_flag($format_flags, EntryFormatFlags::CHILD_INDICES)]
  #[count(try_into($match_mode_and_count))]
  child_indices: [Uint24],

  // ENTRY_ID_DELTA
  #[read_with($entry_id_string_data_offset)]
  #[if_flag($format_flags, EntryFormatFlags::ENTRY_ID_DELTA)]
  #[traverse_with(skip)]
  #[compile(skip)]
  entry_id_delta: IdDeltaOrLength,

  // PATCH_FORMAT
  #[if_flag($format_flags, EntryFormatFlags::PATCH_FORMAT)]
  patch_format: u8,

  // CODEPOINT_BIT_1 or CODEPOINT_BIT_2
  // Non-conditional since we also use this to find the end of the entry.
  #[count(..)]
  codepoint_data: [u8],
}

// See <https://w3c.github.io/IFT/Overview.html#mapping-entry-formatflags>
flags u8 EntryFormatFlags {
  // Fields specifying features and design space are present.
  FEATURES_AND_DESIGN_SPACE = 0b00000001,

  // Fields specifying copy indices are present.
  CHILD_INDICES = 0b00000010,

  // Fields specifying the entry ID delta are present.
  ENTRY_ID_DELTA = 0b00000100,

  // Fields specifying the patch encoding are present.
  PATCH_FORMAT = 0b00001000,

  // These two bits specify how the codepoint set is encoded.
  CODEPOINTS_BIT_1 = 0b00010000,
  CODEPOINTS_BIT_2 = 0b00100000,

  // If set, this entry is ignored.
  IGNORED =  0b01000000,

  // Reserved for future use.
  RESERVED = 0b10000000,
}

record DesignSpaceSegment {
  axis_tag: Tag,
  start: Fixed,
  end: Fixed,
}

// Storage for id strings, indexed by EntryData::entryIdStringLength
// See: https://w3c.github.io/IFT/Overview.html#mapping-entry-entryidstringlength
table IdStringData {
  #[count(..)]
  id_data: [u8],
}

/// [Table Keyed Patch](https://w3c.github.io/IFT/Overview.html#table-keyed)
table TableKeyedPatch {
  format: Tag,
  #[skip_getter]
  #[compile(0)]
  _reserved: u32,

  /// Unique ID that identifies compatible patches.
  #[traverse_with(skip)]
  compatibility_id: CompatibilityId,

  patches_count: u16,
  #[count(add($patches_count, 1))]
  patch_offsets: [Offset32<TablePatch>],
}

/// [TablePatch](https://w3c.github.io/IFT/Overview.html#tablepatch)
table TablePatch {
  tag: Tag,
  flags: TablePatchFlags,
  max_uncompressed_length: u32,
  #[count(..)]
  brotli_stream: [u8],
}

// See <https://w3c.github.io/IFT/Overview.html#tablepatch-flags>
flags u8 TablePatchFlags {
  REPLACE_TABLE = 0b01,
  DROP_TABLE = 0b10,
}

/// [Glyph Keyed Patch](https://w3c.github.io/IFT/Overview.html#glyph-keyed)
table GlyphKeyedPatch {
  format: Tag,
  #[skip_getter]
  #[compile(0)]
  _reserved: u32,
  flags: GlyphKeyedFlags,
  #[traverse_with(skip)]
  compatibility_id: CompatibilityId,
  max_uncompressed_length: u32,
  #[count(..)]
  brotli_stream: [u8],
}

flags u8 GlyphKeyedFlags {
  NONE = 0b0,
  WIDE_GLYPH_IDS = 0b1,
}

/// [GlyphPatches](https://w3c.github.io/IFT/Overview.html#glyphpatches)
#[read_args(flags: GlyphKeyedFlags)]
table GlyphPatches {
  glyph_count: u32,
  table_count: u8,

  #[count($glyph_count)]
  #[read_with($flags)]
  #[traverse_with(skip)]
  #[compile(skip)] // TODO remove this once write fonts side is implemented.
  glyph_ids: ComputedArray<U16Or24>,

  #[count($table_count)]
  tables: [Tag],

  #[count(multiply_add($glyph_count, $table_count, 1))]
  glyph_data_offsets: [Offset32<GlyphData>],
}

table GlyphData {
  #[count(..)]
  data: [u8],
}