use font_types::{GlyphId, GlyphId16};
use std::fmt::Debug;
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::io::Cursor;
use std::io::Read;
use std::iter::Map;
use std::ops::Bound::Excluded;
use std::ops::Bound::Included;
use std::ops::RangeInclusive;
use std::{collections::BTreeSet, error::Error};

use read_fonts::collections::int_set::{Domain, InDomain, IntSet};

#[derive(PartialEq, Clone, Copy)]
pub enum OperationSet {
    Standard,
    #[allow(dead_code)]
    SparseBitSetEncoding(u32, u32),
}

pub trait SetMember: Sized + Domain + Ord + Copy + Debug + Hash {
    fn create(val: u32) -> Option<Self>;
    fn can_be_inverted() -> bool;
    fn increment(&mut self);
}

impl SetMember for u32 {
    fn create(val: u32) -> Option<u32> {
        Some(val)
    }

    fn can_be_inverted() -> bool {
        false
    }

    fn increment(&mut self) {
        *self = self.saturating_add(1);
    }
}

impl SetMember for u16 {
    fn create(val: u32) -> Option<u16> {
        val.try_into().ok()
    }

    fn can_be_inverted() -> bool {
        false
    }

    fn increment(&mut self) {
        *self = self.saturating_add(1);
    }
}

impl SetMember for u8 {
    fn create(val: u32) -> Option<u8> {
        val.try_into().ok()
    }

    fn can_be_inverted() -> bool {
        false
    }

    fn increment(&mut self) {
        *self = self.saturating_add(1);
    }
}

impl SetMember for GlyphId16 {
    fn create(val: u32) -> Option<GlyphId16> {
        let val: u16 = val.try_into().ok()?;
        Some(GlyphId16::new(val))
    }

    fn can_be_inverted() -> bool {
        false
    }

    fn increment(&mut self) {
        *self = GlyphId16::new(self.to_u16().saturating_add(1));
    }
}

impl SetMember for GlyphId {
    fn create(val: u32) -> Option<GlyphId> {
        Some(GlyphId::new(val))
    }

    fn can_be_inverted() -> bool {
        false
    }

    fn increment(&mut self) {
        *self = GlyphId::new(self.to_u32().saturating_add(1));
    }
}

/// This is an integer in the domain of [0, 2048). It's used by the fuzzer
/// for testing inverted sets to avoid causing excessively long running operations
/// and memory usage on the btree set kept along side the IntSet.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct SmallInt(u32);

impl SmallInt {
    const MAX_VALUE: u32 = 4 * 512 - 1;

    fn new(value: u32) -> SmallInt {
        assert!(
            value <= Self::MAX_VALUE,
            "Constructed SmallInt with value > MAX_VALUE"
        );
        SmallInt(value)
    }
}

impl SetMember for SmallInt {
    fn create(val: u32) -> Option<SmallInt> {
        if val > Self::MAX_VALUE {
            return None;
        }
        Some(SmallInt::new(val))
    }

    fn can_be_inverted() -> bool {
        true
    }

    fn increment(&mut self) {
        self.0 = (self.0 + 1).min(Self::MAX_VALUE);
    }
}

impl Domain for SmallInt {
    type OrderedValues = RangeInclusive<u32>;

    fn to_u32(&self) -> u32 {
        self.0
    }

    fn from_u32(member: InDomain) -> SmallInt {
        SmallInt::new(member.value())
    }

    fn contains(value: u32) -> bool {
        value <= Self::MAX_VALUE
    }

    fn is_continuous() -> bool {
        true
    }

    fn ordered_values() -> Self::OrderedValues {
        0..=Self::MAX_VALUE
    }

    fn ordered_values_range(range: RangeInclusive<SmallInt>) -> Self::OrderedValues {
        assert!(
            range.start().0 <= Self::MAX_VALUE && range.end().0 <= Self::MAX_VALUE,
            "Invalid range of the SmallInt set."
        );
        range.start().to_u32()..=range.end().to_u32()
    }

    fn count() -> u64 {
        Self::MAX_VALUE as u64 + 1
    }
}

/// This is an even integer in the domain of [0, 2048). It's used by the fuzzer
/// for testing inverted sets + discontinuous domains.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct SmallEvenInt(u32);

