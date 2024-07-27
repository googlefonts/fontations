#![parse_module(read_fonts::tables::ift)]

extern record U8Or16;

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
  #[count(4)]
  compatibility_id: [u32],

  /// Largest entry index which appears in either the glyph map or feature map.
  max_entry_index: u16,

  /// Largest entry index which appears in the glyph map.
  max_glyph_map_entry_index: u16,
  
  glyph_count: u32,

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
  patch_encoding: u8,
}

#[read_args(glyph_count: u32, max_entry_index: u16)]
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

/// [Patch Map Format Format 2](https://w3c.github.io/IFT/Overview.html#patch-map-format-1)
table PatchMapFormat2 {
  /// Format identifier: format = 2
  #[format = 2]
  format: u8,

  todo: u32,

  // TODO(garretrieger): write me.
}