/// An array with members of different sizes
pub struct VarArray<'a, T> {
    bytes: &'a [u8],
    _t: std::marker::PhantomData<T>,
}

impl<'a, T> VarArray<'a, T> {}

impl<'a, T: super::VarSized<'a>> VarArray<'a, T> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            _t: std::marker::PhantomData,
        }
    }

    pub fn get(&self, idx: usize) -> Option<T> {
        let mut offset = 0;

        for _ in 0..idx {
            let nxt = self.bytes.get(offset..).and_then(T::read)?;
            offset += nxt.len();
        }
        self.bytes.get(offset..).and_then(T::read)
    }

    pub fn iter(&self) -> impl Iterator<Item = T> + 'a {
        let mut offset = 0;
        let bytes = self.bytes;
        std::iter::from_fn(move || {
            //let blob = blob.get(offset..blob.len())?;
            let nxt = bytes.get(offset..).and_then(T::read)?;
            offset += nxt.len();
            Some(nxt)
        })
    }
}
