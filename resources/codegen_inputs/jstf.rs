#![parse_module(read_fonts::tables::jstf)]

/// The [JSTF](https://learn.microsoft.com/en-us/typography/opentype/spec/jstf) table.
#[tag = "JSTF"]
table Jstf {
    /// Table version, 1.0.
    #[version]
    #[compile(MajorMinor::VERSION_1_0)]
    version: MajorMinor,
    /// Number of script records.
    #[compile(array_len($jstf_script_records))]
    jstf_script_count: u16,
    /// Script records, in alphabetical order by tag.
    #[count($jstf_script_count)]
    jstf_script_records: [JstfScriptRecord],
}

/// A script tag and its justification data.
record JstfScriptRecord {
    jstf_script_tag: Tag,
    /// Offset from the beginning of JSTF.
    jstf_script_offset: Offset16<JstfScript>,
}

/// Justification data for a script.
table JstfScript {
    #[nullable]
    extender_glyph_offset: Offset16<ExtenderGlyph>,
    #[nullable]
    def_jstf_lang_sys_offset: Offset16<JstfLangSys>,
    #[compile(array_len($jstf_lang_sys_records))]
    jstf_lang_sys_count: u16,
    #[count($jstf_lang_sys_count)]
    jstf_lang_sys_records: [JstfLangSysRecord],
}

/// Glyphs available for extending lines of text.
table ExtenderGlyph {
    #[compile(array_len($extender_glyphs))]
    glyph_count: u16,
    #[count($glyph_count)]
    extender_glyphs: [GlyphId16],
}

/// A language tag and its justification data.
record JstfLangSysRecord {
    jstf_lang_sys_tag: Tag,
    /// Offset from the beginning of JstfScript.
    jstf_lang_sys_offset: Offset16<JstfLangSys>,
}

/// Justification priorities for a language system.
table JstfLangSys {
    #[compile(self.jstf_priorities.len())]
    jstf_priority_count: u16,
    #[count($jstf_priority_count)]
    #[offset_getter(jstf_priorities)]
    jstf_priority_offsets: [Offset16<JstfPriority>],
}

/// Suggestions for shrinking and extending a line at one priority level.
table JstfPriority {
    #[nullable]
    gsub_shrinkage_enable_offset: Offset16<JstfModList>,
    #[nullable]
    gsub_shrinkage_disable_offset: Offset16<JstfModList>,
    #[nullable]
    gpos_shrinkage_enable_offset: Offset16<JstfModList>,
    #[nullable]
    gpos_shrinkage_disable_offset: Offset16<JstfModList>,
    #[nullable]
    shrinkage_jstf_max_offset: Offset16<JstfMax>,
    #[nullable]
    gsub_extension_enable_offset: Offset16<JstfModList>,
    #[nullable]
    gsub_extension_disable_offset: Offset16<JstfModList>,
    #[nullable]
    gpos_extension_enable_offset: Offset16<JstfModList>,
    #[nullable]
    gpos_extension_disable_offset: Offset16<JstfModList>,
    #[nullable]
    extension_jstf_max_offset: Offset16<JstfMax>,
}

/// Indices of GSUB or GPOS lookups to enable or disable.
table JstfModList {
    #[compile(array_len($lookup_indices))]
    lookup_count: u16,
    #[count($lookup_count)]
    lookup_indices: [u16],
}
