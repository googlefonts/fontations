//! helpers related to collection types

use std::collections::BTreeSet;

/// A helper trait for array-like fields, where we need to know
/// the length in order to populate another field.
pub trait HasLen {
    fn len(&self) -> usize;

    fn checked_len<T: TryFrom<usize>>(&self) -> Result<T, <T as TryFrom<usize>>::Error> {
        self.len().try_into()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> HasLen for [T] {
    fn len(&self) -> usize {
        self.len()
    }
}

impl<T> HasLen for BTreeSet<T> {
    fn len(&self) -> usize {
        self.len()
    }
}

impl<T> HasLen for Vec<T> {
    fn len(&self) -> usize {
        self.len()
    }
}

impl<T: HasLen> HasLen for Option<T> {
    fn len(&self) -> usize {
        match &self {
            Some(t) => t.len(),
            None => 0,
        }
    }
}
