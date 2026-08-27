//! One array type, parameterised over where its elements live.
//!
//! An array is split in two. An [`ArrayStore`] knows where the elements are and
//! how to walk them; an [`ArrayElement`] knows how to turn one into a value.
//! The two vary independently, which is why the six array types in the crate
//! today collapse to one type over four stores.
//!
//! Reading an element does not return a `Result`. Where an element can be
//! absent — a null or unreadable offset, a truncated variable-length element —
//! that shows up in [`ArrayElement::Output`] as an [`Option`], and where it
//! cannot, the element type comes back bare. Records are the latter case: the
//! store checks its whole extent when it is built, so every element within it
//! is known to be present.

#![deny(clippy::arithmetic_side_effects)]

use core::marker::PhantomData;
use core::ops::Range;

use bytemuck::AnyBitPattern;
use font_types::{BigEndian, FixedSize, Offset16, Scalar};

use super::bytes::Bytes;
use super::traits::{ComputedSize, RawOffset, Resolve, Table, VariableSize};

/// Where the elements of an array live, and how to walk them.
///
/// A store is `Copy` so that walking one does not borrow the array.
pub trait ArrayStore<'a>: Copy {
    /// Identifies one element: a byte position, a raw offset value, or the
    /// element's own data, depending on the store.
    type Item;

    /// Walks every element in order.
    ///
    /// Implementations walk incrementally where they can, rather than computing
    /// each position from its index.
    fn iter(self) -> impl Iterator<Item = Self::Item> + Clone;

    /// Locates a single element, or `None` if `idx` is past the end.
    ///
    /// The default walks to it, which is `O(n)`; stores that can address an
    /// element directly override it.
    fn get(self, idx: usize) -> Option<Self::Item> {
        self.iter().nth(idx)
    }
}

/// A store whose length is known without walking it.
///
/// Separate so that an array of self-describing elements, whose length cannot
/// be had cheaply, is simply an array with no `len`.
pub trait SizedArrayStore<'a>: ArrayStore<'a> {
    fn len(self) -> usize;

    fn is_empty(self) -> bool {
        self.len() == 0
    }
}

/// How to read one element of an array.
pub trait ArrayElement<'a>: Sized {
    /// External state needed to read an element.
    type Args: Copy;
    /// Where this kind of element is found.
    type Store: ArrayStore<'a>;
    /// What reading one produces: `Self` for an element that is always present,
    /// `Option<T>` for one that may not be.
    type Output;

    /// Reads the element `item` locates. Total: an element that may be missing
    /// says so through `Output`.
    fn read(
        store: Self::Store,
        item: <Self::Store as ArrayStore<'a>>::Item,
        args: Self::Args,
    ) -> Self::Output;
}

/// An array whose elements are read on access.
pub struct Array<'a, E: ArrayElement<'a>> {
    store: E::Store,
    args: E::Args,
}

impl<'a, E: ArrayElement<'a>> Array<'a, E> {
    /// Builds an array over `store`.
    #[inline]
    pub fn with_store(store: E::Store, args: E::Args) -> Self {
        Self { store, args }
    }

    /// The store the elements are read from.
    #[inline]
    pub fn store(&self) -> E::Store {
        self.store
    }

    /// The args the elements are read with.
    #[inline]
    pub fn args(&self) -> E::Args {
        self.args
    }

    /// Returns the element at `idx`, or `None` if `idx` is past the end.
    #[inline]
    pub fn get(&self, idx: usize) -> Option<E::Output> {
        Some(E::read(self.store, self.store.get(idx)?, self.args))
    }

    /// Returns an iterator over the elements.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = E::Output> + 'a + Clone
    where
        E::Store: 'a,
        E::Args: 'a,
    {
        let (store, args) = (self.store, self.args);
        store.iter().map(move |item| E::read(store, item, args))
    }
}

impl<'a, E: ArrayElement<'a>> Array<'a, E>
where
    E::Store: SizedArrayStore<'a>,
{
    #[inline]
    pub fn len(&self) -> usize {
        self.store.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// derived impls would demand `E: Clone`, when only the store and args need it
impl<'a, E: ArrayElement<'a>> Clone for Array<'a, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, E: ArrayElement<'a>> Copy for Array<'a, E> {}

impl<'a, E: ArrayElement<'a>> Default for Array<'a, E>
where
    E::Store: Default,
    E::Args: Default,
{
    fn default() -> Self {
        Self {
            store: Default::default(),
            args: Default::default(),
        }
    }
}

impl<'a, E: ArrayElement<'a>> core::fmt::Debug for Array<'a, E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Array").finish_non_exhaustive()
    }
}

