#![no_main]
//! A correctness fuzzer that checks all of the public API methods of [IntSet].
//!
//! This fuzzer exercises the public API of IntSet and compares the results to the same operations run
//! on a BTreeSet. Any differences in behaviour and/or set contents triggers a panic.
//!
//! The fuzzer input data is interpreted as a series of op codes which map to the public api methods of IntSet.

use font_types::{GlyphId, GlyphId16};
use libfuzzer_sys::fuzz_target;
mod int_set_op_processor;
use int_set_op_processor::process_op_codes;
use int_set_op_processor::OperationSet;
use int_set_op_processor::SmallEvenInt;
use int_set_op_processor::SmallInt;
use std::io::Cursor;

const OPERATION_COUNT: u64 = 7_500;

fuzz_target!(|data: &[u8]| {
    let Some(mode_byte) = data.first() else {
        return;
    };

    match mode_byte {
        // These variants provide the primary testing of functionality.
        1 => {
            let _ = process_op_codes::<u32>(
                OperationSet::Standard,
                OPERATION_COUNT,
                Cursor::new(&data[1..]),
            );
        }
        2 => {
            let _ = process_op_codes::<SmallInt>(
                OperationSet::Standard,
                OPERATION_COUNT,
                Cursor::new(&data[1..]),
            );
        }
        3 => {
            let _ = process_op_codes::<SmallEvenInt>(
                OperationSet::Standard,
                OPERATION_COUNT,
                Cursor::new(&data[1..]),
            );
        }

        // And these provide coverage of remaining default supported domains
        4 => {
            let _ = process_op_codes::<u8>(
                OperationSet::Standard,
                OPERATION_COUNT,
                Cursor::new(&data[1..]),
            );
        }
        5 => {
            let _ = process_op_codes::<u16>(
                OperationSet::Standard,
                OPERATION_COUNT,
                Cursor::new(&data[1..]),
            );
        }
        6 => {
            let _ = process_op_codes::<GlyphId>(
                OperationSet::Standard,
                OPERATION_COUNT,
                Cursor::new(&data[1..]),
            );
        }
        7 => {
            let _ = process_op_codes::<GlyphId16>(
                OperationSet::Standard,
                OPERATION_COUNT,
                Cursor::new(&data[1..]),
            );
        }
        _ => return,
    };
});
