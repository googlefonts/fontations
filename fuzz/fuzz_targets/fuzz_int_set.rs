#![no_main]

use std::{collections::BTreeSet, error::Error};

use int_set::IntSet;
use libfuzzer_sys::fuzz_target;

// TODO allow inverted sets to be accessed.
// TODO allow a limited domain set to be accessed.

// TODO ops for most public api methods (have operations for iter() be what checks for
//      iter() equality alongside the check at end):
// - iter
// - iter ranges
// - extend / extend_unsorted
// - remove_all
// - union
// - intersect
// - first
// - last
// - contains
// - is_empty
// - len
// - clear

trait Operation {
    fn size(&self) -> u64;
    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>);
}

struct InsertOp(u32);

impl InsertOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        if data.len() < 4 {
            return (None, data);
        }

        (Some(Box::new(InsertOp(read_u32(data)))), &data[4..])
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

struct InsertRangeOp(u32, u32);

impl InsertRangeOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        if data.len() < 8 {
            return (None, data);
        }

        (
            Some(Box::new(InsertRangeOp(
                read_u32(data),
                read_u32(&data[4..]),
            ))),
            &data[8..],
        )
    }
}

impl Operation for InsertRangeOp {
    fn size(&self) -> u64 {
        if self.1 > self.0 {
            self.1 as u64 - self.0 as u64
        } else {
            1
        }
    }

    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.insert_range(self.0..=self.1);
        for v in self.0..=self.1 {
            btree_set.insert(v);
        }
    }
}

struct RemoveOp(u32);

impl RemoveOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        if data.len() < 4 {
            return (None, data);
        }

        (Some(Box::new(RemoveOp(read_u32(data)))), &data[4..])
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

struct RemoveRangeOp(u32, u32);

impl RemoveRangeOp {
    fn new(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
        if data.len() < 8 {
            return (None, data);
        }

        (
            Some(Box::new(RemoveRangeOp(
                read_u32(data),
                read_u32(&data[4..]),
            ))),
            &data[8..],
        )
    }
}

impl Operation for RemoveRangeOp {
    fn size(&self) -> u64 {
        if self.1 > self.0 {
            self.1 as u64 - self.0 as u64
        } else {
            1
        }
    }

    fn operate(&self, int_set: &mut IntSet<u32>, btree_set: &mut BTreeSet<u32>) {
        int_set.remove_range(self.0..=self.1);
        for v in self.0..=self.1 {
            btree_set.remove(&v);
        }
    }
}

fn read_u32(data: &[u8]) -> u32 {
    ((data[0] as u32) << 24) | ((data[1] as u32) << 16) | ((data[2] as u32) << 8) | (data[3] as u32)
}

fn next_operation(data: &[u8]) -> (Option<Box<dyn Operation>>, &[u8]) {
    let Some(next_byte) = data.get(0) else {
        return (None, &data[1..]);
    };

    let data = &data[1..];
    match next_byte {
        1 => InsertOp::new(data),
        2 => RemoveOp::new(data),
        3 => InsertRangeOp::new(data),
        4 => RemoveRangeOp::new(data),
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
