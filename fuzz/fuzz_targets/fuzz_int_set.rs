#![no_main]
//! A correctness fuzzer that checks all of the public API methods of IntSet.
//!
//! This fuzzer exercises the public API of IntSet and compares the results to the same operations run
//! on a BTreeSet. Any differences in behaviour and/or set contents triggers a panic.
//!
//! The fuzzer input data is interpretted as a series of op codes which map to the public api methods of IntSet.
//!
//! Note: currently only inclusive mode IntSet's are tested.

use std::cmp::max;
use std::ops::Bound::Excluded;
use std::ops::Bound::Included;
use std::ops::RangeInclusive;
use std::{collections::BTreeSet, error::Error};

use int_set::IntSet;
use libfuzzer_sys::fuzz_target;

const OPERATION_COUNT: usize = 7_500;

// TODO(garretrieger): allow inverted sets to be accessed.
// TODO(garretrieger): allow a limited domain set to be accessed.

struct Input<'a> {
    // The state includes 2 of each type of sets to allow us to test out binary set operations (eg. union)
    int_set: &'a mut IntSet<u32>,
    btree_set: &'a mut BTreeSet<u32>,
}

impl Input<'_> {
    fn from<'a>(int_set: &'a mut IntSet<u32>, btree_set: &'a mut BTreeSet<u32>) -> Input<'a> {
        Input { int_set, btree_set }
    }
}

trait Operation {
    fn size(&self, set_len: usize) -> usize;
    fn operate(&self, input: Input, other: Input);
}

/* ### Insert ### */

struct InsertOp(u32);

impl InsertOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(val))), data)
    }
}

impl Operation for InsertOp {
    fn operate(&self, input: Input, _: Input) {
        input.int_set.insert(self.0);
        input.btree_set.insert(self.0);
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### Insert Range ### */

struct InsertRangeOp(u32, u32);

impl InsertRangeOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(min), data) = read_u32(data) else {
            return (None, data);
        };
        let (Some(max), data) = read_u32(data) else {
            return (None, data);
        };

        (Some(Box::new(Self(min, max))), data)
    }
}

impl Operation for InsertRangeOp {
    fn size(&self, length: usize) -> usize {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1 as usize - self.0 as usize) + 1) * (length.ilog2() as usize)
    }

    fn operate(&self, input: Input, _: Input) {
        input.int_set.insert_range(self.0..=self.1);
        for v in self.0..=self.1 {
            input.btree_set.insert(v);
        }
    }
}

/* ### Remove ### */

struct RemoveOp(u32);

impl RemoveOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(val))), data)
    }
}

impl Operation for RemoveOp {
    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }

    fn operate(&self, input: Input, _: Input) {
        input.int_set.remove(self.0);
        input.btree_set.remove(&self.0);
    }
}

/* ### Remove Range ### */

struct RemoveRangeOp(u32, u32);

impl RemoveRangeOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(min), data) = read_u32(data) else {
            return (None, data);
        };
        let (Some(max), data) = read_u32(data) else {
            return (None, data);
        };

        (Some(Box::new(Self(min, max))), data)
    }
}

impl Operation for RemoveRangeOp {
    fn size(&self, length: usize) -> usize {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1 as usize - self.0 as usize) + 1) * (length.ilog2() as usize)
    }

    fn operate(&self, input: Input, _: Input) {
        input.int_set.remove_range(self.0..=self.1);
        for v in self.0..=self.1 {
            input.btree_set.remove(&v);
        }
    }
}

/* ### Length ### */
struct LengthOp();

impl LengthOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for LengthOp {
    fn operate(&self, input: Input, _: Input) {
        assert_eq!(input.int_set.len(), input.btree_set.len());
    }

    fn size(&self, _: usize) -> usize {
        1
    }
}

/* ### Is Empty ### */

struct IsEmptyOp();

impl IsEmptyOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for IsEmptyOp {
    fn operate(&self, input: Input, _: Input) {
        assert_eq!(input.int_set.is_empty(), input.btree_set.is_empty());
    }

    fn size(&self, _: usize) -> usize {
        1
    }
}

/* ### Contains ### */
struct ContainsOp(u32);

impl ContainsOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(val))), data)
    }
}

impl Operation for ContainsOp {
    fn operate(&self, input: Input, _: Input) {
        assert_eq!(
            input.int_set.contains(self.0),
            input.btree_set.contains(&self.0)
        );
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### Clear  ### */

struct ClearOp();

impl ClearOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for ClearOp {
    fn operate(&self, input: Input, _: Input) {
        input.int_set.clear();
        input.btree_set.clear();
    }

    fn size(&self, length: usize) -> usize {
        length
    }
}

/* ### Intersects Range ### */

struct IntersectsRangeOp(u32, u32);

impl IntersectsRangeOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(min), data) = read_u32(data) else {
            return (None, data);
        };
        let (Some(max), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(min, max))), data)
    }
}

