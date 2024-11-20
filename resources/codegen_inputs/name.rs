#![parse_module(read_fonts::tables::name)]

/// [Naming table version 1](https://docs.microsoft.com/en-us/typography/opentype/spec/name#naming-table-version-1)
#[tag = "name"]
table Name {
    /// Table version number (0 or 1)
    #[version]
    #[compile(self.compute_version())]
    version: u16,
    /// Number of name records.
    #[compile(array_len($name_record))]
    count: u16,
    /// Offset to start of string storage (from start of table).
    #[compile(self.compute_storage_offset())]
    storage_offset: u16,
    /// The name records where count is the number of records.
    #[count($count)]
    #[offset_data_method(string_data)]
    #[offset_adjustment(self.compute_storage_offset() as u32)]
    #[validate(check_sorted_and_unique_name_records)]
    name_record: [NameRecord],
    /// Number of language-tag records.
    #[since_version(1)]
    #[compile(array_len($lang_tag_record))]
    lang_tag_count: u16,
    /// The language-tag records where langTagCount is the number of records.
    #[count($lang_tag_count)]
    #[offset_data_method(string_data)]
    #[offset_adjustment(self.compute_storage_offset() as u32)]
    #[since_version(1)]
    lang_tag_record: [LangTagRecord],
}

/// Part of [Name]
record LangTagRecord {
    /// Language-tag string length (in bytes)
    #[compile(skip)]
    length: u16,
    /// Language-tag string offset from start of storage area (in
    /// bytes).
    #[offset_getter(lang_tag)]
    #[traverse_with(traverse_lang_tag)]
    #[compile_type(OffsetMarker<String>)]
    #[compile_with(compile_name_string)]
    #[validate(skip)]
    lang_tag_offset: Offset16<NameString>,
}

///[Name Records](https://docs.microsoft.com/en-us/typography/opentype/spec/name#name-records)
record NameRecord {
    /// Platform ID.
    platform_id: u16,
    /// Platform-specific encoding ID.
    encoding_id: u16,
    /// Language ID.
    language_id: u16,
    /// Name ID.
    name_id: NameId,
    /// String length (in bytes).
    #[compile(skip)]
    length: u16,
    /// String offset from start of storage area (in bytes).
    #[traverse_with(traverse_string)]
    #[offset_getter(string)]
    #[compile_type(OffsetMarker<String>)]
    #[compile_with(compile_name_string)]
    #[validate(validate_string_data)]
    string_offset: Offset16<NameString>,
}
