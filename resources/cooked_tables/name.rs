//! The [name (Naming)](https://docs.microsoft.com/en-us/typography/opentype/spec/name) table

/// [Naming table version 0](https://docs.microsoft.com/en-us/typography/opentype/spec/name#naming-table-version-0)
#[offset_host]
Name0<'a> {
    /// Table version number (=0).
    version: BigEndian<u16>,
    /// Number of name records.
    count: BigEndian<u16>,
    /// Offset to start of string storage (from start of table).
    storage_offset: BigEndian<Offset16>,
    /// The name records where count is the number of records.
    #[count(count)]
    name_record: [NameRecord],
}

/// [Naming table version 1](https://docs.microsoft.com/en-us/typography/opentype/spec/name#naming-table-version-1)
#[offset_host]
Name1<'a> {
    /// Table version number (=0).
    version: BigEndian<u16>,
    /// Number of name records.
    count: BigEndian<u16>,
    /// Offset to start of string storage (from start of table).
    storage_offset: BigEndian<Offset16>,
    /// The name records where count is the number of records.
    #[count(count)]
    name_record: [NameRecord],
    /// Number of language-tag records.
    lang_tag_count: BigEndian<u16>,
    /// The language-tag records where langTagCount is the number of records.
    #[count(lang_tag_count)]
    lang_tag_record: [LangTagRecord],
}

#[format(u16)]
#[generate_getters]
enum Name<'a> {
    #[version(0)]
    Version0(Name0<'a>),
    #[version(1)]
    Version1(Name1<'a>),
}

/// Part of [Name1]
LangTagRecord {
    /// Language-tag string length (in bytes)
    length: BigEndian<u16>,
    /// Language-tag string offset from start of storage area (in
    /// bytes).
    lang_tag_offset: BigEndian<Offset16>,
}

///[Name Records](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-records)
NameRecord {
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
