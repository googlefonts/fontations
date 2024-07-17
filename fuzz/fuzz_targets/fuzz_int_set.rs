#![no_main]
//! A correctness fuzzer that checks all of the public API methods of [IntSet].
//!
//! This fuzzer exercises the public API of IntSet and compares the results to the same operations run
//! on a BTreeSet. Any differences in behaviour and/or set contents triggers a panic.
//!
//! The fuzzer input data is interpreted as a series of op codes which map to the public api methods of IntSet.
//!
//! Note: currently only inclusive mode IntSet's are tested.

use std::cmp::max;

use std::cmp::min;
use std::fmt::Debug;
use std::ops::Add;
use std::ops::Bound::Excluded;
use std::ops::Bound::Included;
use std::ops::RangeInclusive;
use std::{collections::BTreeSet, error::Error};

use int_set::Domain;
use int_set::InDomain;
use int_set::IntSet;
use libfuzzer_sys::fuzz_target;

const OPERATION_COUNT: usize = 7_500;

// TODO(garretrieger): use "Cursor" to manage the input buffer.
// TODO(garretrieger): allow inverted sets to be accessed.
// TODO(garretrieger): allow a limited domain set to be accessed.

trait SetMember<T>: Domain<T> + Ord + Copy + Add<u32, Output = T> + Debug {
    fn create(val: u32) -> Option<T>;
    fn increment(&mut self);
}

impl SetMember<u32> for u32 {
    fn create(val: u32) -> Option<u32> {
        Some(val)
    }

    fn increment(&mut self) {
        *self = self.saturating_add(1);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord)]
struct SmallInt(u32);

impl SmallInt {
    const MAX_VALUE: u32 = 4 * 512;

    fn new(value: u32) -> SmallInt {
        if value > Self::MAX_VALUE {
            panic!("Constructed SmallInt with value > MAX_VALUE");
        }
        SmallInt(value)
    }
}

impl Add<u32> for SmallInt {
    type Output = SmallInt;

    fn add(self, rhs: u32) -> Self::Output {
        SmallInt::new(self.0 + rhs)
    }
}

impl SetMember<SmallInt> for SmallInt {
    fn create(val: u32) -> Option<SmallInt> {
        if val > Self::MAX_VALUE {
            return None;
        }
        Some(SmallInt::new(val))
    }

    fn increment(&mut self) {
        self.0 = min(self.0 + 1, Self::MAX_VALUE);
    }
}

impl Domain<SmallInt> for SmallInt {
    fn to_u32(&self) -> u32 {
        self.0
    }

    fn from_u32(member: InDomain) -> SmallInt {
        SmallInt::new(member.value())
    }

    fn is_continous() -> bool {
        true
    }

    fn ordered_values() -> impl DoubleEndedIterator<Item = u32> {
        0..=Self::MAX_VALUE
    }

    fn ordered_values_range(
        range: RangeInclusive<SmallInt>,
    ) -> impl DoubleEndedIterator<Item = u32> {
        if range.start().0 > Self::MAX_VALUE || range.end().0 > Self::MAX_VALUE {
            panic!("Invalid range of the SmallInt set.");
        }
        range.start().to_u32()..=range.end().to_u32()
    }
}

struct Input<'a, T>
where
    T: SetMember<T>,
{
    // The state includes 2 of each type of sets to allow us to test out binary set operations (eg. union)
    int_set: &'a mut IntSet<T>,
    btree_set: &'a mut BTreeSet<T>,
}

impl<T> Input<'_, T>
where
    T: SetMember<T>,
{
    fn from<'a>(int_set: &'a mut IntSet<T>, btree_set: &'a mut BTreeSet<T>) -> Input<'a, T> {
        Input { int_set, btree_set }
    }
}

trait Operation<T>
where
    T: SetMember<T>,
{
    fn size(&self, set_len: usize) -> usize;
    fn operate(&self, input: Input<T>, other: Input<T>);
}

/* ### Insert ### */

struct InsertOp<T>(T)
where
    T: SetMember<T>;

impl<T> InsertOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(InsertOp::<T>(val))), data)
    }
}

impl<T> Operation<T> for InsertOp<T>
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.insert(self.0);
        input.btree_set.insert(self.0);
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### Insert Range ### */

