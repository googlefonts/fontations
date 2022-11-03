//! The name table

include!("../../generated/generated_name.rs");
use read_fonts::tables::name::{Encoding, MacRomanMapping};

impl Name {
    fn compute_storage_offset(&self) -> u16 {
        let v0 = 6 // version, count, storage_offset
            + self.name_record.len() * 12;
        if let Some(lang_tag_records) = self.lang_tag_record.as_ref() {
            v0 + 4 * lang_tag_records.len()
        } else {
            v0
        }
        .try_into()
        .unwrap()
    }

    fn compute_version(&self) -> u16 {
        self.lang_tag_record.is_some().into()
    }
}

impl NameRecord {
    fn string(&self) -> &str {
        self.string_offset.as_str()
    }

    fn string_writer(&self) -> NameStringWriter {
        NameStringWriter {
            encoding: Encoding::new(self.platform_id, self.encoding_id),
            string: self.string(),
        }
    }

    fn validate_string_data(&self, ctx: &mut ValidationCtx) {
        let encoding = Encoding::new(self.platform_id, self.encoding_id);
        match encoding {
            Encoding::Unknown => ctx.report(format!(
                "Unhandled platform/encoding id pair: ({}, {})",
                self.platform_id, self.encoding_id
            )),
            Encoding::Utf16Be => (), // lgtm
            Encoding::MacRoman => {
                for c in self.string().chars() {
                    if MacRomanMapping.encode(c).is_none() {
                        ctx.report(format!(
                            "char {c} {} not representable in MacRoman encoding",
                            c.escape_unicode()
                        ))
                    }
                }
            }
        }
    }
}

impl FontWrite for NameRecord {
    fn write_into(&self, writer: &mut TableWriter) {
        self.platform_id.write_into(writer);
        self.encoding_id.write_into(writer);
        self.language_id.write_into(writer);
        self.name_id.write_into(writer);
        let string_writer = self.string_writer();
        string_writer.compute_length().write_into(writer);
        writer.write_offset(&string_writer, 2);
    }
}

impl LangTagRecord {
    fn lang_tag(&self) -> &str {
        self.lang_tag_offset.as_str()
    }

    fn string_writer(&self) -> NameStringWriter {
        NameStringWriter {
            encoding: Encoding::Utf16Be,
            string: self.lang_tag(),
        }
    }
}

impl FontWrite for LangTagRecord {
    fn write_into(&self, writer: &mut TableWriter) {
        let string_writer = self.string_writer();
        string_writer.compute_length().write_into(writer);
        writer.write_offset(&string_writer, 2);
    }
}

struct NameStringWriter<'a> {
    encoding: Encoding,
    string: &'a str,
}

impl NameStringWriter<'_> {
    fn compute_length(&self) -> u16 {
        match self.encoding {
            Encoding::Utf16Be => self.string.chars().map(|c| c.len_utf16() as u16 * 2).sum(),
            // this will be correct assuming we pass validation
            Encoding::MacRoman => self.string.len().try_into().unwrap(),
            Encoding::Unknown => 0,
        }
    }
}

impl FontWrite for NameStringWriter<'_> {
    fn write_into(&self, writer: &mut TableWriter) {
        for c in self.string.chars() {
            match self.encoding {
                Encoding::Utf16Be => {
                    let mut buf = [0, 0];
                    let enc = c.encode_utf16(&mut buf);
                    enc.iter()
                        .for_each(|unit| writer.write_slice(&unit.to_be_bytes()))
                }
                Encoding::MacRoman => {
                    MacRomanMapping
                        .encode(c)
                        .expect("invalid char for MacRoman")
                        .write_into(writer);
                }
                Encoding::Unknown => panic!("unknown encoding"),
            }
        }
    }
}

impl FromObjRef<read_fonts::tables::name::NameString<'_>> for String {
    fn from_obj_ref(obj: &read_fonts::tables::name::NameString<'_>, _: FontData) -> Self {
        obj.chars().collect()
    }
}

impl FromTableRef<read_fonts::tables::name::NameString<'_>> for String {}

impl PartialEq for NameRecord {
    fn eq(&self, other: &Self) -> bool {
        self.platform_id == other.platform_id
            && self.encoding_id == other.encoding_id
            && self.language_id == other.language_id
            && self.name_id == other.name_id
            && self.string_offset == other.string_offset
    }
}

impl Eq for NameRecord {}

impl Ord for NameRecord {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.platform_id,
            self.encoding_id,
            self.language_id,
            self.name_id,
        )
            .cmp(&(
                other.platform_id,
                other.encoding_id,
                other.language_id,
                other.name_id,
            ))
    }
}

