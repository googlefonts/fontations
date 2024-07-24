#![parse_module(read_fonts::tables::ift)]

#[tag = "IFT "]
#[skip_font_write]
#[skip_from_obj]
table IFT {
}

#[tag = "IFTX"]
#[skip_font_write]
#[skip_from_obj]
table IFTX {
}


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

  /// Number of entries and glyphs that are mapped.
  max_entry_index: u16,
  glyph_count: u32,

  /// Sub table that maps glyph ids to entry indices.
  #[read_offset_with($glyph_count, $max_entry_index)]
  glyph_map_offset: Offset32<GlyphMap>,

  /// Sub table that maps feature and glyph ids to entry indices.
  #[nullable] // TODO(garretrieger): this does not currently match the spec, update spec to allow feature map to be nullable.
  feature_map_offset: Offset32<FeatureMap>,

  #[count(max_value_bitmap($max_entry_index))]
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

table FeatureMap {
  feature_count: u16,
  // TODO(garretrieger): write me.
}

record FeatureRecord {
  // TODO(garretrieger): write me.
  todo: u8,
}

record EntryMapRecord {
  // TODO(garretrieger): write me.
  todo: u8,
}

/// [Patch Map Format Format 2](https://w3c.github.io/IFT/Overview.html#patch-map-format-1)
table PatchMapFormat2 {
  /// Format identifier: format = 2
  #[format = 2]
  format: u8,

  todo: u32,

  // TODO(garretrieger): write me.
}