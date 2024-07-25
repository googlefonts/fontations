//! Various helpers for dealing with iteration of a slice
//! that represents a loop. Specifically, outline contours.

#[derive(Copy, Clone)]
pub(super) struct IndexCycler {
    last: usize,
}

impl IndexCycler {
    pub fn new(len: usize) -> Self {
        Self { last: len - 1 }
    }

    pub fn next(self, ix: usize) -> usize {
        if ix >= self.last {
            0
        } else {
            ix + 1
        }
    }

    pub fn prev(self, ix: usize) -> usize {
        if ix == 0 {
            self.last
        } else {
            ix - 1
        }
    }
}

/// Iterator that begins at `start + 1` and cycles through all items
/// of the slice in forward order, ending with `start`.
pub(super) fn cycle_forward<T>(items: &[T], start: usize) -> impl Iterator<Item = (usize, &T)> {
    let len = items.len();
    let start = start + 1;
    (0..len).map(move |ix| {
        let real_ix = (ix + start) % len;
        (real_ix, &items[real_ix])
    })
}

/// Iterator that begins at `start - 1` and cycles through all items
/// of the slice in reverse order, ending with `start`.
pub(super) fn cycle_backward<T>(items: &[T], start: usize) -> impl Iterator<Item = (usize, &T)> {
    let len = items.len();
    (0..len).rev().map(move |ix| {
        let real_ix = (ix + start) % len;
        (real_ix, &items[real_ix])
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn cycle_iter_forward() {
        let items = [0, 1, 2, 3, 4, 5, 6, 7];
        let from_5 = super::cycle_forward(&items, 5)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_5, &[6, 7, 0, 1, 2, 3, 4, 5]);
        let from_last = super::cycle_forward(&items, 7)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_last, &items);
    }

    #[test]
    fn cycle_iter_backward() {
        let items = [0, 1, 2, 3, 4, 5, 6, 7];
        let from_5 = super::cycle_backward(&items, 5)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_5, &[4, 3, 2, 1, 0, 7, 6, 5]);
        let from_0 = super::cycle_backward(&items, 0)
            .map(|(_, val)| *val)
            .collect::<Vec<_>>();
        assert_eq!(from_0, &[7, 6, 5, 4, 3, 2, 1, 0]);
    }
}
