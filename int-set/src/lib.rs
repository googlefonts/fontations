mod bitpage;
mod bitset;

use bitset::BitSet;
use std::ops::RangeInclusive;

/// A fast & efficient unsigned integer (u32) bit set which is invertible.
#[derive(Clone, Debug)]
pub struct IntSet<T>(Membership<T>);

#[derive(Clone, Debug)]
enum Membership<T> {
    /// Records a set of integers which are members of the set.
    Inclusive(BitSet<T>),

    /// Records the set of integers which are not members of the set.
    Exclusive(BitSet<T>),
}

impl<T: Into<u32> + Copy> Default for IntSet<T> {
    fn default() -> IntSet<T> {
        IntSet::<T>::empty()
    }
}

impl<T: Into<u32> + Copy> IntSet<T> {
    /// Create a new empty set.

    /// Create a new empty set.
    pub fn empty() -> IntSet<T> {
        IntSet::<T>(Membership::Inclusive(BitSet::<T>::empty()))
    }

    /// Create a new set which contains all integers.
    pub fn all() -> IntSet<T> {
        IntSet::<T>(Membership::Exclusive(BitSet::<T>::empty()))
    }

    /// Return the inverted version of this set.
    pub fn inverted(self) -> IntSet<T> {
        match self.0 {
            Membership::<T>::Inclusive(s) => IntSet::<T>(Membership::<T>::Exclusive(s)),
            Membership::<T>::Exclusive(s) => IntSet::<T>(Membership::<T>::Inclusive(s)),
        }
    }

    /// Return a new version of this set with all members removed.
    pub fn clear(mut self) -> IntSet<T> {
        self.clear_internal_set();
        match self.0 {
            Membership::<T>::Inclusive(s) => IntSet::<T>(Membership::<T>::Inclusive(s)),
            Membership::<T>::Exclusive(s) => IntSet::<T>(Membership::<T>::Inclusive(s)),
        }
    }

    /// Add val as a member of this set.
    pub fn insert(&mut self, val: T) -> bool {
        match &mut self.0 {
            Membership::<T>::Inclusive(s) => s.insert(val),
            Membership::<T>::Exclusive(s) => s.remove(val),
        }
    }

    /// Add all values in range as members of this set.
    pub fn insert_range(&mut self, range: RangeInclusive<T>) {
        match &mut self.0 {
            Membership::<T>::Inclusive(s) => s.insert_range(range),
            Membership::<T>::Exclusive(_) => todo!("implement bitset::remove_range and call here."),
        }
    }

    /// Remove val from this set.
    pub fn remove(&mut self, val: T) -> bool {
        match &mut self.0 {
            Membership::<T>::Inclusive(s) => s.remove(val),
            Membership::<T>::Exclusive(s) => s.insert(val),
        }
    }

    /// Returns true if val is a member of this set.
    pub fn contains(&self, val: T) -> bool {
        match &self.0 {
            Membership::<T>::Inclusive(s) => s.contains(val),
            Membership::<T>::Exclusive(s) => !s.contains(val),
        }
    }

    fn clear_internal_set(&mut self) {
        match &mut self.0 {
            Membership::<T>::Inclusive(s) => s.clear(),
            Membership::<T>::Exclusive(s) => s.clear(),
        }
    }
}

impl<T> IntSet<T> {
    /// Returns the number of members in this set.
    pub fn len(&self) -> usize {
        match &self.0 {
            Membership::<T>::Inclusive(s) => s.len(),
            Membership::<T>::Exclusive(s) => u32::MAX as usize - s.len(),
        }
    }

    /// Return true if there are no members in this set.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> std::hash::Hash for IntSet<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match &self.0 {
            Membership::<T>::Inclusive(s) => {
                s.hash(state);
                0u32.hash(state);
            }
            Membership::<T>::Exclusive(s) => {
                s.hash(state);
                1u32.hash(state);
            }
        }
    }
}

impl<T> std::cmp::Eq for IntSet<T> {}

impl<T> std::cmp::PartialEq for IntSet<T> {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Membership::<T>::Inclusive(a), Membership::<T>::Inclusive(b)) => a == b,
            (Membership::<T>::Exclusive(a), Membership::<T>::Exclusive(b)) => a == b,
            _ => return false,
        }
    }
}

impl<T: Into<u32> + Copy> FromIterator<T> for IntSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        // TODO(garretrieger): implement a more efficient version of this which avoids page lookups
        //  when the iterator values are in sorted order (eg. if the next value is on the same page as
        //  the previous value). This will require BitSet to also implement FromIterator.
        let mut s = IntSet::<T>::empty();
        for i in iter {
            s.insert(i);
        }
        s
    }
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

        let set = set.inverted();
        assert!(!set.is_empty());

        let empty = IntSet::<u32>::empty();
        assert!(empty.is_empty());
        let all = empty.inverted();
        assert!(!all.is_empty());
    }

    #[test]
    fn clear() {
        let mut set = IntSet::<u32>::empty();
        set.insert(13);
        set.insert(800);

        let mut set_inverted = IntSet::<u32>::empty();
        set_inverted.insert(13);
        set_inverted.insert(800);
        let set_inverted = set_inverted.inverted();

        let set = set.clear();
        assert!(set.is_empty());
        let set_inverted = set_inverted.clear();
        assert!(set_inverted.is_empty());
    }

    #[test]
    fn equal_an_hash() {
        let mut inc1 = IntSet::<u32>::empty();
        inc1.insert(14);
        inc1.insert(670);

        let mut inc2 = IntSet::<u32>::empty();
        inc2.insert(670);
        inc2.insert(14);

        let mut inc3 = inc1.clone();
        inc3.insert(5);

        let exc = inc1.clone().inverted();

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
    fn from_iterator() {
        let s: IntSet<u32> = vec![3, 8, 12, 589].iter().copied().collect();
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

        set = set.inverted();
        assert_eq!(set.len(), u32::MAX as usize - 2);
        assert!(!set.contains(13));
        assert!(set.contains(80));
        assert!(!set.contains(800));

        set.remove(80);
        assert!(!set.contains(80));

        set.insert(13);
        assert!(set.contains(13));

        set = set.inverted();
        assert!(set.contains(80));
        assert!(set.contains(800));
    }
}
