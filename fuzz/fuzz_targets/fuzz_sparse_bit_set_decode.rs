#![no_main]
//! A fuzzer for the code which decodes sparse bit sets <https://w3c.github.io/IFT/Overview.html#sparse-bit-set-decoding>

use int_set::IntSet;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = IntSet::<u32>::from_sparse_bit_set(data);
});
