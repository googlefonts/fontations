mod bitpage;
mod bitset;

use bitset::BitSet;

#[derive(Clone, Debug)]
pub enum IntSet<T> {
    Standard(BitSet<T>),
    Inverted(BitSet<T>),
}

impl<T: Into<u32> + Copy + Default> IntSet<T> {
    pub fn default() -> IntSet<T> {
        IntSet::Standard(BitSet::<T>::default())
    }

    pub fn inverted(self) -> IntSet<T> {
        match self {
            IntSet::<T>::Standard(s) => IntSet::<T>::Inverted(s),
            IntSet::<T>::Inverted(s) => IntSet::<T>::Standard(s),
        }
    }

    pub fn insert(&mut self, val: T) -> bool {
        match self {
            IntSet::<T>::Standard(s) => s.insert(val),
            IntSet::<T>::Inverted(s) => s.remove(val),
        }
    }

    // TODO(garretrieger): implement
    // pub fn insert_range(&mut self, range: RangeInclusive<T>) {
    //    todo!()
    //}

    pub fn remove(&mut self, val: T) -> bool {
        match self {
            IntSet::<T>::Standard(s) => s.remove(val),
            IntSet::<T>::Inverted(s) => s.insert(val),
        }
    }

    // TODO(garretrieger): implement
    // pub fn remove_range(&mut self, range: RangeInclusive<T>) {
    //    todo!()
    // }

    pub fn contains(&self, val: T) -> bool {
        match self {
            IntSet::<T>::Standard(s) => s.contains(val),
            IntSet::<T>::Inverted(s) => !s.contains(val),
        }
    }
}

impl<T> IntSet<T> {
    pub fn len(&self) -> usize {
        match self {
            IntSet::<T>::Standard(s) => s.len(),
            IntSet::<T>::Inverted(s) => u32::MAX as usize - s.len(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn inverted() {
        let mut set = IntSet::<u32>::default();

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