impl SmallEvenInt {
    const MAX_VALUE: u32 = 4 * 512 - 2;

    fn new(value: u32) -> SmallEvenInt {
        assert!(
            value <= Self::MAX_VALUE,
            "Constructed SmallEvenInt with value > MAX_VALUE."
        );
        assert!(
            value % 2 == 0,
            "Constructed SmallEvenInt with an odd value."
        );
        SmallEvenInt(value)
    }
}

impl SetMember for SmallEvenInt {
    fn create(val: u32) -> Option<SmallEvenInt> {
        if val > Self::MAX_VALUE || val % 2 != 0 {
            return None;
        }
        Some(SmallEvenInt::new(val))
    }

    fn can_be_inverted() -> bool {
        true
    }

    fn increment(&mut self) {
        self.0 = (self.0 + 2).min(Self::MAX_VALUE);
    }
}

impl Domain for SmallEvenInt {
    type OrderedValues = Map<RangeInclusive<u32>, fn(u32) -> u32>;

    fn to_u32(&self) -> u32 {
        self.0
    }

    fn from_u32(member: InDomain) -> SmallEvenInt {
        SmallEvenInt::new(member.value())
    }

    fn contains(value: u32) -> bool {
        (value % 2) == 0 && value <= Self::MAX_VALUE
    }

    fn is_continuous() -> bool {
        false
    }

    fn ordered_values() -> Self::OrderedValues {
        fn double(input: u32) -> u32 {
            input * 2
        }
        (0..=(Self::MAX_VALUE / 2)).map(double)
    }

    fn ordered_values_range(range: RangeInclusive<SmallEvenInt>) -> Self::OrderedValues {
        assert!(
            range.start().0 <= Self::MAX_VALUE && range.end().0 <= Self::MAX_VALUE,
            "Invalid range of the SmallInt set."
        );
        ((range.start().to_u32() / 2)..=(range.end().to_u32() / 2)).map(|ord| ord * 2)
    }

    fn count() -> u64 {
        ((Self::MAX_VALUE / 2) + 1) as u64
    }
}

struct Input<'a, T> {
    // The state includes 2 of each type of sets to allow us to test out binary set operations (eg. union)
    int_set: &'a mut IntSet<T>,
    btree_set: &'a mut BTreeSet<T>,
}

impl<T> Input<'_, T> {
    fn from<'a>(int_set: &'a mut IntSet<T>, btree_set: &'a mut BTreeSet<T>) -> Input<'a, T> {
        Input { int_set, btree_set }
    }
}

trait Operation<T> {
    fn size(&self, set_len: u64) -> u64;
    fn operate(&self, input: Input<T>, other: Input<T>);
}

/* ### Insert ### */

struct InsertOp<T>(T);

impl<T> InsertOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        Some(Box::new(InsertOp::<T>(read_u32(data)?)))
    }
}

impl<T> Operation<T> for InsertOp<T>
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.insert(self.0);
        input.btree_set.insert(self.0);
    }

    fn size(&self, length: u64) -> u64 {
        length.ilog2() as u64
    }
}

/* ### Insert Range ### */

struct InsertRangeOp<T>(T, T);

impl<T> InsertRangeOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        let min = read_u32(data)?;
        let max = read_u32(data)?;
        Some(Box::new(InsertRangeOp::<T>(min, max)))
    }
}

impl<T> Operation<T> for InsertRangeOp<T>
where
    T: SetMember,
{
    fn size(&self, length: u64) -> u64 {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1.to_u32() as u64 - self.0.to_u32() as u64) + 1) * (length.ilog2() as u64)
    }

    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.insert_range(self.0..=self.1);

        let mut v = self.0;
        for _ in T::ordered_values_range(self.0..=self.1) {
            input.btree_set.insert(v);
            v.increment();
        }
    }
}

/* ### Remove ### */

struct RemoveOp<T>(T);

impl<T> RemoveOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        Some(Box::new(RemoveOp::<T>(read_u32(data)?)))
    }
}

impl<T> Operation<T> for RemoveOp<T>
where
    T: SetMember,
{
    fn size(&self, length: u64) -> u64 {
        length.ilog2() as u64
    }

    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.remove(self.0);
        input.btree_set.remove(&self.0);
    }
}

/* ### Remove Range ### */

struct RemoveRangeOp<T>(T, T);

