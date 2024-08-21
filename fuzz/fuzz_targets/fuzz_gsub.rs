#![no_main]

mod traversal_fuzz;
use libfuzzer_sys::{fuzz_target, Corpus};
use read_fonts::tables::gsub::Gsub;

fuzz_target!(|data: &[u8]| -> Corpus { traversal_fuzz::try_traverse_table::<Gsub>(data, false) });
