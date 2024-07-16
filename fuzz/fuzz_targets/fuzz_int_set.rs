#![no_main]

use std::cmp::max;
use std::ops::Bound::Excluded;
use std::ops::Bound::Included;
use std::ops::RangeInclusive;
use std::{collections::BTreeSet, error::Error};

use int_set::IntSet;
use libfuzzer_sys::fuzz_target;

// TODO allow inverted sets to be accessed.
// TODO allow a limited domain set to be accessed.

trait Operation {
    fn size(&self, set_len: usize) -> usize;
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>);
}

/* ### Insert ### */

struct InsertOp(u32);

impl InsertOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(val))), data)
    }
}

impl Operation for InsertOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.insert(self.0);
        btree_set.insert(self.0);
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### Insert Range ### */

struct InsertRangeOp(u32, u32);

impl InsertRangeOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
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

    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.insert_range(self.0..=self.1);
        for v in self.0..=self.1 {
            btree_set.insert(v);
        }
    }
}

/* ### Remove ### */

struct RemoveOp(u32);

impl RemoveOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
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

    fn operate(&self, int_set: &mut IntSet<u32>, hash_set: &mut BTreeSet<u32>) {
        int_set.remove(self.0);
        hash_set.remove(&self.0);
    }
}

/* ### Remove Range ### */

struct RemoveRangeOp(u32, u32);

impl RemoveRangeOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
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

    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.remove_range(self.0..=self.1);
        for v in self.0..=self.1 {
            btree_set.remove(&v);
        }
    }
}

/* ### Length ### */
struct LengthOp();

impl LengthOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for LengthOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        assert_eq!(int_set.len(), btree_set.len());
    }

    fn size(&self, _: usize) -> usize {
        1
    }
}

/* ### Is Empty ### */

struct IsEmptyOp();

impl IsEmptyOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for IsEmptyOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        assert_eq!(int_set.is_empty(), btree_set.is_empty());
    }

    fn size(&self, _: usize) -> usize {
        1
    }
}

/* ### Contains ### */
struct ContainsOp(u32);

impl ContainsOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(val))), data)
    }
}

impl Operation for ContainsOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        assert_eq!(int_set.contains(self.0), btree_set.contains(&self.0));
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### Clear  ### */

struct ClearOp();

impl ClearOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for ClearOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.clear();
        btree_set.clear();
    }

    fn size(&self, length: usize) -> usize {
        length
    }
}

/* ### Intersects Range ### */

struct IntersectsRangeOp(u32, u32);

impl IntersectsRangeOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
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
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        let int_set_intersects = int_set.intersects_range(self.0..=self.1);

        let mut btree_intersects = false;
        for v in self.0..=self.1 {
            if btree_set.contains(&v) {
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
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for FirstOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        assert_eq!(int_set.first(), btree_set.first().copied());
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### First  ### */

struct LastOp();

impl LastOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for LastOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        assert_eq!(int_set.last(), btree_set.last().copied());
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### Iter ### */

struct IterOp();

impl IterOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for IterOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        assert!(int_set.iter().eq(btree_set.iter().copied()));
    }

    fn size(&self, length: usize) -> usize {
        return length as usize;
    }
}

/* ### Iter Ranges ### */

struct IterRangesOp();

impl IterRangesOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        (Some(Box::new(Self())), data)
    }
}

impl Operation for IterRangesOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        let mut btree_ranges: Vec<RangeInclusive<u32>> = vec![];
        let mut cur_range: Option<RangeInclusive<u32>> = None;

        for v in btree_set.iter().copied() {
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

        assert!(int_set.iter_ranges().eq(btree_ranges.iter().cloned()));
    }

    fn size(&self, length: usize) -> usize {
        return length as usize;
    }
}

/* ### Iter After ### */

struct IterAfterOp(u32);

impl IterAfterOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(val))), data)
    }
}

impl Operation for IterAfterOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        let it = int_set.iter_after(self.0);
        let btree_it = btree_set.range((Excluded(self.0), Included(u32::MAX)));
        assert!(it.eq(btree_it.copied()));
    }

    fn size(&self, length: usize) -> usize {
        return length as usize;
    }
}

/* ### Remove All ### */

struct RemoveAllOp(Vec<u32>);

impl RemoveAllOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(values))), data)
    }
}

impl Operation for RemoveAllOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.remove_all(self.0.iter().copied());
        for v in self.0.iter() {
            btree_set.remove(&v);
        }
    }

    fn size(&self, length: usize) -> usize {
        return (length.ilog2() as usize) * self.0.len();
    }
}

/* ### Extend ### */

struct ExtendOp(Vec<u32>);

impl ExtendOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(values))), data)
    }
}

impl Operation for ExtendOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.extend(self.0.iter().copied());
        btree_set.extend(self.0.iter().copied());
    }

    fn size(&self, length: usize) -> usize {
        return (length.ilog2() as usize) * self.0.len();
    }
}

/* ### Extend Unsorted ### */

struct ExtendUnsortedOp(Vec<u32>);

impl ExtendUnsortedOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(values))), data)
    }
}

impl Operation for ExtendUnsortedOp {
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.extend_unsorted(self.0.iter().copied());
        btree_set.extend(self.0.iter().copied());
    }

    fn size(&self, length: usize) -> usize {
        return (length.ilog2() as usize) * self.0.len();
    }
}

/* ### End of Ops ### */

fn read_u8(data: &[u8]) -> (Option<u8>, &[u8]) {
    if data.len() < 1 {
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

fn next_operation(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
    let Some(op_code) = data.get(0) else {
        return (None, &data[1..]);
    };

    // TODO ops for most public api methods (have operations for iter() be what checks for
    //      iter() equality alongside the check at end):
    // - union
    // - intersect
    let data = &data[1..];
    match op_code {
        1 => InsertOp::new(data),
        2 => RemoveOp::new(data),
        3 => InsertRangeOp::new(data),
        4 => RemoveRangeOp::new(data),
        5 => LengthOp::new(data),
        6 => IsEmptyOp::new(data),
        7 => ContainsOp::new(data),
        8 => ClearOp::new(data),
        9 => IntersectsRangeOp::new(data),
        10 => FirstOp::new(data),
        11 => LastOp::new(data),
        12 => IterOp::new(data),
        13 => IterRangesOp::new(data),
        14 => IterAfterOp::new(data),
        15 => RemoveAllOp::new(data),
        16 => ExtendOp::new(data),
        17 => ExtendUnsortedOp::new(data),
        _ => (None, data),
    }
}

fn do_int_set_things(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut int_set = IntSet::<u32>::empty();
    let mut btree_set = BTreeSet::<u32>::new();

    let mut ops = 0usize;
    let mut data = data;
    while !data.is_empty() {
        let (op, new_data) = next_operation(data);
        data = new_data;

        let Some(op) = op else {
            break;
        };

        // when computing size use minimum length of 2 to ensure minimum value of log2(length) is 1.
        ops = ops.saturating_add(op.size(max(2, btree_set.len())));
        if ops > 5000 {
            break;
        }

        op.operate(&mut int_set, &mut btree_set);
    }

    assert!(int_set.iter().eq(btree_set.into_iter()));
    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = do_int_set_things(data);
});
