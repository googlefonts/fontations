//! Incremental Font Transfer [Patch Map](https://w3c.github.io/IFT/Overview.html#font-format-extensions)

include!("../../generated/generated_ift.rs");

use std::str;

#[derive(Clone)]
pub struct Entry {
    // Key
    pub codepoints: Vec<u32>,
    // TODO: features and axis space.

    // Value
    pub compatibility_id: [u32; 4],
    pub uri: String,
    pub patch_encoding: u8, // TODO: give this a type?
}

impl<'a> PatchMapFormat1<'a> {
    pub fn to_entries(&self) -> Result<Vec<Entry>, ReadError> {
        let prototype = Entry {
            codepoints: vec![],
            compatibility_id: [
                self.compatibility_id().get(0).unwrap().get(),
                self.compatibility_id().get(1).unwrap().get(),
                self.compatibility_id().get(2).unwrap().get(),
                self.compatibility_id().get(3).unwrap().get(),
            ],
            uri: self.uri_template_as_string().to_string(),
            patch_encoding: self.patch_encoding(),
        };

        let mut entries = vec![];
        entries.resize(self.entry_count() as usize, prototype);

        let glyph_map = self.glyph_map()?;
        for gid in glyph_map.first_mapped_glyph() as u32
            ..(glyph_map.first_mapped_glyph() as u32 + self.glyph_count())
        {
            let entry_id = glyph_map.entry_index()[gid as usize];
            entries[entry_id as usize].codepoints.push(gid); // TODO convert to codepoint w/ cmap
        }

        Ok(entries)
    }

    pub fn uri_template_as_string(&self) -> &str {
        // TODO handle errors
        str::from_utf8(self.uri_template()).unwrap()
    }
}

impl<'a> PatchMapFormat2<'a> {
    pub fn to_entries(&self) -> Result<Vec<Entry>, ReadError> {
        todo!()
    }
}

impl<'a> Ift<'a> {
    pub fn to_entries(&self) -> Result<Vec<Entry>, ReadError> {
        match self {
            Self::Format1(v) => v.to_entries(),
            Self::Format2(v) => v.to_entries(),
        }
    }
}
