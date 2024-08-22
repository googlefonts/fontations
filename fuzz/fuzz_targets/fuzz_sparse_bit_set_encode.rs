#![no_main]
//! A fuzzer which tests code for encoding sparse bit sets. Re-uses the op processing from the
//! int set fuzzer with a more limited set of op codes (just ones related to building up
//! the set).

use libfuzzer_sys::fuzz_target;
mod int_set_op_processor;
use int_set_op_processor::process_op_codes;
use int_set_op_processor::read_u32;
use int_set_op_processor::OperationSet;
use std::io::Cursor;

const OPERATION_COUNT: u64 = 2000;

fuzz_target!(|data: &[u8]| {
    let mut data = Cursor::new(data);

    let (Some(bias), Some(max_value)) = (read_u32(&mut data), read_u32(&mut data)) else {
        return;
    };

    let _ = process_op_codes::<u32>(
        OperationSet::SparseBitSetEncoding(bias, max_value),
        OPERATION_COUNT,
        data,
    );
});
