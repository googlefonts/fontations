#![parse_module(read_fonts::tables::svg)]

/// [SVG](https://learn.microsoft.com/en-us/typography/opentype/spec/svg)
#[tag = "SVG "]
table SVG {
    /// Table version (starting at 0). Set to 0.
    #[compile(0)]
    version: u16,
    /// Offset to the SVGDocumentList, from the start of the SVG table. 
    /// Must be non-zero.
    svg_document_list_offset: Offset32<SVGDocumentList>,
    /// Set to 0.
    #[skip_getter]
    #[compile(0)]
    _reserved: u16,
}

/// [SVGDocumentList](https://learn.microsoft.com/en-us/typography/opentype/spec/svg)
table SVGDocumentList {
    /// Number of SVGDocumentRecords. Must be non-zero.
    num_entries: u16,
    /// Array of SVGDocumentRecords.
    #[count($num_entries)]
    document_records: [SVGDocumentRecord],
}

/// [SVGDocumentRecord](https://learn.microsoft.com/en-us/typography/opentype/spec/svg)
record SVGDocumentRecord {
    /// The first glyph ID for the range covered by this record.
    start_glyph_id: GlyphId16,
    /// The last glyph ID for the range covered by this record.
    end_glyph_id: GlyphId16,
    /// Offset from the beginning of the SVGDocumentList to an SVG 
    /// document. Must be non-zero.
    // TODO: Use a different type here?
    svg_doc_offset: u32,
    /// Length of the SVG document data. Must be non-zero.
    svg_doc_length: u32,
}