/// Elements laid out end to end at a fixed stride.
///
/// The whole extent is checked once, when the store is built. That single check
/// is what lets reading an element be total, and what lets walking the
/// store be one addition per element with no further bounds arithmetic.
#[derive(Clone, Copy, Default)]
pub struct StridedStore<'a> {
    /// The enclosing data, retained whole: an element's offsets are measured
    /// from here, not from the element.
    data: Bytes<'a>,
    start: usize,
    stride: usize,
    len: usize,
}

impl<'a> StridedStore<'a> {
    /// Builds a store over the elements occupying `range` within `data`.
    ///
    /// Returns `None` if `range` is not entirely within `data`.
    pub fn new(data: Bytes<'a>, range: Range<usize>, stride: usize) -> Option<Self> {
        let available = data.as_bytes().get(range.clone())?.len();
        let len = available.checked_div(stride).unwrap_or(0);
        Some(Self {
            data,
            start: range.start,
            stride,
            len,
        })
    }

    /// An empty store over `data`.
    ///
    /// The `unwrap_or_default` case: a field beyond `MIN_SIZE` whose extent is
    /// not there reads as empty, rather than as an error the caller has to
    /// unwrap. This is what the crate does today and what callers expect.
    pub fn empty(data: Bytes<'a>) -> Self {
        Self {
            data,
            start: 0,
            stride: 0,
            len: 0,
        }
    }

    /// Builds a store over `count` elements beginning at `start`.
    pub fn with_count(data: Bytes<'a>, start: usize, count: usize, stride: usize) -> Option<Self> {
        let end = count.checked_mul(stride)?.checked_add(start)?;
        Self::new(data, start..end, stride)
    }

    /// The data the elements were located within: the whole enclosing extent,
    /// not a slice at any one element.
    #[inline]
    pub fn data(self) -> Bytes<'a> {
        self.data
    }

    #[inline]
    pub fn stride(self) -> usize {
        self.stride
    }

    /// The data of one element, sliced to exactly that element.
    #[inline]
    pub fn element_data(self, item: usize) -> Option<Bytes<'a>> {
        self.data.slice(item..item.checked_add(self.stride)?)
    }
}

impl<'a> ArrayStore<'a> for StridedStore<'a> {
    type Item = usize;

    #[inline]
    fn iter(self) -> impl Iterator<Item = usize> + Clone {
        // `new` proved that `start + len * stride` indexes `data`, so every
        // position below is representable and each step is a plain add.
        let stride = self.stride;
        let mut pos = self.start;
        (0..self.len).map(move |_| {
            let at = pos;
            #[allow(clippy::arithmetic_side_effects)] // bounded by `new`
            {
                pos += stride;
            }
            at
        })
    }

    #[inline]
    fn get(self, idx: usize) -> Option<usize> {
        if idx >= self.len {
            return None;
        }
        idx.checked_mul(self.stride)?.checked_add(self.start)
    }
}

impl<'a> SizedArrayStore<'a> for StridedStore<'a> {
    #[inline]
    fn len(self) -> usize {
        self.len
    }
}

impl<'a, E> Array<'a, E>
where
    E: ArrayElement<'a, Store = StridedStore<'a>>
        + ComputedSize<Args = <E as ArrayElement<'a>>::Args>,
{
    /// Builds an array of `count` elements beginning at `start` within `data`.
    ///
    /// `data` is the enclosing table's data, so the elements can resolve their
    /// own offsets; `start` locates the first of them.
    ///
    /// This is where a strided array's one bounds check happens. Everything
    /// after it — every `get`, every step of every `iter`, every field read
    /// inside an element — is total.
    ///
    /// The two bounds are the whole story: [`ComputedSize`] says how far apart
    /// the elements are, [`ArrayElement`] says how to read one. There is no
    /// third trait tying them together, because nothing else needs to know they
    /// belong to the same type.
    pub fn of_computed(
        data: Bytes<'a>,
        start: usize,
        count: usize,
        args: <E as ArrayElement<'a>>::Args,
    ) -> Option<Self> {
        let store = StridedStore::with_count(data, start, count, E::computed_size(args))?;
        Some(Self::with_store(store, args))
    }

    /// As [`of_computed`][Self::of_computed], but empty when the extent is not
    /// there.
    ///
    /// This is the shape a table accessor uses for a field beyond `MIN_SIZE`:
    /// the crate returns a value rather than an `Option` there today, and a
    /// truncated array reads as having no elements.
    pub fn of_computed_or_empty(
        data: Bytes<'a>,
        start: usize,
        count: usize,
        args: <E as ArrayElement<'a>>::Args,
    ) -> Self {
        Self::of_computed(data, start, count, args)
            .unwrap_or_else(|| Self::with_store(StridedStore::empty(data), args))
    }
}

