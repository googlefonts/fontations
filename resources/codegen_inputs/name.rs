#![parse_module(font_tables::tables::name)]
/// [Naming table version 1](https://docs.microsoft.com/en-us/typography/opentype/spec/name#naming-table-version-1)
table Name {
    /// Table version number (0 or 1)
    #[version]
    version: BigEndian<u16>,
    /// Number of name records.
    count: BigEndian<u16>,
    /// Offset to start of string storage (from start of table).
    storage_offset: BigEndian<Offset16>,
    /// The name records where count is the number of records.
    #[count($count)]
    name_record: [NameRecord],
    /// Number of language-tag records.
    #[available(1)]
    lang_tag_count: BigEndian<u16>,
    /// The language-tag records where langTagCount is the number of records.
    #[count($lang_tag_count)]
    #[available(1)]
    lang_tag_record: [LangTagRecord],
}

/// Part of [Name1]
record LangTagRecord {
    /// Language-tag string length (in bytes)
    length: BigEndian<u16>,
    /// Language-tag string offset from start of storage area (in
    /// bytes).
    lang_tag_offset: BigEndian<Offset16>,
}

///[Name Records](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-records)
record NameRecord {
    /// Platform ID.
    platform_id: BigEndian<u16>,
    /// Platform-specific encoding ID.
    encoding_id: BigEndian<u16>,
    /// Language ID.
    language_id: BigEndian<u16>,
    /// Name ID.
    name_id: BigEndian<u16>,
    /// String length (in bytes).
    length: BigEndian<u16>,
    /// String offset from start of storage area (in bytes).
    string_offset: BigEndian<Offset16>,
}
