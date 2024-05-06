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

mod bitpage;
mod bitset;

use bitset::BitSet;
use std::hash::Hash;
use std::ops::RangeInclusive;

/// A fast & efficient invertible ordered set for small (up to 32-bit) unsigned integer types.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct IntSet<T>(Membership<T>);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum Membership<T> {
    /// Records a set of integers which are members of the set.
    Inclusive(BitSet<T>),

    /// Records the set of integers which are not members of the set.
    Exclusive(BitSet<T>),
}

impl<T: Into<u32> + Copy> Default for IntSet<T> {
    fn default() -> IntSet<T> {
        IntSet::empty()
    }
}

impl<T: Into<u32> + Copy> IntSet<T> {
    /// Adds a value to the set.
    ///
    /// Returns `true` if the value was newly inserted.
    pub fn insert(&mut self, val: T) -> bool {
        match &mut self.0 {
            Membership::Inclusive(s) => s.insert(val),
            Membership::Exclusive(s) => s.remove(val),
        }
    }

    /// Add all values in range as members of this set.
    pub fn insert_range(&mut self, range: RangeInclusive<T>) {
        match &mut self.0 {
            Membership::Inclusive(s) => s.insert_range(range),
            Membership::Exclusive(_) => todo!("implement bitset::remove_range and call here."),
        }
    }

    /// Removes a value from the set. Returns whether the value was present in the set.
    pub fn remove(&mut self, val: T) -> bool {
        match &mut self.0 {
            Membership::Inclusive(s) => s.remove(val),
            Membership::Exclusive(s) => s.insert(val),
        }
    }

    /// Returns `true` if the set contains a value.
    pub fn contains(&self, val: T) -> bool {
        match &self.0 {
            Membership::Inclusive(s) => s.contains(val),
            Membership::Exclusive(s) => !s.contains(val),
        }
    }
}

impl<T> IntSet<T> {
    /// Create a new empty set (inclusive).
    pub fn empty() -> IntSet<T> {
        IntSet(Membership::Inclusive(BitSet::empty()))
    }

    /// Create a new set which contains all integers (exclusive).
    pub fn all() -> IntSet<T> {
        IntSet(Membership::Exclusive(BitSet::empty()))
    }

    /// Returns an iterator over all members of the set.
    /// Note: iteration of inverted sets can be extremely slow due to the very large number of members in the set
    /// care should be taken when using .iter() in combination with an inverted set.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        match &self.0 {
            Membership::Inclusive(s) => int_set_iter(s.iter(), false),
            Membership::Exclusive(s) => int_set_iter(s.iter(), true),
        }
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

impl<T: Into<u32> + Copy> FromIterator<T> for IntSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut s = IntSet::empty();
        s.extend(iter);
        s
    }
}

impl<T: Into<u32> + Copy> Extend<T> for IntSet<T> {
    fn extend<U: IntoIterator<Item = T>>(&mut self, iter: U) {
        // TODO(garretrieger): implement a more efficient version of this which avoids page lookups
        //  when the iterator values are in sorted order (eg. if the next value is on the same page as
        //  the previous value). This will require BitSet to also implement FromIterator.
        for elem in iter {
            self.insert(elem);
        }
    }
}

fn int_set_iter<U>(mut values: U, exclusive: bool) -> impl Iterator<Item = u32>
where
    U: Iterator<Item = u32>,
{
    let mut cur_value = values.next();
    let mut index: u64 = u32::MIN as u64;

    std::iter::from_fn(move || {
        if !exclusive {
            let ret = cur_value;
            cur_value = values.next();
            return ret;
        }

        if index > u32::MAX as u64 {
            return None;
        }

        while let Some(skip) = cur_value {
            if index < skip as u64 {
                break;
            }

            cur_value = values.next();
            if index > skip as u64 {
                continue;
            }

            index += 1;
            continue;
        }

        let next_index = index;
        index = next_index + 1;
        Some(next_index as u32)
    })
}

#[cfg(test)]
mod test {
    use std::{
        collections::HashSet,
        hash::{DefaultHasher, Hash, Hasher},
    };

    use super::*;

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
    fn extend() {
        let mut s = IntSet::<u32>::empty();
        s.extend([3, 12].iter().copied());
        s.extend([8, 589].iter().copied());

        let mut expected = IntSet::<u32>::empty();
        expected.insert(3);
        expected.insert(8);
        expected.insert(12);
        expected.insert(589);

        assert_eq!(s, expected);
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
}
