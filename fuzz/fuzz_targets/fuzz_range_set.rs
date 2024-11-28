#![no_main]
//! Fuzzes the incremental_font_transfer patch_group.rs API

use std::ops::RangeInclusive;

use libfuzzer_sys::{arbitrary, fuzz_target};
use read_fonts::collections::{IntSet, RangeSet};

#[derive(Debug, arbitrary::Arbitrary)]
enum Operation {
    Insert(RangeInclusive<u16>),
    Extend(Vec<RangeInclusive<u16>>),
    Intersection,
    Iter,
}

#[derive(Debug, arbitrary::Arbitrary)]
enum SetIndex {
    One,
    Two,
}

#[derive(Default)]
struct State {
    range_set: RangeSet<u16>,
    int_set: IntSet<u16>,
}

const OP_COUNT_LIMIT: u64 = 1000;

fn range_len(range: &RangeInclusive<u16>) -> u64 {
    if range.end() > range.start() {
        let count = *range.end() as u64 - *range.start() as u64;
        count.saturating_mul(count.ilog2() as u64)
    } else {
        0
    }
}

fuzz_target!(|operations: Vec<(Operation, SetIndex)>| {
    let mut state1: State = Default::default();
    let mut state2: State = Default::default();
    let mut op_count = 0u64;
    for (op, index) in operations {
        let (state, state_other) = match index {
            SetIndex::One => (&mut state1, &mut state2),
            SetIndex::Two => (&mut state2, &mut state1),
        };

        match op {
            Operation::Insert(range) => {
                op_count = op_count.saturating_add(range_len(&range));
                if op_count > OP_COUNT_LIMIT {
                    return;
                }

                state.range_set.insert(range.clone());
                state.int_set.insert_range(range.clone());
            }
            Operation::Extend(ranges) => {
                for range in ranges.iter() {
                    op_count = op_count.saturating_add(range_len(range));
                    if op_count > OP_COUNT_LIMIT {
                        return;
                    }
                    state.int_set.insert_range(range.clone());
                }
                state.range_set.extend(ranges.into_iter());
            }
            Operation::Iter => {
                op_count = op_count.saturating_add(state.int_set.iter_ranges().count() as u64);
                if op_count > OP_COUNT_LIMIT {
                    return;
                }

                assert!(state.range_set.iter().eq(state.int_set.iter_ranges()));
            }
            Operation::Intersection => {
                op_count = op_count.saturating_add(state.int_set.len());
                op_count = op_count.saturating_add(state_other.int_set.len());
                if op_count > OP_COUNT_LIMIT {
                    return;
                }

                let mut tmp = state.int_set.clone();
                tmp.intersect(&state_other.int_set);
                assert!(state
                    .range_set
                    .intersection(&state_other.range_set)
                    .eq(tmp.iter_ranges()));
            }
        }
    }
});
