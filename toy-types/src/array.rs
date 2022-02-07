use crate::{Blob, ExactSized, FontRead};

#[derive(Clone)]
pub struct Array<'a, T> {
    data: Blob<'a>,
    _t: std::marker::PhantomData<T>,
}

impl<'a, T: ExactSized + FontRead<'a>> Array<'a, T> {
    //NOTE: this data is relative to start of parent, and we pass in offset.
    // I think this was so that we only needed to index once? and we could
    // have all our logic in here to ensure we checked bounds so we could unsafe later?
    pub fn new(data: Blob<'a>, offset: usize, len: usize) -> Option<Self> {
        let byte_len = len * T::SIZE;
        let data = data.get(offset..offset + byte_len)?;
        Some(Self {
            data,
            _t: std::marker::PhantomData,
        })
    }

    /// A new array taking all of the data `offset..`. Used in cmap glyph id arrays
    pub fn new_no_len(data: Blob<'a>, offset: usize) -> Option<Self> {
        data.get(offset..data.len()).map(|data| Self {
            data,
            _t: std::marker::PhantomData,
        })
    }

    /// The number of *items* in the array
    pub fn len(&self) -> usize {
        self.data_len() / T::SIZE
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The number of *bytes* backing the array
    pub fn data_len(&self) -> usize {
        self.data.len()
    }

    pub fn get(&self, idx: usize) -> Option<T> {
        let offset = idx * T::SIZE;
        self.data.get(offset..offset + T::SIZE).and_then(T::read)
    }

    pub unsafe fn get_unchecked(&self, idx: usize) -> T {
        let offset = idx * T::SIZE;
        T::read(self.data.get_unchecked(offset..offset + T::SIZE)).unwrap()
    }

    pub fn iter(&self) -> impl Iterator<Item = T> + 'a {
        let mut offset = 0;
        let blob = self.data.clone();

        std::iter::from_fn(move || {
            let result = blob.get(offset..offset + T::SIZE).and_then(T::read);
            offset += T::SIZE;
            result
        })
    }

    //taken from std
    #[inline]
    pub fn binary_search_by<F>(&self, mut f: F) -> Result<usize, usize>
    where
        F: FnMut(T) -> std::cmp::Ordering,
    {
        use std::cmp::Ordering::*;
        let mut size = self.len();
        let mut left = 0;
        let mut right = size;
        while left < right {
            let mid = left + size / 2;
            //let mid_off = mid * T::SIZE;

            // SAFETY: the call is made safe by the following invariants:
            // - `mid >= 0`
            // - `mid < size`: `mid` is limited by `[left; right)` bound.
            let cmp = f(unsafe { self.get_unchecked(mid) });
            //let cmp = f(self.get_unchecked(mid).unwrap());

            // The reason why we use if/else control flow rather than match
            // is because match reorders comparison operations, which is perf sensitive.
            // This is x86 asm for u8: https://rust.godbolt.org/z/8Y8Pra.
            if cmp == Less {
                left = mid + 1;
            } else if cmp == Greater {
                right = mid;
            } else {
                // SAFETY: same as the `get_unchecked` above
                //unsafe { crate::intrinsics::assume(mid < self.len()) };
                return Ok(mid);
            }

            size = right - left;
        }
        Err(left)
    }
}

impl<'a, T: ExactSized + FontRead<'a> + Ord> Array<'a, T> {
    pub fn binary_search(&self, item: &T) -> Result<usize, usize> {
        self.binary_search_by(|other| other.cmp(item))
    }
}

/// An array with non-uniform member sizes.
#[derive(Clone, Debug)]
pub struct VariableSizeArray<'a, T> {
    data: Blob<'a>,
    _t: std::marker::PhantomData<T>,
}

pub trait DynamicSize<'a>: FontRead<'a> {
    fn size(blob: Blob<'a>) -> Option<usize>;
}

impl<'a, T: DynamicSize<'a>> VariableSizeArray<'a, T> {
    pub fn new(data: Blob<'a>, offset: usize, _len: usize) -> Option<Self> {
        let data = data.get(offset..data.len())?;
        Some(Self {
            data,
            _t: std::marker::PhantomData,
        })
    }

    pub fn get(&self, idx: usize) -> Option<T> {
        let mut offset = 0;
        for _ in 0..idx {
            offset += T::size(self.data.get(offset..self.data.len())?)?;
        }
        T::read(self.data.get(offset..self.data.len())?)
    }

    pub fn iter(&self) -> impl Iterator<Item = T> + 'a {
        let mut offset = 0;
        let blob = self.data.clone();
        std::iter::from_fn(move || {
            let blob = blob.get(offset..blob.len())?;
            offset += T::size(blob.clone())?;
            T::read(blob)
        })
    }
}

impl<T: ExactSized> Array<'_, T> {}

impl<'a, T> Default for Array<'a, T> {
    fn default() -> Self {
        todo!()
    }
}

impl<'a, T: std::fmt::Debug + ExactSized + FontRead<'a>> std::fmt::Debug for Array<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "{} {} items:", std::any::type_name::<Self>(), self.len())?;
        f.debug_list().entries(self.iter()).finish()
    }
}
