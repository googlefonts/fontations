//! A fast, efficient, sparse, & ordered unsigned integer (u32) bit set which is invertible.
//!
//! The bitset is implemented using fixed size pages which allows it to compactly
//! represent sparse membership. However, the set excels when set members are typically
//! clustered together. For example when representing glyph id or unicode codepoint values
//! in a font.
//!
//! The set can have inclusive (the set of integers which are members) or
//! exclusive (the set of integers which are not members) membership. The
//! exclusive/inverted version of the set is useful for patterns such as
//! "keep all codepoints except for {x, y, z, ...}".
//!
//! When constructing a new IntSet from an existing lists of integer values the most efficient
//! way to create the set is to initialize it from a sorted (ascending) iterator of the values.
//!
//! For a type to be stored in the IntSet it must implement the [`Domain`] trait, and all
//! unique values of that type must be able to be mapped to and from a unique `u32` value.
//! See the [`Domain`] trait for more information.

mod bitpage;
mod bitset;

use bitset::BitSet;
use font_types::GlyphId;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::RangeInclusive;

/// A fast & efficient invertible ordered set for small (up to 32-bit) unsigned integer types.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct IntSet<T>(Membership, PhantomData<T>);

/// Defines the domain of `IntSet` member types.
///
/// Members of `IntSet` must implement this trait. Members of `IntSet`'s must meet the following
/// conditions to be used in an `IntSet`:
///
/// 1. Every possible unique value of `T` must be able map to and from a unique `u32`
///    integer.
///
/// 2. The mapped `u32` values must retain the same ordering as the values in `T`.
///
/// 3. `ordered_values`() must iterate over all values in `T` in sorted order (ascending).
///
/// `from_u32`() will only ever be called with u32 values that are part of the domain of T as defined
/// by an implementation of this trait. So it doesn't need to correctly handle values
/// that are outside the domain of `T`.
pub trait Domain<T> {
    /// Converts this value of `T` to a value in u32.
    ///
    /// The mapped value must maintain the same ordering as `T`.
    fn to_u32(&self) -> u32;

    /// Converts a mapped u32 value back to T.
    ///
    /// Will only ever be called with values produced by `to_u32`.
    fn from_u32(member: InDomain) -> T;

    /// Returns true if all u32 values between the mapped u32 min and mapped u32 max value of T are used.
    fn is_continous() -> bool;

    /// Returns an iterator which iterates over all values in the domain of `T`
    ///
    /// Values should be converted to `u32`'s according to the mapping defined in
    /// `to_u32`/`from_u32`.
    fn ordered_values() -> impl DoubleEndedIterator<Item = u32>;

    /// Return an iterator which iterates over all values of T in the given range.
    ///
    /// Values should be converted to `u32`'s according to the mapping defined in
    /// `to_u32`/`from_u32`.
    fn ordered_values_range(range: RangeInclusive<T>) -> impl DoubleEndedIterator<Item = u32>;
}

/// Marks a mapped value as being in the domain of `T` for [`Domain<T>`].
///
/// See [`Domain`] for more information.
pub struct InDomain(u32);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum Membership {
    /// Records a set of integers which are members of the set.
    Inclusive(BitSet),

    /// Records the set of integers which are not members of the set.
    Exclusive(BitSet),
}

impl InDomain {
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl<T: Domain<T>> Default for IntSet<T> {
    fn default() -> IntSet<T> {
        IntSet::empty()
    }
}

impl<T: Domain<T>> IntSet<T> {
    // TODO(garretrieger): add additional functionality that the harfbuzz version has:
    // - Iteration in reverse.
    // - Iteration starting from some value (and before some value for reverse).
    // - Set operations (union, subtract, intersect, sym diff).
    // - Intersects range and intersects iter.
    // - min()/max()