impl Operation for IntersectsRangeOp {
    fn operate(&self, input: Input, _: Input) {
        let int_set_intersects = input.int_set.intersects_range(self.0..=self.1);

        let mut btree_intersects = false;
        for v in self.0..=self.1 {
            if input.btree_set.contains(&v) {
                btree_intersects = true;
                break;
            }
        }

        assert_eq!(int_set_intersects, btree_intersects);
    }

    fn size(&self, length: usize) -> usize {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1 as usize - self.0 as usize) + 1) * (length.ilog2() as usize)
    }
}

/* ### First  ### */

struct FirstOp();

impl FirstOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for FirstOp {
    fn operate(&self, input: Input, _: Input) {
        assert_eq!(input.int_set.first(), input.btree_set.first().copied());
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### First  ### */

struct LastOp();

impl LastOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for LastOp {
    fn operate(&self, input: Input, _: Input) {
        assert_eq!(input.int_set.last(), input.btree_set.last().copied());
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### Iter ### */

struct IterOp();

impl IterOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for IterOp {
    fn operate(&self, input: Input, _: Input) {
        assert!(input.int_set.iter().eq(input.btree_set.iter().copied()));
    }

    fn size(&self, length: usize) -> usize {
        length
    }
}

/* ### Iter Ranges ### */

struct IterRangesOp();

impl IterRangesOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for IterRangesOp {
    fn operate(&self, input: Input, _: Input) {
        let mut btree_ranges: Vec<RangeInclusive<u32>> = vec![];
        let mut cur_range: Option<RangeInclusive<u32>> = None;

        for v in input.btree_set.iter().copied() {
            if let Some(range) = cur_range {
                if range.end() + 1 == v {
                    cur_range = Some(*range.start()..=v);
                    continue;
                }
                btree_ranges.push(range);
            }

            cur_range = Some(v..=v);
        }

        if let Some(range) = cur_range {
            btree_ranges.push(range);
        }

        assert!(input.int_set.iter_ranges().eq(btree_ranges.iter().cloned()));
    }

    fn size(&self, length: usize) -> usize {
        length
    }
}

/* ### Iter After ### */

struct IterAfterOp(u32);

impl IterAfterOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(val))), data)
    }
}

impl Operation for IterAfterOp {
    fn operate(&self, input: Input, _: Input) {
        let it = input.int_set.iter_after(self.0);
        let btree_it = input
            .btree_set
            .range((Excluded(self.0), Included(u32::MAX)));
        assert!(it.eq(btree_it.copied()));
    }

    fn size(&self, length: usize) -> usize {
        length
    }
}

/* ### Remove All ### */

struct RemoveAllOp(Vec<u32>);

impl RemoveAllOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(values))), data)
    }
}

impl Operation for RemoveAllOp {
    fn operate(&self, input: Input, _: Input) {
        input.int_set.remove_all(self.0.iter().copied());
        for v in self.0.iter() {
            input.btree_set.remove(v);
        }
    }

    fn size(&self, length: usize) -> usize {
        (length.ilog2() as usize) * self.0.len()
    }
}

/* ### Extend ### */

struct ExtendOp(Vec<u32>);

impl ExtendOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(values))), data)
    }
}

impl Operation for ExtendOp {
    fn operate(&self, input: Input, _: Input) {
        input.int_set.extend(self.0.iter().copied());
        input.btree_set.extend(self.0.iter().copied());
    }

    fn size(&self, length: usize) -> usize {
        (length.ilog2() as usize) * self.0.len()
    }
}

/* ### Extend Unsorted ### */

struct ExtendUnsortedOp(Vec<u32>);

impl ExtendUnsortedOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(values))), data)
    }
}

impl Operation for ExtendUnsortedOp {
    fn operate(&self, input: Input, _: Input) {
        input.int_set.extend_unsorted(self.0.iter().copied());
        input.btree_set.extend(self.0.iter().copied());
    }

    fn size(&self, length: usize) -> usize {
        (length.ilog2() as usize) * self.0.len()
    }
}

/* ### Union ### */

struct UnionOp();

impl UnionOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for UnionOp {
    fn operate(&self, a: Input, b: Input) {
        a.int_set.union(b.int_set);
        for v in b.btree_set.iter() {
            a.btree_set.insert(*v);
        }
    }

    fn size(&self, length: usize) -> usize {
        // TODO(garretrieger): should be length a + length b
        length
    }
}

/* ### Intersect ### */

struct IntersectOp();