/// Elements borrowed out of a zerocopy slice.
///
/// The whole run is read and bounds checked once, when the store is built, and
/// an element is then a slice index. This is the store for fixed-size records
/// whose fields are all scalars: it is the only one whose `Item` is already a
/// reference, so an element type over it — [`WithParent`][super::with_parent::WithParent] —
/// never has to read anything, and never has to say it failed.
///
/// [`StridedStore`] could locate the same elements, but only by handing out a
/// position for the element type to read from, which puts a bounds check and a
/// copy on the hot path and leaves the element type owing an answer for the
/// case it cannot satisfy. Where the elements are is the store's business.
pub struct SliceStore<'a, R> {
    records: &'a [R],
    /// The enclosing data, for elements that resolve offsets against it.
    parent: Bytes<'a>,
}

impl<'a, R: AnyBitPattern + FixedSize> SliceStore<'a, R> {
    /// Builds a store over `count` records beginning at `start` within `data`.
    ///
    /// Returns `None` if the run does not fit.
    pub fn new(data: Bytes<'a>, start: usize, count: usize) -> Option<Self> {
        let end = count.checked_mul(R::RAW_BYTE_LEN)?.checked_add(start)?;
        Some(Self {
            records: data.read_array(start..end)?,
            parent: data,
        })
    }
}

impl<'a, R> SliceStore<'a, R> {
    /// Builds a store over records already read.
    pub fn with_slice(records: &'a [R], parent: Bytes<'a>) -> Self {
        Self { records, parent }
    }

    /// The records, as the zerocopy slice they are.
    ///
    /// This is the bulk view, available for free: it is what the store holds.
    #[inline]
    pub fn records(self) -> &'a [R] {
        self.records
    }

    /// The data the records' offsets are measured from.
    #[inline]
    pub fn parent(self) -> Bytes<'a> {
        self.parent
    }
}

// deriving these would demand `R: Clone`, which is not needed behind a slice
impl<R> Clone for SliceStore<'_, R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R> Copy for SliceStore<'_, R> {}

impl<R> Default for SliceStore<'_, R> {
    fn default() -> Self {
        Self {
            records: &[],
            parent: Default::default(),
        }
    }
}

impl<'a, R: 'a> ArrayStore<'a> for SliceStore<'a, R> {
    type Item = &'a R;

    #[inline]
    fn iter(self) -> impl Iterator<Item = &'a R> + Clone {
        self.records.iter()
    }

    #[inline]
    fn get(self, idx: usize) -> Option<&'a R> {
        self.records.get(idx)
    }
}

impl<'a, R: 'a> SizedArrayStore<'a> for SliceStore<'a, R> {
    #[inline]
    fn len(self) -> usize {
        self.records.len()
    }
}

/// Elements reached through a table of offsets.
pub struct OffsetStore<'a, O: Scalar> {
    offsets: &'a [BigEndian<O>],
    data: Bytes<'a>,
}

impl<'a, O: Scalar> OffsetStore<'a, O> {
    pub fn new(offsets: &'a [BigEndian<O>], data: Bytes<'a>) -> Self {
        Self { offsets, data }
    }

    /// The data the offsets are resolved against.
    #[inline]
    pub fn data(self) -> Bytes<'a> {
        self.data
    }

    /// The raw offsets.
    #[inline]
    pub fn offsets(self) -> &'a [BigEndian<O>] {
        self.offsets
    }
}

// deriving these would demand `O: Clone`, which is not needed behind a slice
impl<O: Scalar> Clone for OffsetStore<'_, O> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<O: Scalar> Copy for OffsetStore<'_, O> {}

impl<O: Scalar> Default for OffsetStore<'_, O> {
    fn default() -> Self {
        Self {
            offsets: &[],
            data: Default::default(),
        }
    }
}

impl<'a, O: Scalar> ArrayStore<'a> for OffsetStore<'a, O> {
    type Item = O;

    #[inline]
    fn iter(self) -> impl Iterator<Item = O> + Clone {
        self.offsets.iter().map(|offset| offset.get())
    }

    #[inline]
    fn get(self, idx: usize) -> Option<O> {
        self.offsets.get(idx).map(|offset| offset.get())
    }
}

impl<'a, O: Scalar> SizedArrayStore<'a> for OffsetStore<'a, O> {
    #[inline]
    fn len(self) -> usize {
        self.offsets.len()
    }
}