    /// Returns an iterator over all members of the set.
    ///
    /// Note: iteration of inverted sets can be extremely slow due to the very large number of members in the set
    /// care should be taken when using .iter() in combination with an inverted set.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = T> + '_ {
        let u32_iter = match &self.0 {
            Membership::Inclusive(s) => Iter::new(s.iter(), None),
            Membership::Exclusive(s) => Iter::new(s.iter(), Some(T::ordered_values())),
        };
        u32_iter.map(|v| T::from_u32(InDomain(v)))
    }

    /// Adds a value to the set.
    ///
    /// Returns `true` if the value was newly inserted.
    pub fn insert(&mut self, val: T) -> bool {
        let val = val.to_u32();
        match &mut self.0 {
            Membership::Inclusive(s) => s.insert(val),
            Membership::Exclusive(s) => s.remove(val),
        }
    }

    /// Add all values in range as members of this set.
    pub fn insert_range(&mut self, range: RangeInclusive<T>) {
        if T::is_continous() {
            let range = range.start().to_u32()..=range.end().to_u32();
            match &mut self.0 {
                Membership::Inclusive(s) => s.insert_range(range),
                Membership::Exclusive(s) => s.remove_range(range),
            }
        } else {
            let range = T::ordered_values_range(range);
            match &mut self.0 {
                Membership::Inclusive(s) => s.extend(range),
                Membership::Exclusive(s) => s.remove_all(range),
            }
        }
    }

    /// An alternate version of extend() which is optimized for inserting an unsorted iterator of values.
    pub fn extend_unsorted<U: IntoIterator<Item = T>>(&mut self, iter: U) {
        let iter = iter.into_iter().map(|v| v.to_u32());
        match &mut self.0 {
            Membership::Inclusive(s) => s.extend_unsorted(iter),
            Membership::Exclusive(s) => s.remove_all(iter),
        }
    }

    /// Removes a value from the set. Returns whether the value was present in the set.
    pub fn remove(&mut self, val: T) -> bool {
        let val = val.to_u32();
        match &mut self.0 {
            Membership::Inclusive(s) => s.remove(val),
            Membership::Exclusive(s) => s.insert(val),
        }
    }

    // Removes all values in iter from the set.
    pub fn remove_all<U: IntoIterator<Item = T>>(&mut self, iter: U) {
        let iter = iter.into_iter().map(|v| v.to_u32());
        match &mut self.0 {
            Membership::Inclusive(s) => s.remove_all(iter),
            Membership::Exclusive(s) => s.extend(iter),
        }
    }

    /// Removes all values in range as members of this set.
    pub fn remove_range(&mut self, range: RangeInclusive<T>) {
        if T::is_continous() {
            let range = range.start().to_u32()..=range.end().to_u32();
            match &mut self.0 {
                Membership::Inclusive(s) => s.remove_range(range),
                Membership::Exclusive(s) => s.insert_range(range),
            }
        } else {
            let range = T::ordered_values_range(range);
            match &mut self.0 {
                Membership::Inclusive(s) => s.remove_all(range),
                Membership::Exclusive(s) => s.extend(range),
            }
        }
    }

    /// Returns `true` if the set contains a value.
    pub fn contains(&self, val: T) -> bool {
        let val = val.to_u32();
        match &self.0 {
            Membership::Inclusive(s) => s.contains(val),
            Membership::Exclusive(s) => !s.contains(val),
        }
    }
}

impl<T> IntSet<T> {
    /// Create a new empty set (inclusive).
    pub fn empty() -> IntSet<T> {
        IntSet(Membership::Inclusive(BitSet::empty()), PhantomData::<T>)
    }

    /// Create a new set which contains all integers (exclusive).
    pub fn all() -> IntSet<T> {
        IntSet(Membership::Exclusive(BitSet::empty()), PhantomData::<T>)
    }

