use font_types::{BigEndian, FontRead};

font_types_macro::tables! {
    /// A kind of a record
    #[flags(u16)]
    RecordKind {
        /// Thing One
        X_PLACEMENT = 0x0001,
        /// Another
        Y_PLACEMENT = 0x0002,
        /// Advance X
        X_ADVANCE = 0x0004,
        /// Y Advances!
        Y_ADVANCE = 0x0008,
    }

    Thing {
        kind: BigEndian<RecordKind>,
        field: BigEndian<u16>,
    }
}

fn main() {
    let mut buf = font_types::test_helpers::BeBuffer::new();
    buf.push(RecordKind::X_ADVANCE | RecordKind::Y_PLACEMENT);
    buf.push(69u16);

    let thing = Thing::read(&buf).unwrap();
    assert_eq!(thing.field(), 69);
    assert!(thing.kind().contains(RecordKind::X_ADVANCE));
    assert!(thing.kind().contains(RecordKind::Y_PLACEMENT));
    assert!(!thing.kind().contains(RecordKind::Y_ADVANCE));
}
