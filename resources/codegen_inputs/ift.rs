#![parse_module(read_fonts::tables::ift)]

extern scalar MatchModeAndCount;
extern record U8Or16;
extern record U16Or24;
extern scalar CompatibilityId;

flags u8 PatchMapFieldPresenceFlags {
  CFF_CHARSTRINGS_OFFSET = 0b00000001,
  CFF2_CHARSTRINGS_OFFSET = 0b00000010,
}

/// [Patch Map Table](https://w3c.github.io/IFT/Overview.html#patch-map-table)
table IftPatchMap {
  /// Format identifier: format = 2
  format: u8,

  #[skip_getter]
  #[compile(0)]
  _reserved_0: u8,
  #[skip_getter]
  #[compile(0)]
  _reserved_1: u8,
  #[skip_getter]
  #[compile(0)]
  _reserved_2: u8,

  field_flags: PatchMapFieldPresenceFlags,

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
  url_template_length: u16,
  #[count($url_template_length)]
  url_template: [u8],

  // Offset to the cff charstrings INDEX from the start of the CFF table.
  #[if_flag($field_flags, PatchMapFieldPresenceFlags::CFF_CHARSTRINGS_OFFSET)]
  cff_charstrings_offset: u32,

  // Offset to the cff charstrings INDEX from the start of the CFF2 table.
  #[if_flag($field_flags, PatchMapFieldPresenceFlags::CFF2_CHARSTRINGS_OFFSET)]
  cff2_charstrings_offset: u32,
}

table MappingEntries {
  #[count(..)]
  entry_data: [u8],
}

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
  // PATCH_FORMAT
  // CODEPOINT_BIT_1 or CODEPOINT_BIT_2
  //
  // These remaining fields don't have well defined widths and are handling with
  // custom parsing.
  #[skip_getter] // this is the only non-conditional field that occurs after a
                 // conditional field, and codegen chokes on that.
  #[count(..)]
  trailing_data: [u8],
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
