#![no_main]

use std::{collections::BTreeSet, error::Error};

use int_set::IntSet;
use libfuzzer_sys::fuzz_target;

// TODO allow inverted sets to be accessed.
// TODO allow a limited domain set to be accessed.

trait Operation {
    fn size(&self) -> u64;
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

    fn size(&self) -> u64 {
        1
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
    fn size(&self) -> u64 {
        if self.1 < self.0 {
            return 1;
        }
        (self.1 as u64 - self.0 as u64) + 1
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
    fn size(&self) -> u64 {
        1
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
    fn size(&self) -> u64 {
        if self.1 < self.0 {
            return 1;
        }
        (self.1 as u64 - self.0 as u64) + 1
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

    fn size(&self) -> u64 {
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

    fn size(&self) -> u64 {
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

    fn size(&self) -> u64 {
        1
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

    fn size(&self) -> u64 {
        1
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

    fn size(&self) -> u64 {
        if self.1 < self.0 {
            return 1;
        }
        (self.1 as u64 - self.0 as u64) + 1
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
        assert_eq!(int_set.first(), btree_set.iter().next().copied());
    }

    fn size(&self) -> u64 {
        1
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
        assert_eq!(int_set.last(), btree_set.iter().next_back().copied());
    }

    fn size(&self) -> u64 {
        1
    }
}

/* ### End of Ops ### */

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

fn next_operation(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
    let Some(op_code) = data.get(0) else {
        return (None, &data[1..]);
    };

    // TODO ops for most public api methods (have operations for iter() be what checks for
    //      iter() equality alongside the check at end):
    // - iter
    // - iter ranges
    // - iter after
    // - extend / extend_unsorted
    // - remove_all
    // - union
    // - intersect
    // - first
    // - last
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
        _ => (None, data),
    }
}

fn do_int_set_things(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut int_set = IntSet::<u32>::empty();
    let mut btree_set = BTreeSet::<u32>::new();

    let mut ops = 0u64;
    let mut data = data;
    while !data.is_empty() {
        let (op, new_data) = next_operation(data);
        data = new_data;

        let Some(op) = op else {
            break;
        };

        ops = ops.saturating_add(op.size());
        if ops > 1000 {
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