impl<T> RemoveRangeOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        let min = read_u32(data)?;
        let max = read_u32(data)?;
        Some(Box::new(RemoveRangeOp::<T>(min, max)))
    }
}

impl<T> Operation<T> for RemoveRangeOp<T>
where
    T: SetMember,
{
    fn size(&self, length: u64) -> u64 {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1.to_u32() as u64 - self.0.to_u32() as u64) + 1) * (length.ilog2() as u64)
    }

    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.remove_range(self.0..=self.1);
        let mut v = self.0;
        for _ in T::ordered_values_range(self.0..=self.1) {
            input.btree_set.remove(&v);
            v.increment();
        }
    }
}

/* ### Length ### */
struct LengthOp;

impl LengthOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for LengthOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(input.int_set.len(), input.btree_set.len() as u64);
    }

    fn size(&self, _: u64) -> u64 {
        1
    }
}

/* ### Is Empty ### */

struct IsEmptyOp;

impl IsEmptyOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for IsEmptyOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(input.int_set.is_empty(), input.btree_set.is_empty());
    }

    fn size(&self, _: u64) -> u64 {
        1
    }
}

/* ### Contains ### */
struct ContainsOp<T>(T)
where
    T: SetMember;

impl<T> ContainsOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        Some(Box::new(ContainsOp::<T>(read_u32(data)?)))
    }
}

impl<T> Operation<T> for ContainsOp<T>
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(
            input.int_set.contains(self.0),
            input.btree_set.contains(&self.0)
        );
    }

    fn size(&self, length: u64) -> u64 {
        length.ilog2() as u64
    }
}

/* ### Clear  ### */

struct ClearOp;

impl ClearOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for ClearOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.clear();
        input.btree_set.clear();
    }

    fn size(&self, length: u64) -> u64 {
        length
    }
}

/* ### Intersects Range ### */

struct IntersectsRangeOp<T>(T, T);

impl<T> IntersectsRangeOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        let min = read_u32(data)?;
        let max = read_u32(data)?;
        Some(Box::new(IntersectsRangeOp::<T>(min, max)))
    }
}

impl<T> Operation<T> for IntersectsRangeOp<T>
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        let int_set_intersects = input.int_set.intersects_range(self.0..=self.1);

        let mut btree_intersects = false;
        let mut v = self.0;
        for _ in T::ordered_values_range(self.0..=self.1) {
            if input.btree_set.contains(&v) {
                btree_intersects = true;
                break;
            }
            v.increment();
        }

        assert_eq!(int_set_intersects, btree_intersects);
    }

    fn size(&self, length: u64) -> u64 {
        if self.1 < self.0 {
            return 1;
        }
        ((self.1.to_u32() as u64 - self.0.to_u32() as u64) + 1) * (length.ilog2() as u64)
    }
}

/* ### Intersects Set ### */

struct IntersectsSetOp;

impl IntersectsSetOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for IntersectsSetOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, other: Input<T>) {
        let intersects_int_set = input.int_set.intersects_set(other.int_set);
        let intersects_btree_set = input
            .btree_set
            .intersection(other.btree_set)
            .next()
            .is_some();
        assert_eq!(intersects_int_set, intersects_btree_set);
    }

    fn size(&self, length: u64) -> u64 {
        length * (length.ilog2() as u64)
    }
}

/* ### First  ### */

struct FirstOp;

impl FirstOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for FirstOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(input.int_set.first(), input.btree_set.first().copied());
    }

    fn size(&self, length: u64) -> u64 {
        length.ilog2() as u64
    }
}

/* ### First  ### */

struct LastOp;

impl LastOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for LastOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert_eq!(input.int_set.last(), input.btree_set.last().copied());
    }

    fn size(&self, length: u64) -> u64 {
        length.ilog2() as u64
    }
}

/* ### Iter ### */

struct IterOp;

impl IterOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for IterOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        assert!(input.int_set.iter().eq(input.btree_set.iter().copied()));
    }

    fn size(&self, length: u64) -> u64 {
        length
    }
}

/* ### InclusiveIter ### */

struct InclusiveIterOp;