    /// If this is an inclusive membership set then returns an iterator over the members, otherwise returns None.
    pub fn inclusive_iter(&self) -> Option<impl Iterator<Item = u32> + '_> {
        match &self.0 {
            Membership::Inclusive(s) => Some(s.iter()),
            Membership::Exclusive(_) => None,
        }
    }

    /// Returns true if this set is inverted (has exclusive membership).
    pub fn is_inverted(&self) -> bool {
        match &self.0 {
            Membership::Inclusive(_) => false,
            Membership::Exclusive(_) => true,
        }
    }

    /// Return the inverted version of this set.
    pub fn invert(&mut self) {
        let reuse_storage = match &mut self.0 {
            // take the existing storage to reuse in a new set of the oppposite
            // type.
            Membership::Inclusive(s) | Membership::Exclusive(s) => {
                std::mem::replace(s, BitSet::empty())
            }
        };

        // reuse the storage with a membership of the opposite type.
        self.0 = match &mut self.0 {
            Membership::Inclusive(_) => Membership::Exclusive(reuse_storage),
            Membership::Exclusive(_) => Membership::Inclusive(reuse_storage),
        };
    }

    /// Clears the set, removing all values.
    pub fn clear(&mut self) {
        let mut reuse_storage = match &mut self.0 {
            // if we're inclusive, we just clear the storage
            Membership::Inclusive(s) => {
                s.clear();
                return;
            }
            // otherwise take the existing storage to reuse in a new
            // inclusive set:
            Membership::Exclusive(s) => std::mem::replace(s, BitSet::empty()),
        };
        // reuse the now empty storage and mark us as inclusive
        reuse_storage.clear();
        self.0 = Membership::Inclusive(reuse_storage);
    }

    /// Returns the number of members in this set.
    pub fn len(&self) -> usize {
        match &self.0 {
            Membership::Inclusive(s) => s.len(),
            Membership::Exclusive(s) => u32::MAX as usize - s.len(),
        }
    }

    /// Return true if there are no members in this set.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T: Domain<T>> FromIterator<T> for IntSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut s = IntSet::empty();
        s.extend(iter);
        s
    }
}

impl<T: Domain<T>> Extend<T> for IntSet<T> {
    /// Extends a collection with the contents of an iterator.
    ///
    /// This implementation is optimized to provide the best performance when the iterator contains sorted values.
    /// Consider using extend_unsorted() if the iterator is known to contain unsorted values.
    fn extend<U: IntoIterator<Item = T>>(&mut self, iter: U) {
        let iter = iter.into_iter().map(|v| v.to_u32());
        match &mut self.0 {
            Membership::Inclusive(s) => s.extend(iter),
            Membership::Exclusive(s) => s.remove_all(iter),
        }
    }
}

struct Iter<SetIter, AllValuesIter>
where
    SetIter: DoubleEndedIterator<Item = u32>,
    AllValuesIter: DoubleEndedIterator<Item = u32>,
{
    set_values: SetIter,
    all_values: Option<AllValuesIter>,
    next_skipped_forward: Option<u32>,
    next_skipped_backward: Option<u32>,
}

impl<SetIter, AllValuesIter> Iter<SetIter, AllValuesIter>
where
    SetIter: DoubleEndedIterator<Item = u32>,
    AllValuesIter: DoubleEndedIterator<Item = u32>,
{
    fn new(
        mut set_values: SetIter,
        all_values: Option<AllValuesIter>,
    ) -> Iter<SetIter, AllValuesIter> {
        match all_values {
            Some(_) => Iter {
                next_skipped_forward: set_values.next(),
                next_skipped_backward: set_values.next_back(),
                set_values,
                all_values,
            },
            None => Iter {
                set_values,
                all_values,
                next_skipped_forward: None,
                next_skipped_backward: None,
            },
        }
    }
}

