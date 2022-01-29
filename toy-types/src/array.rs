use crate::{Blob, ExactSized, FontRead};

pub struct Array<'a, T> {
    data: Blob<'a>,
    _t: std::marker::PhantomData<T>,
}

impl<'a, T: ExactSized + FontRead<'a>> Array<'a, T> {
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
}

impl<T: ExactSized> Array<'_, T> {}

///// An array with non-uniform member sizes.
//pub struct VariableSizedArray<'a, T> {
//data: Blob<'a>,
//_t: std::marker::PhantomData<T>,
//}

impl<'a, T> Default for Array<'a, T> {
    fn default() -> Self {
        todo!()
    }
}
