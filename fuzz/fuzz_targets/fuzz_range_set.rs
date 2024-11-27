#![no_main]
//! Fuzzes the incremental_font_transfer patch_group.rs API

use libfuzzer_sys::{arbitrary, fuzz_target};
use read_fonts::collections::{IntSet, RangeSet};

#[derive(Debug, arbitrary::Arbitrary)]
enum Operation {
    Insert(u16, u16),
    Iter(),
}

#[derive(Default)]
struct State {
    range_set: RangeSet<u16>,
    int_set: IntSet<u16>,
}

const OP_COUNT_LIMIT: u64 = 1000;

fuzz_target!(|operations: Vec<Operation>| {
    let mut state: State = Default::default();
    let mut op_count = 0u64;
    for op in operations {
        match op {
            Operation::Insert(start, end) => {
                if end > start {
                    let count = end as u64 - start as u64;
                    op_count = op_count.saturating_add(count.saturating_mul(count.ilog2() as u64));
                    if op_count > OP_COUNT_LIMIT {
                        return;
                    }
                }

                state.range_set.insert(start, end);
                state.int_set.insert_range(start..=end);
            }
            Operation::Iter() => {
                op_count = op_count.saturating_add(state.int_set.iter_ranges().count() as u64);
                if op_count > OP_COUNT_LIMIT {
                    return;
                }

                assert!(state.range_set.iter().eq(state.int_set.iter_ranges()));
            }
        }
    }
});