impl<SetIter, AllValuesIter> Iterator for Iter<SetIter, AllValuesIter>
where
    SetIter: DoubleEndedIterator<Item = u32>,
    AllValuesIter: DoubleEndedIterator<Item = u32>,
{
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        let Some(all_values_it) = &mut self.all_values else {
            return self.set_values.next();
        };

        for index in all_values_it.by_ref() {
            let index = index.to_u32();
            loop {
                let Some(skip) = self.next_skipped_forward else {
                    // There are no skips left in the iterator, but there may still be a skipped
                    // number on the backwards iteration, so check that.
                    if let Some(skip) = self.next_skipped_backward {
                        if skip == index {
                            // this index should be skipped, go to the next one.
                            break;
                        }
                    }
                    // No-longer any values to skip, can freely return index
                    return Some(index);
                };

                if index < skip {
                    // Not a skipped value
                    return Some(index);
                }

                self.next_skipped_forward = self.set_values.next();
                if index > skip {
                    // We've passed the skip value, need to check the next one.
                    continue;
                }

                // index == skip, so we need to skip this index.
                break;
            }
        }
        None
    }
}

impl<SetIter, AllValuesIter> DoubleEndedIterator for Iter<SetIter, AllValuesIter>
where
    SetIter: DoubleEndedIterator<Item = u32>,
    AllValuesIter: DoubleEndedIterator<Item = u32>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let Some(all_values_it) = &mut self.all_values else {
            return self.set_values.next_back();
        };

        for index in all_values_it.by_ref().rev() {
            let index = index.to_u32();
            loop {
                let Some(skip) = self.next_skipped_backward else {
                    // There are no skips left in the iterator, but there may still be a skipped
                    // number on the backwards iteration, so check that.
                    if let Some(skip) = self.next_skipped_forward {
                        if skip == index {
                            // this index should be skipped, go to the next one.
                            break;
                        }
                    }
                    // No-longer any values to skip, can freely return index
                    return Some(index);
                };

                if index > skip {
                    // Not a skipped value
                    return Some(index);
                }

                self.next_skipped_backward = self.set_values.next_back();
                if index < skip {
                    // We've passed the skip value, need to check the next one.
                    continue;
                }

                // index == skip, so we need to skip this index.
                break;
            }
        }
        None
    }
}

impl Domain<u32> for u32 {
    fn to_u32(&self) -> u32 {
        *self
    }

    fn from_u32(member: InDomain) -> u32 {
        member.value()
    }

    fn is_continous() -> bool {
        true
    }

    fn ordered_values() -> impl DoubleEndedIterator<Item = u32> {
        u32::MIN..=u32::MAX
    }

    fn ordered_values_range(range: RangeInclusive<u32>) -> impl DoubleEndedIterator<Item = u32> {
        range
    }
}

impl Domain<u16> for u16 {
    fn to_u32(&self) -> u32 {
        *self as u32
    }

    fn from_u32(member: InDomain) -> u16 {
        member.value() as u16
    }

    fn is_continous() -> bool {
        true
    }

    fn ordered_values() -> impl DoubleEndedIterator<Item = u32> {
        (u16::MIN as u32)..=(u16::MAX as u32)
    }

    fn ordered_values_range(range: RangeInclusive<u16>) -> impl DoubleEndedIterator<Item = u32> {
        (*range.start() as u32)..=(*range.end() as u32)
    }
}

impl Domain<u8> for u8 {
    fn to_u32(&self) -> u32 {
        *self as u32
    }

    fn from_u32(member: InDomain) -> u8 {
        member.value() as u8
    }

    fn is_continous() -> bool {
        true
    }

    fn ordered_values() -> impl DoubleEndedIterator<Item = u32> {
        (u8::MIN as u32)..=(u8::MAX as u32)
    }

    fn ordered_values_range(range: RangeInclusive<u8>) -> impl DoubleEndedIterator<Item = u32> {
        (*range.start() as u32)..=(*range.end() as u32)
    }
}

impl Domain<GlyphId> for GlyphId {
    fn to_u32(&self) -> u32 {
        self.to_u16() as u32
    }

    fn from_u32(member: InDomain) -> GlyphId {
        GlyphId::from(member.value() as u16)
    }