/// An element reached by resolving an offset.
///
/// One element type covers both nullable and non-nullable offsets: reading
/// either yields `Option<T>`, because an offset declared non-nullable may still
/// be null in the font in front of us. `O` carries the declared nullability —
/// `Offset16` or `Nullable<Offset16>` — which is what the compile side reads;
/// the read side treats them the same.
///
/// An array of these is spelled `Array<'a, OffsetTo<Target, Offset16>>`. There
/// were once `ArrayOfOffsets` and `ArrayOfNullableOffsets` aliases for the two
/// nullabilities, from when they were separate types; they said nothing the
/// element type does not.
pub struct OffsetTo<T, O = Offset16>(PhantomData<fn() -> (T, O)>);

impl<'a, T, O> ArrayElement<'a> for OffsetTo<T, O>
where
    T: Table<'a>,
    O: Scalar + RawOffset + 'a,
{
    type Args = T::Args;
    type Store = OffsetStore<'a, O>;
    type Output = Option<T>;

    #[inline]
    fn read(store: Self::Store, item: O, args: T::Args) -> Option<T> {
        store.data().resolve_with_args(item, args)
    }
}

impl<'a, T, O> Array<'a, OffsetTo<T, O>>
where
    T: Table<'a>,
    O: Scalar + RawOffset + 'a,
{
    pub fn of_offsets(offsets: &'a [BigEndian<O>], data: Bytes<'a>, args: T::Args) -> Self {
        Self::with_store(OffsetStore::new(offsets, data), args)
    }
}

/// Elements of non-uniform length, each of which describes its own size.
///
/// This is the second kind of runtime-known size, and the one
/// [`ComputedSize`] deliberately excludes: the size is read from the element
/// rather than computed from args, so a run can only be walked. An element here
/// is handed its own slice and read as a [`Table`], which every variably
/// sized thing in the crate can be, because none of them holds an offset
/// measured from an ancestor.
///
/// There is no [`SizedArrayStore`] impl: the number of elements cannot be known
/// without walking them, so an array over this store has no `len` and `get` is
/// `O(n)` — the same as today, but now said by the type rather than by a doc
/// comment.
pub struct VariableSizeStore<'a, T> {
    data: Bytes<'a>,
    element: PhantomData<fn() -> T>,
}

impl<'a, T> VariableSizeStore<'a, T> {
    pub fn new(data: Bytes<'a>) -> Self {
        Self {
            data,
            element: PhantomData,
        }
    }

    #[inline]
    pub fn data(self) -> Bytes<'a> {
        self.data
    }
}

// deriving these would demand `T: Clone`, which is not needed behind a marker
impl<T> Clone for VariableSizeStore<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for VariableSizeStore<'_, T> {}

impl<T> Default for VariableSizeStore<'_, T> {
    fn default() -> Self {
        Self::new(Bytes::default())
    }
}

impl<'a, T: VariableSize> ArrayStore<'a> for VariableSizeStore<'a, T> {
    /// The element's own data, sliced to it, or `None` if it is truncated.
    type Item = Option<Bytes<'a>>;

    fn iter(self) -> impl Iterator<Item = Self::Item> + Clone {
        let mut pos = 0usize;
        let mut done = false;
        core::iter::from_fn(move || {
            if done || pos >= self.data.len() {
                return None;
            }
            let Some(len) = T::len_at(self.data, pos) else {
                done = true;
                return Some(None);
            };
            // a zero-length element would otherwise spin forever
            if len == 0 {
                done = true;
                return None;
            }
            let Some(end) = pos.checked_add(len) else {
                done = true;
                return Some(None);
            };
            let item = self.data.slice(pos..end);
            pos = end;
            Some(item)
        })
    }
}

/// An element that describes its own length, read from its own data.
///
/// The counterpart to a [`ComputedSize`] element: where that one is a cursor into the
/// parent, this one is handed a slice at itself and read as a table, because
/// walking is the only way to find it and nothing that is walked needs a base
/// from further out.
pub struct VariableSizeOf<T>(PhantomData<fn() -> T>);

impl<'a, T: Table<'a, Args = ()> + VariableSize> ArrayElement<'a> for VariableSizeOf<T> {
    type Args = ();
    type Store = VariableSizeStore<'a, T>;
    /// `None` for an element the walk could not complete.
    type Output = Option<T>;

    #[inline]
    fn read(_: VariableSizeStore<'a, T>, item: Option<Bytes<'a>>, _: ()) -> Option<T> {
        T::read(item?)
    }
}

/// An array of elements that describe their own length.
pub type VariableSizeArray<'a, T> = Array<'a, VariableSizeOf<T>>;

impl<'a, T: Table<'a, Args = ()> + VariableSize> Array<'a, VariableSizeOf<T>> {
    /// Builds an array over `data`, walked by the element's own
    /// [`VariableSize`] impl.
    pub fn of_variable_size(data: Bytes<'a>) -> Self {
        Self::with_store(VariableSizeStore::new(data), ())
    }
}
