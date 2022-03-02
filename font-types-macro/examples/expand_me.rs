//! something to macro-expand when debugging

#![allow(dead_code)]

use font_types::BigEndian;

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
        field: BigEndian<u32>,
    }
}

fn main() {}