    fn is_continous() -> bool {
        true
    }

    fn ordered_values() -> impl DoubleEndedIterator<Item = u32> {
        (u16::MIN as u32)..=(u16::MAX as u32)
    }

    fn ordered_values_range(
        range: RangeInclusive<GlyphId>,
    ) -> impl DoubleEndedIterator<Item = u32> {
        range.start().to_u32()..=range.end().to_u32()
    }
}

#[cfg(test)]
mod test {
    use std::{
        collections::HashSet,
        hash::{DefaultHasher, Hash, Hasher},
    };

    use super::*;

    #[derive(PartialEq, Eq, Debug, PartialOrd, Ord)]
    struct EvenInts(u16);

    impl Domain<EvenInts> for EvenInts {
        fn to_u32(&self) -> u32 {
            self.0 as u32
        }

        fn from_u32(member: InDomain) -> EvenInts {
            EvenInts(member.0 as u16)
        }

        fn is_continous() -> bool {
            false
        }

        fn ordered_values() -> impl DoubleEndedIterator<Item = u32> {
            (u16::MIN..=u16::MAX)
                .filter(|v| v % 2 == 0)
                .map(|v| v as u32)
        }

        fn ordered_values_range(
            range: RangeInclusive<EvenInts>,
        ) -> impl DoubleEndedIterator<Item = u32> {
            Self::ordered_values()
                .filter(move |v| *v >= range.start().to_u32() && *v <= range.end().to_u32())
        }
    }

    #[test]
    fn insert() {
        let mut empty = IntSet::<u32>::empty();
        let mut all = IntSet::<u32>::all();

        assert!(!empty.contains(10));
        assert!(empty.insert(10));
        assert!(empty.contains(10));
        assert!(!empty.insert(10));

        assert!(all.contains(10));
        assert!(!all.insert(10));
        assert!(all.contains(10));
        assert!(!all.insert(10));
    }

    #[test]
    fn remove() {
        let mut empty = IntSet::<u32>::empty();
        empty.insert(10);
        let mut all = IntSet::<u32>::all();

        assert!(empty.contains(10));
        assert!(empty.remove(10));
        assert!(!empty.contains(10));
        assert!(!empty.remove(10));

        assert!(all.contains(10));
        assert!(all.remove(10));
        assert!(!all.contains(10));
        assert!(!all.remove(10));
    }

    #[test]
    fn is_empty() {
        let mut set = IntSet::<u32>::empty();

        assert!(set.is_empty());
        set.insert(13);
        set.insert(800);
        assert!(!set.is_empty());

        set.invert();
        assert!(!set.is_empty());

        let mut empty = IntSet::<u32>::empty();
        assert!(empty.is_empty());
        empty.invert();
        assert!(!empty.is_empty());
    }