impl IntersectOp {
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for IntersectOp {
    fn operate(&self, a: Input, b: Input) {
        a.int_set.intersect(b.int_set);
        let mut intersected: BTreeSet<u32> =
            a.btree_set.intersection(b.btree_set).copied().collect();
        std::mem::swap(a.btree_set, &mut intersected);
    }

    fn size(&self, length: usize) -> usize {
        // TODO(garretrieger): should be length a + length b
        length
    }
}

/* ### End of Ops ### */

fn read_u8(data: &[u8]) -> (Option<u8>, &[u8]) {
    if data.is_empty() {
        return (None, data);
    }
    (Some(data[0]), &data[1..])
}

fn read_u32(data: &[u8]) -> (Option<u32>, &[u8]) {
    if data.len() < 4 {
        return (None, data);
    }
    (
        Some(
            ((data[0] as u32) << 24)
                | ((data[1] as u32) << 16)
                | ((data[2] as u32) << 8)
                | (data[3] as u32),
        ),
        &data[4..],
    )
}

fn read_u32_vec(data: &[u8]) -> (Option<Vec<u32>>, &[u8]) {
    let (Some(count), data) = read_u8(data) else {
        return (None, data);
    };

    let mut values: Vec<u32> = vec![];
    let mut data = data;
    for _ in 0..count {
        let r = read_u32(data);
        let Some(value) = r.0 else {
            return (None, data);
        };
        data = r.1;
        values.push(value);
    }
    (Some(values), data)
}

struct NextOperation<'a> {
    op: Box<dyn Operation>,
    set_index: usize,
    data: &'a [u8],
}

fn next_operation(data: &[u8]) -> Option<NextOperation> {
    let op_code = data.first()?;

    // Check the msb of op code to see which set index to use.
    const INDEX_MASK: u8 = 0b10000000;
    let set_index = if (op_code & INDEX_MASK) > 0 { 1 } else { 0 };
    let op_code = !INDEX_MASK & op_code;

    // TODO ops for most public api methods (have operations for iter() be what checks for
    //      iter() equality alongside the check at end):
    // - invert
    // - inclusive_iter
    // - is_inverted
    let data = &data[1..];
    let (op, data) = match op_code {
        1 => InsertOp::parse_args(data),
        2 => RemoveOp::parse_args(data),
        3 => InsertRangeOp::parse_args(data),
        4 => RemoveRangeOp::parse_args(data),
        5 => LengthOp::parse_args(data),
        6 => IsEmptyOp::parse_args(data),
        7 => ContainsOp::parse_args(data),
        8 => ClearOp::parse_args(data),
        9 => IntersectsRangeOp::parse_args(data),
        10 => FirstOp::parse_args(data),
        11 => LastOp::parse_args(data),
        12 => IterOp::parse_args(data),
        13 => IterRangesOp::parse_args(data),
        14 => IterAfterOp::parse_args(data),
        15 => RemoveAllOp::parse_args(data),
        16 => ExtendOp::parse_args(data),
        17 => ExtendUnsortedOp::parse_args(data),
        18 => UnionOp::parse_args(data),
        19 => IntersectOp::parse_args(data),
        _ => (None, data),
    };

    let op = op?;
    Some(NextOperation {
        op,
        set_index,
        data,
    })
}

fn process_op_codes(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut int_set_0 = IntSet::<u32>::empty();
    let mut int_set_1 = IntSet::<u32>::empty();
    let mut btree_set_0 = BTreeSet::<u32>::new();
    let mut btree_set_1 = BTreeSet::<u32>::new();

    let mut ops = 0usize;
    let mut data = data;
    loop {
        let next_op = next_operation(data);
        let Some(next_op) = next_op else {
            break;
        };

        data = next_op.data;

        {
            let btree_set = if next_op.set_index == 0 {
                &btree_set_0
            } else {
                &btree_set_1
            };
            // when computing size use minimum length of 2 to ensure minimum value of log2(length) is 1.
            ops = ops.saturating_add(next_op.op.size(max(2, btree_set.len())));
            if ops > OPERATION_COUNT {
                // Operation count limit reached.
                break;
            }
        }

        let (input, other) = if next_op.set_index == 0 {
            (
                Input::from(&mut int_set_0, &mut btree_set_0),
                Input::from(&mut int_set_1, &mut btree_set_1),
            )
        } else {
            (
                Input::from(&mut int_set_1, &mut btree_set_1),
                Input::from(&mut int_set_0, &mut btree_set_0),
            )
        };

        next_op.op.operate(input, other);
    }

    assert!(int_set_0.iter().eq(btree_set_0.iter().copied()));
    assert!(int_set_1.iter().eq(btree_set_1.iter().copied()));
    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = process_op_codes(data);
});
