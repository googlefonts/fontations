#![no_main]
//! A fuzzer for the code which decodes sparse bit sets <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>

use libfuzzer_sys::fuzz_target;
use read_fonts::collections::IntSet;

fuzz_target!(|data: &[u8]| {
    let _ = IntSet::<u32>::from_sparse_bit_set_bounded(data, 10_000);
});