    #[test]
    fn clear() {
        let mut set = IntSet::<u32>::empty();
        set.insert(13);
        set.insert(800);

        let mut set_inverted = IntSet::<u32>::empty();
        set_inverted.insert(13);
        set_inverted.insert(800);
        set_inverted.invert();

        set.clear();
        assert!(set.is_empty());
        set_inverted.clear();
        assert!(set_inverted.is_empty());
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn equal_an_hash() {
        let mut inc1 = IntSet::<u32>::empty();
        inc1.insert(14);
        inc1.insert(670);

        let mut inc2 = IntSet::<u32>::empty();
        inc2.insert(670);
        inc2.insert(14);

        let mut inc3 = inc1.clone();
        inc3.insert(5);

        let mut exc = inc1.clone();
        exc.invert();

        assert_eq!(inc1, inc2);
        assert_ne!(inc1, inc3);
        assert_ne!(inc1, exc);

        let set = HashSet::from([inc1.clone(), inc3.clone(), exc.clone()]);
        assert!(set.contains(&inc1));
        assert!(set.contains(&inc3));
        assert!(set.contains(&exc));

        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        let mut h3 = DefaultHasher::new();
        inc1.hash(&mut h1);
        exc.hash(&mut h2);
        inc2.hash(&mut h3);

        assert_ne!(h1.finish(), h2.finish());
        assert_eq!(h1.finish(), h3.finish());
    }

    #[test]
    fn iter() {
        let mut set = IntSet::<u32>::empty();
        set.insert(3);
        set.insert(8);
        set.insert(534);
        set.insert(700);
        set.insert(10000);
        set.insert(10001);
        set.insert(10002);

        let v: Vec<u32> = set.iter().collect();
        assert_eq!(v, vec![3, 8, 534, 700, 10000, 10001, 10002]);

        let v: Vec<u32> = set.inclusive_iter().unwrap().collect();
        assert_eq!(v, vec![3, 8, 534, 700, 10000, 10001, 10002]);
    }

    #[test]
    fn iter_backwards() {
        let mut set = IntSet::<u32>::empty();
        set.insert_range(1..=6);
        {
            let mut it = set.iter();
            assert_eq!(Some(1), it.next());
            assert_eq!(Some(6), it.next_back());
            assert_eq!(Some(5), it.next_back());
            assert_eq!(Some(2), it.next());
            assert_eq!(Some(3), it.next());
            assert_eq!(Some(4), it.next());
            assert_eq!(None, it.next());
            assert_eq!(None, it.next_back());
        }

        let mut set = IntSet::<u8>::empty();
        set.invert();
        set.remove_range(10..=255);
        set.remove(4);
        set.remove(8);
        {
            let mut it = set.iter();
            assert_eq!(Some(0), it.next());
            assert_eq!(Some(1), it.next());
            assert_eq!(Some(2), it.next());
            assert_eq!(Some(3), it.next());

            assert_eq!(Some(9), it.next_back());
            assert_eq!(Some(7), it.next_back());
            assert_eq!(Some(6), it.next_back());
            assert_eq!(Some(5), it.next_back());
            assert_eq!(None, it.next_back());

            assert_eq!(None, it.next());
        }

        let mut set = IntSet::<u8>::empty();
        set.invert();
        set.remove_range(10..=255);
        set.remove(4);
        set.remove(8);
        {
            let mut it = set.iter();
            assert_eq!(Some(0), it.next());
            assert_eq!(Some(1), it.next());
            assert_eq!(Some(2), it.next());
            assert_eq!(Some(3), it.next());
            assert_eq!(Some(5), it.next());

            assert_eq!(Some(9), it.next_back());
            assert_eq!(Some(7), it.next_back());
            assert_eq!(Some(6), it.next_back());
            assert_eq!(None, it.next_back());

            assert_eq!(None, it.next());
        }
    }

    #[test]
    fn exclusive_iter() {
        let mut set = IntSet::<u32>::all();
        set.remove(3);
        set.remove(7);
        set.remove(8);

        let mut iter = set.iter();

        assert_eq!(iter.next(), Some(0));
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(5));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), Some(9));
        assert_eq!(iter.next(), Some(10));

        assert!(set.inclusive_iter().is_none());

        // Forward skip first
        let mut set = IntSet::<u32>::all();
        set.remove_range(0..=200);

        let mut iter = set.iter();
        assert_eq!(iter.next(), Some(201));

        // Backward skip first
        let mut set = IntSet::<u8>::all();
        set.remove_range(200..=255);

        let mut iter = set.iter();
        assert_eq!(iter.next_back(), Some(199));
    }

    #[test]
    fn from_iterator() {
        let s: IntSet<u32> = [3, 8, 12, 589].iter().copied().collect();
        let mut expected = IntSet::<u32>::empty();
        expected.insert(3);
        expected.insert(8);
        expected.insert(12);
        expected.insert(589);

        assert_eq!(s, expected);
    }

    #[test]
    fn from_int_set_iterator() {
        let s1: IntSet<u32> = [3, 8, 12, 589].iter().copied().collect();
        let s2: IntSet<u32> = s1.iter().collect();
        assert_eq!(s1, s2);
    }

    #[test]
    fn extend() {
        let mut s = IntSet::<u32>::empty();
        s.extend([3, 12].iter().copied());
        s.extend([8, 10, 589].iter().copied());

        let mut expected = IntSet::<u32>::empty();
        expected.insert(3);
        expected.insert(8);
        expected.insert(10);
        expected.insert(12);
        expected.insert(589);

        assert_eq!(s, expected);
    }

    #[test]
    fn extend_on_inverted() {
        let mut s = IntSet::<u32>::all();
        for i in 10..=20 {
            s.remove(i);
        }

        s.extend([12, 17, 18].iter().copied());

        assert!(!s.contains(11));
        assert!(s.contains(12));
        assert!(!s.contains(13));

        assert!(!s.contains(16));
        assert!(s.contains(17));
        assert!(s.contains(18));
        assert!(!s.contains(19));
        assert!(s.contains(100));
    }

    #[test]
    fn remove_all() {
        let mut empty = IntSet::<u32>::empty();
        let mut all = IntSet::<u32>::all();

        empty.extend([1, 2, 3, 4]);

        empty.remove_all([2, 3]);
        all.remove_all([2, 3]);

        assert!(empty.contains(1));
        assert!(!empty.contains(2));
        assert!(!empty.contains(3));
        assert!(empty.contains(4));

        assert!(all.contains(1));
        assert!(!all.contains(2));
        assert!(!all.contains(3));
        assert!(all.contains(4));
    }

    #[test]
    fn remove_range() {
        let mut empty = IntSet::<u32>::empty();
        let mut all = IntSet::<u32>::all();

        empty.extend([1, 2, 3, 4]);

        empty.remove_range(2..=3);
        all.remove_range(2..=3);

        assert!(empty.contains(1));
        assert!(!empty.contains(2));
        assert!(!empty.contains(3));
        assert!(empty.contains(4));

        assert!(all.contains(1));
        assert!(!all.contains(2));
        assert!(!all.contains(3));
        assert!(all.contains(4));
    }

    #[test]
    fn inverted() {
        let mut set = IntSet::<u32>::empty();

        set.insert(13);
        set.insert(800);
        assert!(set.contains(13));
        assert!(set.contains(800));
        assert_eq!(set.len(), 2);
        assert!(!set.is_inverted());

        set.invert();
        assert_eq!(set.len(), u32::MAX as usize - 2);
        assert!(!set.contains(13));
        assert!(set.contains(80));
        assert!(!set.contains(800));
        assert!(set.is_inverted());

        set.remove(80);
        assert!(!set.contains(80));

        set.insert(13);
        assert!(set.contains(13));

        set.invert();
        assert!(set.contains(80));
        assert!(set.contains(800));
    }

    #[test]
    fn limited_domain_type() {
        let mut set = IntSet::<EvenInts>::empty();

        set.insert(EvenInts(2));
        set.insert(EvenInts(8));
        set.insert(EvenInts(12));
        set.insert_range(EvenInts(20)..=EvenInts(34));
        set.remove_range(EvenInts(30)..=EvenInts(34));

        assert!(set.contains(EvenInts(2)));
        assert!(!set.contains(EvenInts(4)));

        assert!(!set.contains(EvenInts(18)));
        assert!(!set.contains(EvenInts(19)));
        assert!(set.contains(EvenInts(20)));
        assert!(!set.contains(EvenInts(21)));
        assert!(set.contains(EvenInts(28)));
        assert!(!set.contains(EvenInts(29)));
        assert!(!set.contains(EvenInts(30)));

        let copy: IntSet<EvenInts> = set.iter().collect();
        assert_eq!(set, copy);

        set.invert();

        assert!(!set.contains(EvenInts(2)));
        assert!(set.contains(EvenInts(4)));

        let Some(max) = set.iter().max() else {
            panic!("should have a max");
        };

        assert_eq!(max.0, u16::MAX - 1);

        {
            let mut it = set.iter();
            assert_eq!(it.next(), Some(EvenInts(0)));
            assert_eq!(it.next(), Some(EvenInts(4)));
            assert_eq!(it.next(), Some(EvenInts(6)));
            assert_eq!(it.next(), Some(EvenInts(10)));
            assert_eq!(it.next(), Some(EvenInts(14)));
        }

        set.insert_range(EvenInts(6)..=EvenInts(10));
        {
            let mut it = set.iter();
            assert_eq!(it.next(), Some(EvenInts(0)));
            assert_eq!(it.next(), Some(EvenInts(4)));
            assert_eq!(it.next(), Some(EvenInts(6)));
            assert_eq!(it.next(), Some(EvenInts(8)));
            assert_eq!(it.next(), Some(EvenInts(10)));
            assert_eq!(it.next(), Some(EvenInts(14)));
        }

        set.remove_range(EvenInts(6)..=EvenInts(10));
        {
            let mut it = set.iter();
            assert_eq!(it.next(), Some(EvenInts(0)));
            assert_eq!(it.next(), Some(EvenInts(4)));
            assert_eq!(it.next(), Some(EvenInts(14)));
        }
    }

    #[test]
    fn with_u16() {
        let mut set = IntSet::<u16>::empty();

        set.insert(5);
        set.insert(8);
        set.insert(12);
        set.insert_range(200..=210);

        assert!(set.contains(5));
        assert!(!set.contains(6));
        assert!(!set.contains(199));
        assert!(set.contains(200));
        assert!(set.contains(210));
        assert!(!set.contains(211));

        let copy: IntSet<u16> = set.iter().collect();
        assert_eq!(set, copy);

        set.invert();

        assert!(!set.contains(5));
        assert!(set.contains(6));

        let Some(max) = set.iter().max() else {
            panic!("should have a max");
        };

        assert_eq!(max, u16::MAX);

        let mut it = set.iter();
        assert_eq!(it.next(), Some(0));
        assert_eq!(it.next(), Some(1));
        assert_eq!(it.next(), Some(2));
        assert_eq!(it.next(), Some(3));
        assert_eq!(it.next(), Some(4));
        assert_eq!(it.next(), Some(6));
    }

    #[test]
    fn with_glyph_id() {
        let mut set = IntSet::<font_types::GlyphId>::empty();

        set.insert(GlyphId::new(5));
        set.insert(GlyphId::new(8));
        set.insert(GlyphId::new(12));
        set.insert_range(GlyphId::new(200)..=GlyphId::new(210));

        assert!(set.contains(GlyphId::new(5)));
        assert!(!set.contains(GlyphId::new(6)));
        assert!(!set.contains(GlyphId::new(199)));
        assert!(set.contains(GlyphId::new(200)));
        assert!(set.contains(GlyphId::new(210)));
        assert!(!set.contains(GlyphId::new(211)));

        let copy: IntSet<GlyphId> = set.iter().collect();
        assert_eq!(set, copy);

        set.invert();

        assert!(!set.contains(GlyphId::new(5)));
        assert!(set.contains(GlyphId::new(6)));

        let Some(max) = set.iter().max() else {
            panic!("should have a max");
        };

        assert_eq!(max, GlyphId::new(u16::MAX));

        let mut it = set.iter();
        assert_eq!(it.next(), Some(GlyphId::new(0)));
        assert_eq!(it.next(), Some(GlyphId::new(1)));
        assert_eq!(it.next(), Some(GlyphId::new(2)));
        assert_eq!(it.next(), Some(GlyphId::new(3)));
        assert_eq!(it.next(), Some(GlyphId::new(4)));
        assert_eq!(it.next(), Some(GlyphId::new(6)));
    }
}