struct InsertRangeOp<T>(T, T)
where
    T: SetMember<T>;

impl<T> InsertRangeOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(min), data) = read_u32(data) else {
            return (None, data);
        };
        let (Some(max), data) = read_u32(data) else {
            return (None, data);
        };

        (Some(Box::new(InsertRangeOp::<T>(min, max))), data)
    }
}

impl<T> Operation<T> for InsertRangeOp<T>
where
    T: SetMember<T>,
{
    fn size(&self, length: usize) -> usize {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1.to_u32() as usize - self.0.to_u32() as usize) + 1) * (length.ilog2() as usize)
    }

    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.insert_range(self.0..=self.1);

        let mut v = self.0;
        for _ in self.0.to_u32()..=self.1.to_u32() {
            input.btree_set.insert(v);
            v.increment();
        }
    }
}

/* ### Remove ### */

struct RemoveOp<T>(T)
where
    T: SetMember<T>;

impl<T> RemoveOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(RemoveOp::<T>(val))), data)
    }
}

impl<T> Operation<T> for RemoveOp<T>
where
    T: SetMember<T>,
{
    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }

    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.remove(self.0);
        input.btree_set.remove(&self.0);
    }
}

/* ### Remove Range ### */

struct RemoveRangeOp<T>(T, T)
where
    T: SetMember<T>;

impl<T> RemoveRangeOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(min), data) = read_u32(data) else {
            return (None, data);
        };
        let (Some(max), data) = read_u32(data) else {
            return (None, data);
        };

        (Some(Box::new(RemoveRangeOp::<T>(min, max))), data)
    }
}

impl<T> Operation<T> for RemoveRangeOp<T>
where
    T: SetMember<T>,
{
    fn size(&self, length: usize) -> usize {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1.to_u32() as usize - self.0.to_u32() as usize) + 1) * (length.ilog2() as usize)
    }

    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.remove_range(self.0..=self.1);
        let mut v = self.0;
        for _ in self.0.to_u32()..=self.1.to_u32() {
            input.btree_set.remove(&v);
            v.increment();
        }
    }
}

/* ### Length ### */
struct LengthOp();

impl LengthOp {
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for LengthOp
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(input.int_set.len(), input.btree_set.len());
    }

    fn size(&self, _: usize) -> usize {
        1
    }
}

/* ### Is Empty ### */

struct IsEmptyOp();

impl IsEmptyOp {
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for IsEmptyOp
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(input.int_set.is_empty(), input.btree_set.is_empty());
    }

    fn size(&self, _: usize) -> usize {
        1
    }
}

/* ### Contains ### */
struct ContainsOp<T>(T)
where
    T: SetMember<T>;

impl<T> ContainsOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(ContainsOp::<T>(val))), data)
    }
}

impl<T> Operation<T> for ContainsOp<T>
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
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
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for ClearOp
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.clear();
        input.btree_set.clear();
    }

    fn size(&self, length: usize) -> usize {
        length
    }
}

/* ### Intersects Range ### */

struct IntersectsRangeOp<T>(T, T)
where
    T: SetMember<T>;

impl<T> IntersectsRangeOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(min), data) = read_u32(data) else {
            return (None, data);
        };
        let (Some(max), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(IntersectsRangeOp::<T>(min, max))), data)
    }
}

impl<T> Operation<T> for IntersectsRangeOp<T>
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        let int_set_intersects = input.int_set.intersects_range(self.0..=self.1);

        let mut btree_intersects = false;
        let mut v = self.0;
        for _ in self.0.to_u32()..=self.1.to_u32() {
            if input.btree_set.contains(&v) {
                btree_intersects = true;
                break;
            }
            v.increment();
        }

        assert_eq!(int_set_intersects, btree_intersects);
    }

    fn size(&self, length: usize) -> usize {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1.to_u32() as usize - self.0.to_u32() as usize) + 1) * (length.ilog2() as usize)
    }
}

/* ### First  ### */

struct FirstOp();

impl FirstOp {
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for FirstOp
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(input.int_set.first(), input.btree_set.first().copied());
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### First  ### */

struct LastOp();

impl LastOp {
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for LastOp
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(input.int_set.last(), input.btree_set.last().copied());
    }