impl InclusiveIterOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for InclusiveIterOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        let int_set_it = input.int_set.inclusive_iter();
        let btree_set_it = if input.int_set.is_inverted() {
            None
        } else {
            Some(input.btree_set.iter())
        };

        assert_eq!(int_set_it.is_some(), btree_set_it.is_some());
        if let (Some(a), Some(b)) = (int_set_it, btree_set_it) {
            assert!(a.eq(b.copied()));
        };
    }

    fn size(&self, length: u64) -> u64 {
        length
    }
}

/* ### Iter Ranges ### */

struct IterRangesOp;

impl IterRangesOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for IterRangesOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        let mut btree_ranges: Vec<RangeInclusive<T>> = vec![];
        let mut cur_range: Option<RangeInclusive<T>> = None;

        for v in input.btree_set.iter().copied() {
            if let Some(range) = cur_range {
                let mut end = *range.end();
                end.increment();
                if end == v {
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

    fn size(&self, length: u64) -> u64 {
        length
    }
}

/* ### Iter Excluded Ranges ### */

struct IterExcludedRangesOp;

impl IterExcludedRangesOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for IterExcludedRangesOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        let mut btree_ranges: Vec<RangeInclusive<T>> = vec![];
        let mut cur_range: Option<RangeInclusive<T>> = None;

        let inverted: BTreeSet<_> = T::ordered_values()
            .map(|v| T::create(v).unwrap())
            .filter(|v| !input.btree_set.contains(v))
            .collect();

        for v in inverted.iter().copied() {
            if let Some(range) = cur_range {
                let mut end = *range.end();
                end.increment();
                if end == v {
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

        assert!(input
            .int_set
            .iter_excluded_ranges()
            .eq(btree_ranges.iter().cloned()));
    }

    fn size(&self, length: u64) -> u64 {
        length
    }
}

/* ### Iter After ### */

struct IterAfterOp<T>(T);

impl<T> IterAfterOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        Some(Box::new(IterAfterOp::<T>(read_u32(data)?)))
    }
}

impl<T> Operation<T> for IterAfterOp<T>
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        let domain_max = T::create(T::ordered_values().next_back().unwrap()).unwrap();
        let it = input.int_set.iter_after(self.0);
        let btree_it = input
            .btree_set
            .range((Excluded(self.0), Included(domain_max)));
        assert!(it.eq(btree_it.copied()));
    }

    fn size(&self, length: u64) -> u64 {
        length
    }
}

/* ### Remove All ### */

struct RemoveAllOp<T>(Vec<T>);

impl<T> RemoveAllOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        Some(Box::new(RemoveAllOp::<T>(read_u32_vec(data)?)))
    }
}

impl<T> Operation<T> for RemoveAllOp<T>
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.remove_all(self.0.iter().copied());
        for v in self.0.iter() {
            input.btree_set.remove(v);
        }
    }

    fn size(&self, length: u64) -> u64 {
        (length.ilog2() as u64) * (self.0.len() as u64)
    }
}

/* ### Extend ### */

struct ExtendOp<T>(Vec<T>);

impl<T> ExtendOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        Some(Box::new(Self(read_u32_vec(data)?)))
    }
}

impl<T> Operation<T> for ExtendOp<T>
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.extend(self.0.iter().copied());
        input.btree_set.extend(self.0.iter().copied());
    }

    fn size(&self, length: u64) -> u64 {
        (length.ilog2() as u64) * (self.0.len() as u64)
    }
}

/* ### Extend Unsorted ### */

struct ExtendUnsortedOp<T>(Vec<T>);

impl<T> ExtendUnsortedOp<T>
where
    T: SetMember + 'static,
{
    fn parse_args(data: &mut Cursor<&[u8]>) -> Option<Box<dyn Operation<T>>> {
        Some(Box::new(Self(read_u32_vec(data)?)))
    }
}

impl<T> Operation<T> for ExtendUnsortedOp<T>
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.extend_unsorted(self.0.iter().copied());
        input.btree_set.extend(self.0.iter().copied());
    }

    fn size(&self, length: u64) -> u64 {
        (length.ilog2() as u64) * (self.0.len() as u64)
    }
}

/* ### Union ### */

struct UnionOp;

impl UnionOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for UnionOp
where
    T: SetMember,
{
    fn operate(&self, a: Input<T>, b: Input<T>) {
        a.int_set.union(b.int_set);
        for v in b.btree_set.iter() {
            a.btree_set.insert(*v);
        }
    }

    fn size(&self, length: u64) -> u64 {
        // TODO(garretrieger): should be length a + length b
        length
    }
}

