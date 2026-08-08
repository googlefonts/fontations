#![no_main]

mod traversal_fuzz;
use libfuzzer_sys::{fuzz_target, Corpus};
use read_fonts::tables::gdef::Gdef;

fuzz_target!(|data: &[u8]| -> Corpus { traversal_fuzz::try_traverse_table::<Gdef>(data, false) });