    fn size(&self, length: usize) -> usize {
        length.ilog2() as usize
    }
}

/* ### Iter ### */

struct IterOp();

impl IterOp {
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for IterOp
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert!(input.int_set.iter().eq(input.btree_set.iter().copied()));
    }

    fn size(&self, length: usize) -> usize {
        length
    }
}

/* ### Iter Ranges ### */

struct IterRangesOp();

impl IterRangesOp {
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for IterRangesOp
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        let mut btree_ranges: Vec<RangeInclusive<T>> = vec![];
        let mut cur_range: Option<RangeInclusive<T>> = None;

        for v in input.btree_set.iter().copied() {
            if let Some(range) = cur_range {
                if *range.end() + 1 == v {
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

struct IterAfterOp<T>(T)
where
    T: SetMember<T>;

impl<T> IterAfterOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(val), data) = read_u32(data) else {
            return (None, data);
        };
        (Some(Box::new(IterAfterOp::<T>(val))), data)
    }
}

impl<T> Operation<T> for IterAfterOp<T>
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        let domain_max = T::create(T::ordered_values().next_back().unwrap()).unwrap();
        let it = input.int_set.iter_after(self.0);
        let btree_it = input
            .btree_set
            .range((Excluded(self.0), Included(domain_max)));
        assert!(it.eq(btree_it.copied()));
    }

    fn size(&self, length: usize) -> usize {
        length
    }
}

/* ### Remove All ### */

struct RemoveAllOp<T>(Vec<T>)
where
    T: SetMember<T>;

impl<T> RemoveAllOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(RemoveAllOp::<T>(values))), data)
    }
}

impl<T> Operation<T> for RemoveAllOp<T>
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
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

struct ExtendOp<T>(Vec<T>)
where
    T: SetMember<T>;

impl<T> ExtendOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(values))), data)
    }
}

impl<T> Operation<T> for ExtendOp<T>
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.extend(self.0.iter().copied());
        input.btree_set.extend(self.0.iter().copied());
    }

    fn size(&self, length: usize) -> usize {
        (length.ilog2() as usize) * self.0.len()
    }
}

/* ### Extend Unsorted ### */

struct ExtendUnsortedOp<T>(Vec<T>)
where
    T: SetMember<T>;

impl<T> ExtendUnsortedOp<T>
where
    T: SetMember<T> + 'static,
{
    fn parse_args(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8]) {
        let (Some(values), data) = read_u32_vec(data) else {
            return (None, data);
        };
        (Some(Box::new(Self(values))), data)
    }
}

impl<T> Operation<T> for ExtendUnsortedOp<T>
where
    T: SetMember<T>,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
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
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for UnionOp
where
    T: SetMember<T>,
{
    fn operate(&self, a: Input<T>, b: Input<T>) {
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
    fn parse_args<T>(data: &[u8]) -> (Option<Box<dyn Operation<T>>>, &[u8])
    where
        T: SetMember<T>,
    {
        (Some(Box::new(Self())), data)
    }
}

impl<T> Operation<T> for IntersectOp
where
    T: SetMember<T>,
{
    fn operate(&self, a: Input<T>, b: Input<T>) {
        a.int_set.intersect(b.int_set);
        let mut intersected: BTreeSet<T> = a.btree_set.intersection(b.btree_set).copied().collect();
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

fn read_u32<T: SetMember<T>>(data: &[u8]) -> (Option<T>, &[u8]) {
    if data.len() < 4 {
        return (None, data);
    }

    let u32_val = ((data[0] as u32) << 24)
        | ((data[1] as u32) << 16)
        | ((data[2] as u32) << 8)
        | (data[3] as u32);

    (T::create(u32_val), &data[4..])
}

fn read_u32_vec<T: SetMember<T>>(data: &[u8]) -> (Option<Vec<T>>, &[u8]) {
    let (Some(count), data) = read_u8(data) else {
        return (None, data);
    };

    let mut values: Vec<T> = vec![];
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

struct NextOperation<'a, T>
where
    T: SetMember<T>,
{
    op: Box<dyn Operation<T>>,
    set_index: usize,
    data: &'a [u8],
}

fn next_operation<T>(data: &[u8]) -> Option<NextOperation<T>>
where
    T: SetMember<T> + 'static,
{
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