/* ### Intersect ### */

struct IntersectOp;

impl IntersectOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for IntersectOp
where
    T: SetMember,
{
    fn operate(&self, a: Input<T>, b: Input<T>) {
        a.int_set.intersect(b.int_set);
        let mut intersected: BTreeSet<T> = a.btree_set.intersection(b.btree_set).copied().collect();
        std::mem::swap(a.btree_set, &mut intersected);
    }

    fn size(&self, length: u64) -> u64 {
        // TODO(garretrieger): should be length a + length b
        length
    }
}

/* ### Invert ### */

struct InvertOp;

impl InvertOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for InvertOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.invert();

        let mut inverted: BTreeSet<_> = T::ordered_values()
            .map(|v| T::create(v).unwrap())
            .filter(|v| !input.btree_set.contains(v))
            .collect();
        std::mem::swap(input.btree_set, &mut inverted);
    }

    fn size(&self, _: u64) -> u64 {
        T::count()
    }
}

/* ### Is Inverted ### */

struct IsInvertedOp;

impl IsInvertedOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for IsInvertedOp
where
    T: SetMember,
{
    fn operate(&self, input: Input<T>, _: Input<T>) {
        input.int_set.is_inverted();
    }

    fn size(&self, _: u64) -> u64 {
        1
    }
}

/* ### Hash ### */

struct HashOp;

impl HashOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl HashOp {
    fn hash<T: SetMember>(a: Input<T>) -> u64 {
        let mut hasher = DefaultHasher::new();
        a.int_set.hash(&mut hasher);
        hasher.finish()
    }
}

impl<T> Operation<T> for HashOp
where
    T: SetMember,
{
    fn operate(&self, a: Input<T>, b: Input<T>) {
        if a.int_set == b.int_set {
            assert_eq!(Self::hash(a), Self::hash(b));
        }
    }

    fn size(&self, length: u64) -> u64 {
        length
    }
}

/* ### Equal ### */

struct EqualOp;

impl EqualOp {
    fn parse_args<T>() -> Option<Box<dyn Operation<T>>>
    where
        T: SetMember,
    {
        Some(Box::new(Self))
    }
}

impl<T> Operation<T> for EqualOp
where
    T: SetMember,
{
    fn operate(&self, a: Input<T>, b: Input<T>) {
        assert_eq!(a.int_set == b.int_set, a.btree_set == b.btree_set);
    }

    fn size(&self, length: u64) -> u64 {
        length
    }
}

struct CmpOp;

impl CmpOp {
    fn parse_args<T: SetMember>() -> Option<Box<dyn Operation<T>>> {
        Some(Box::new(Self))
    }
}

impl<T: SetMember> Operation<T> for CmpOp {
    fn operate(&self, a: Input<T>, b: Input<T>) {
        assert_eq!(a.int_set.cmp(&b.int_set), a.btree_set.cmp(&b.btree_set));
    }

    fn size(&self, length: u64) -> u64 {
        length
    }
}

/* ### End of Ops ### */

fn read_u8(data: &mut Cursor<&[u8]>) -> Option<u8> {
    let mut byte_val = [0];
    data.read_exact(&mut byte_val).ok()?;
    Some(byte_val[0])
}

pub(crate) fn read_u32<T: SetMember>(data: &mut Cursor<&[u8]>) -> Option<T> {
    let mut byte_val = [0, 0, 0, 0];
    data.read_exact(&mut byte_val).ok()?;

    let u32_val = ((byte_val[0] as u32) << 24)
        | ((byte_val[1] as u32) << 16)
        | ((byte_val[2] as u32) << 8)
        | (byte_val[3] as u32);

    T::create(u32_val)
}

fn read_u32_vec<T: SetMember>(data: &mut Cursor<&[u8]>) -> Option<Vec<T>> {
    let count = read_u8(data)?;
    let mut values: Vec<T> = vec![];
    for _ in 0..count {
        values.push(read_u32(data)?);
    }
    Some(values)
}

struct NextOperation<T>
where
    T: SetMember,
{
    op: Box<dyn Operation<T>>,
    set_index: usize,
}

