#![no_main]
//! A fuzzer for the code which decodes sparse bit sets <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>

use libfuzzer_sys::fuzz_target;
use read_fonts::collections::IntSet;

fuzz_target!(|data: &[u8]| {
    let bias = data.first().map(|v| *v as u32).unwrap_or(0);
    let _ = IntSet::<u32>::from_sparse_bit_set_bounded(data, bias, 10_000);
});
