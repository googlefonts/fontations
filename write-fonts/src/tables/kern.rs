//! The [Kerning (kern)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6kern.html) table.

include!("../../generated/generated_kern.rs");

/// The kerning table.
#[derive(Clone)]
pub enum Kern {
    Ot(Vec<Subtable>),
    Aat(Vec<u8>),
}

/// The various kinds of kerning subtables.
#[derive(Clone)]
pub enum SubtableKind {
    Format0(Subtable0),
    Other(Vec<u8>),
}

#[derive(Clone)]
pub struct Subtable {
    pub coverage: u16,
    pub kind: SubtableKind,
}

impl FromObjRef<read_fonts::tables::kern::Kern<'_>> for Kern {
    fn from_obj_ref(from: &read_fonts::tables::kern::Kern, data: FontData) -> Self {
        match from {
            read_fonts::tables::kern::Kern::Ot(_) => {
                let mut subtables = vec![];
                for (subtable, kind) in from
                    .subtables()
                    .filter_map(|table| table.ok())
                    .filter_map(|table| Some((table.clone(), table.kind().ok()?)))
                {
                    let (subtable_data, coverage) = match subtable {
                        read_fonts::tables::kern::Subtable::Ot(subtable) => {
                            (subtable.data(), subtable.coverage())
                        }
                        _ => continue,
                    };
                    let kind =
                        if let read_fonts::tables::kern::SubtableKind::Format0(format0) = kind {
                            SubtableKind::Format0(Subtable0::from_obj_ref(&format0, data))
                        } else {
                            SubtableKind::Other(subtable_data.to_vec())
                        };
                    subtables.push(Subtable { coverage, kind })
                }
                Self::Ot(subtables)
            }
            read_fonts::tables::kern::Kern::Aat(kern) => {
                Self::Aat(kern.offset_data().as_bytes().to_vec())
            }
        }
    }
}

impl FontWrite for Kern {
    fn table_type(&self) -> TableType {
        TableType::Named("kern")
    }

    fn write_into(&self, writer: &mut TableWriter) {
        match self {
            Self::Ot(subtables) => {
                0u16.write_into(writer);
                (subtables.len() as u16).write_into(writer);
                for subtable in subtables {
                    subtable.write_into(writer);
                }
            }
            Self::Aat(bytes) => {
                writer.write_slice(bytes);
            }
        }
    }
}

impl Validate for Kern {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        if let Kern::Ot(subtables) = self {
            ctx.in_table("Kern", |ctx| {
                ctx.in_field("subtables", |ctx| {
                    if subtables.len() > (u16::MAX as usize) {
                        ctx.report("array exceeds max length");
                    }
                    // for subtable in subtables {
                    //     subtable.validate_impl(ctx);
                    // }
                });
            })
        }
    }
}

impl FontWrite for Subtable {
    fn write_into(&self, writer: &mut TableWriter) {
        // version
        0u16.write_into(writer);
        let length = match &self.kind {
            SubtableKind::Format0(format0) => format0.compute_length(),
            SubtableKind::Other(bytes) => (u16::RAW_BYTE_LEN * 3) + bytes.len(),
        };
        (length as u16).write_into(writer);
        self.coverage.write_into(writer);
        match &self.kind {
            SubtableKind::Format0(format0) => format0.write_into(writer),
            SubtableKind::Other(bytes) => writer.write_slice(bytes),
        }
    }
}

impl Subtable0 {
    fn compute_length(&self) -> usize {
        const KERN_PAIR_LEN: usize = 6;
        u16::RAW_BYTE_LEN * 7 + // format, length, coverage, num_pairs,
                                          // search_range, entry_selector, range_shift
        self.pairs.len() * KERN_PAIR_LEN
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::SearchRange;

    #[test]
    fn smoke_test() {
        let pairs = vec![
            Subtable0Pair::new(GlyphId16::new(4), GlyphId16::new(12), -40),
            Subtable0Pair::new(GlyphId16::new(4), GlyphId16::new(28), 40),
            Subtable0Pair::new(GlyphId16::new(5), GlyphId16::new(40), -50),
        ];
        //searchRange, entrySelector, rangeShift = getSearchRange(pairs.len(), 6);
        let computed = SearchRange::compute(pairs.len(), 6);
        let kern0 = Subtable0::new(
            computed.search_range,
            computed.entry_selector,
            computed.range_shift,
            pairs,
        );
        let subtable = Subtable {
            coverage: 0x0001,
            kind: SubtableKind::Format0(kern0),
        };
        let kern = Kern::Ot(vec![subtable]);

        let bytes = crate::dump_table(&kern).unwrap();
        assert_eq!(bytes, font_test_data::kern::KERN_VER_0_FMT_0_DATA);
    }
}
