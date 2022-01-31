use crate::{Blob, ExactSized, FontRead};

#[derive(Clone, Debug)]
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

    pub fn get(&self, idx: usize) -> Option<T> {
        let offset = idx * T::SIZE;
        self.data.get(offset..offset + T::SIZE).and_then(T::read)
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
