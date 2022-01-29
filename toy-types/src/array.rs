use crate::Blob;

pub struct Array<'a, T> {
    data: Blob<'a>,
    _t: std::marker::PhantomData<T>,
}

impl<'a, T> Default for Array<'a, T> {
    fn default() -> Self {
        todo!()
    }
}
