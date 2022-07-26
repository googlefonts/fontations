//! OpenType Layout

mod gpos;

include!("../generated/layout.rs");

/// A typed lookup table.
///
/// Our generated code doesn't handle generics, so we define this ourselves.
pub struct TypedLookup<'a, T> {
    inner: Lookup<'a>,
    phantom: std::marker::PhantomData<T>,
}

impl<'a, T: FontRead<'a>> TypedLookup<'a, T> {
    pub(crate) fn new(inner: Lookup<'a>) -> Self {
        TypedLookup {
            inner,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn subtables<'b: 'a>(&'b self) -> impl Iterator<Item = T> + 'a {
        self.inner
            .subtable_offsets()
            .iter()
            .flat_map(|off| self.inner.resolve_offset(off.get()))
    }
}

impl<'a, T> std::ops::Deref for TypedLookup<'a, T> {
    type Target = Lookup<'a>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

fn delta_value_count(start_size: u16, end_size: u16, delta_format: DeltaFormat) -> usize {
    let range_len = start_size.saturating_add(1).saturating_sub(end_size) as usize;
    let val_per_word = match delta_format {
        DeltaFormat::Local2BitDeltas => 8,
        DeltaFormat::Local4BitDeltas => 4,
        DeltaFormat::Local8BitDeltas => 2,
        _ => return 0,
    };

    let count = range_len / val_per_word;
    let extra = (range_len % val_per_word).min(1);
    count + extra
}

fn minus_one(val: impl Into<usize>) -> usize {
    val.into().saturating_sub(1)
}
