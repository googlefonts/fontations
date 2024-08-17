#![no_main]
//! A fuzzer for the code which decodes sparse bit sets <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>

use libfuzzer_sys::fuzz_target;
use read_fonts::collections::IntSet;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    };
    let Ok(bias_bytes) = data[..4].try_into() else {
        return;
    };
    let bias = u32::from_be_bytes(bias_bytes);
    let _ = IntSet::<u32>::from_sparse_bit_set_bounded(&data[4..], bias, 10_000);
});