impl PartialOrd for NameRecord {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding() {
        let stringthing = NameStringWriter {
            encoding: Encoding::Utf16Be,
            string: "hello",
        };
        assert_eq!(stringthing.compute_length(), 10);
    }

    #[test]
    fn compute_version() {
        let mut table = Name::default();
        assert_eq!(table.compute_version(), 0);
        table.lang_tag_record = Some(Vec::new());
        assert_eq!(table.compute_version(), 1);
    }

    #[test]
    fn sorting() {
        let mut table = Name::default();
        table.name_record.insert(NameRecord {
            platform_id: 3,
            encoding_id: 1,
            language_id: 0,
            name_id: 1030,
            string_offset: OffsetMarker::new("Ordinær".into()),
        });
        table.name_record.insert(NameRecord {
            platform_id: 0,
            encoding_id: 4,
            language_id: 0,
            name_id: 4,
            string_offset: OffsetMarker::new("oh".into()),
        });
        table.name_record.insert(NameRecord {
            platform_id: 3,
            encoding_id: 1,
            language_id: 0,
            name_id: 1029,
            string_offset: OffsetMarker::new("Regular".into()),
        });

        let _dumped = crate::dump_table(&table).unwrap();
        let loaded = read_fonts::tables::name::Name::read(FontData::new(&_dumped)).unwrap();
        assert_eq!(loaded.name_record()[0].encoding_id, 4);
        assert_eq!(loaded.name_record()[1].name_id, 1029);
        assert_eq!(loaded.name_record()[2].name_id, 1030);
    }

    #[test]
    fn roundtrip() {
        #[rustfmt::skip]
        static COLINS_BESPOKE_DATA: &[u8] = &[
            0x0, 0x0, // version
            0x0, 0x03, // count
            0x0, 42, // storage offset
            //record 1:
            0x00, 0x03, // platformID
            0x00, 0x01, // encodingID
            0x04, 0x09, // languageID
            0x00, 0x01, // nameID
            0x00, 0x0a, // length
            0x00, 0x00, // offset
            //record 2:
            0x00, 0x03, // platformID
            0x00, 0x01, // encodingID
            0x04, 0x09, // languageID
            0x00, 0x02, // nameID
            0x00, 0x10, // length
            0x00, 0x0a, // offset
            //record 2:
            0x00, 0x03, // platformID
            0x00, 0x01, // encodingID
            0x04, 0x09, // languageID
            0x00, 0x03, // nameID
            0x00, 0x18, // length
            0x00, 0x1a, // offset
            // storage area:
            // string 1 'colin'
            0x0, 0x63, 0x0, 0x6F, 0x0, 0x6C, 0x0, 0x69,
            0x0, 0x6E,
            // string 2, 'nicelife'
            0x0, 0x6E, 0x0, 0x69, 0x0, 0x63, 0x0, 0x65,
            0x0, 0x6C, 0x0, 0x69, 0x0, 0x66, 0x0, 0x65,
            // string3 'i hate fonts'
            0x0, 0x69, 0x0, 0x20, 0x0, 0x68, 0x0, 0x61,
            0x0, 0x74, 0x0, 0x65, 0x0, 0x20, 0x0, 0x66,
            0x0, 0x6F, 0x0, 0x6E, 0x0, 0x74, 0x0, 0x73,
        ];

        let raw_table =
            read_fonts::tables::name::Name::read(FontData::new(COLINS_BESPOKE_DATA)).unwrap();
        let owned: Name = raw_table.to_owned_table();
        let dumped = crate::dump_table(&owned).unwrap();
        let reloaded = read_fonts::tables::name::Name::read(FontData::new(&dumped)).unwrap();

        for rec in raw_table.name_record() {
            let raw_str = rec.string(raw_table.string_data()).unwrap();
            eprintln!("{raw_str}");
        }

        assert_eq!(raw_table.version(), reloaded.version());
        assert_eq!(raw_table.count(), reloaded.count());
        assert_eq!(raw_table.storage_offset(), reloaded.storage_offset());

        let mut fail = false;
        for (old, new) in raw_table
            .name_record()
            .iter()
            .zip(reloaded.name_record().iter())
        {
            assert_eq!(old.platform_id(), new.platform_id());
            assert_eq!(old.encoding_id(), new.encoding_id());
            assert_eq!(old.language_id(), new.language_id());
            assert_eq!(old.name_id(), new.name_id());
            assert_eq!(old.length(), new.length());
            eprintln!("{:?} {:?}", old.string_offset(), new.string_offset());
            let old_str = old.string(raw_table.string_data()).unwrap();
            let new_str = new.string(reloaded.string_data()).unwrap();
            if old_str != new_str {
                eprintln!("'{old_str}' != '{new_str}'");
                fail = true;
            }
        }
        if fail {
            panic!("some comparisons failed");
        }
    }
}