fn next_operation<T>(
    operation_set: OperationSet,
    data: &mut Cursor<&[u8]>,
) -> Option<NextOperation<T>>
where
    T: SetMember + 'static,
{
    let op_code = read_u8(data)?;

    // Check the msb of op code to see which set index to use.
    const INDEX_MASK: u8 = 0b10000000;
    let is_standard = operation_set == OperationSet::Standard;
    let set_index = if (op_code & INDEX_MASK) > 0 && is_standard {
        1
    } else {
        0
    };
    let op_code = !INDEX_MASK & op_code;

    let op = match op_code {
        1 => InsertOp::parse_args(data),
        2 if is_standard => RemoveOp::parse_args(data),
        3 => InsertRangeOp::parse_args(data),
        4 if is_standard => RemoveRangeOp::parse_args(data),
        5 if is_standard => LengthOp::parse_args(),
        6 if is_standard => IsEmptyOp::parse_args(),
        7 if is_standard => ContainsOp::parse_args(data),
        8 if is_standard => ClearOp::parse_args(),
        9 if is_standard => IntersectsRangeOp::parse_args(data),
        10 if is_standard => FirstOp::parse_args(),
        11 if is_standard => LastOp::parse_args(),
        12 if is_standard => IterOp::parse_args(),
        13 if is_standard => IterRangesOp::parse_args(),
        14 if is_standard => IterAfterOp::parse_args(data),
        15 if is_standard => InclusiveIterOp::parse_args(),
        16 if is_standard => RemoveAllOp::parse_args(data),
        17 => ExtendOp::parse_args(data),
        18 if is_standard => ExtendUnsortedOp::parse_args(data),
        19 if is_standard => UnionOp::parse_args(),
        20 if is_standard => IntersectOp::parse_args(),
        21 if is_standard => IsInvertedOp::parse_args(),
        22 if is_standard && T::can_be_inverted() => InvertOp::parse_args(),
        23 if is_standard => HashOp::parse_args(),
        24 if is_standard => EqualOp::parse_args(),
        25 if is_standard => CmpOp::parse_args(),
        26 if is_standard => IntersectsSetOp::parse_args(),
        27 if is_standard && T::can_be_inverted() => IterExcludedRangesOp::parse_args(),

        _ => None,
    };

    let op = op?;
    Some(NextOperation { op, set_index })
}

pub fn process_op_codes<T: SetMember + 'static>(
    operation_set: OperationSet,
    op_count_limit: u64,
    mut data: Cursor<&[u8]>,
) -> Result<(), Box<dyn Error>> {
    let mut int_set_0 = IntSet::<T>::empty();
    let mut int_set_1 = IntSet::<T>::empty();
    let mut btree_set_0 = BTreeSet::<T>::new();
    let mut btree_set_1 = BTreeSet::<T>::new();

    let mut ops_counter = 0u64;
    loop {
        let next_op = next_operation::<T>(operation_set, &mut data);
        let Some(next_op) = next_op else {
            break;
        };

        {
            let btree_set = if next_op.set_index == 0 {
                &btree_set_0
            } else {
                &btree_set_1
            };
            // when computing size use minimum length of 2 to ensure minimum value of log2(length) is 1.
            ops_counter =
                ops_counter.saturating_add(next_op.op.size(2.max(btree_set.len() as u64)));
            if ops_counter > op_count_limit {
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

    match operation_set {
        OperationSet::Standard => {
            assert!(int_set_0.iter().eq(btree_set_0.iter().copied()));
            assert!(int_set_1.iter().eq(btree_set_1.iter().copied()));
        }
        OperationSet::SparseBitSetEncoding(bias, max_value) => {
            let u32_set: IntSet<u32> = int_set_0.iter().map(|v| v.to_u32()).collect();
            let encoding = u32_set.to_sparse_bit_set();
            let (decoded, remaining) =
                IntSet::<u32>::from_sparse_bit_set_bounded(&encoding, bias, max_value).unwrap();

            let biased_u32_set: IntSet<u32> = int_set_0
                .iter()
                .flat_map(|v| v.to_u32().checked_add(bias))
                .filter(|v| *v <= max_value)
                .collect();
            assert_eq!(remaining.len(), 0);
            assert_eq!(biased_u32_set, decoded);
        }
    }
    Ok(())
}
