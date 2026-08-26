//! Pairing a record with the data its offsets are measured from.

use core::ops::Deref;

use bytemuck::AnyBitPattern;
use font_types::FixedSize;

use super::array::{Array, ArrayElement, SliceStore};
use super::bytes::Bytes;

/// A fixed-size record together with the data its offsets resolve against.
///
/// Twenty five of the crate's records hold an offset measured from the start of
/// the enclosing table. Today the record is handed out bare — as `&'a Record`
/// out of a zerocopy slice — so every offset accessor on it takes a `data`
/// argument that the caller has to fetch from the right ancestor and pass down
/// by hand. Getting that wrong is silent: the offset resolves against the wrong
/// base and yields a plausible, wrong table.
///
/// Wrapping the record removes the argument. `WithParent<'a, R>` derefs to `R`,
/// so the plain field accessors are unchanged, and codegen emits the offset
/// accessors on the wrapper instead, where the base is already at hand:
///
/// ```ignore
/// impl MarkRecord {                       // plain fields, no data needed
///     pub fn mark_class(&self) -> u16;
///     pub fn mark_anchor_offset(&self) -> Offset16;
/// }
/// impl<'a> WithParent<'a, MarkRecord> {   // offsets, base already held
///     pub fn mark_anchor(&self) -> Option<Anchor<'a>>;
/// }
/// ```
///
/// A record with no offsets is never wrapped; it stays a bare `&'a R` in a
/// zerocopy slice, which is both smaller and faster to scan.
///
/// # Why the record is borrowed
///
/// Holding `R` by value would mean copying it out of the parent's data, which
/// needs a bounds check, which the wrapper would then have to be able to fail —
/// or to fabricate a zeroed record to stand in. Borrowing avoids the question:
/// a `&'a R` cannot be produced without the check having already passed, so the
/// check happens once, further out, where the records are located. It is also
/// a constant 24 bytes regardless of how large the record is.
#[derive(Clone, Copy)]
pub struct WithParent<'a, R> {
    record: &'a R,
    parent: Bytes<'a>,
}

impl<'a, R> WithParent<'a, R> {
    /// Pairs `record` with the data its offsets are measured from.
    #[inline]
    pub fn new(record: &'a R, parent: Bytes<'a>) -> Self {
        Self { record, parent }
    }

    /// The record itself.
    #[inline]
    pub fn record(&self) -> &'a R {
        self.record
    }

    /// The data this record's offsets are measured from.
    #[inline]
    pub fn parent(&self) -> Bytes<'a> {
        self.parent
    }
}

impl<'a, R: AnyBitPattern + FixedSize> WithParent<'a, R> {
    /// Borrows the record at `pos` within `parent`, or `None` if `parent` is
    /// too short.
    ///
    /// For a record embedded directly in a table rather than reached through an
    /// array. Nothing is fabricated for the short case: a caller who wants a
    /// value regardless can say so, with `.map(|r| *r).unwrap_or_default()` or
    /// `.copied().unwrap_or_default()` on the record itself.
    #[inline]
    pub fn at(parent: Bytes<'a>, pos: usize) -> Option<Self> {
        Some(Self::new(parent.read_ref_at(pos)?, parent))
    }
}

impl<R> Deref for WithParent<'_, R> {
    type Target = R;

    #[inline]
    fn deref(&self) -> &R {
        self.record
    }
}

/// A wrapped record is an array element found by indexing a slice.
///
/// This is what the borrow buys. The elements are read and bounds checked once,
/// as a `&'a [R]`, when the store is built; after that an element is a slice
/// index, and pairing it with the parent cannot fail and copies nothing. There
/// is no fallback here because there is no read here.
impl<'a, R: AnyBitPattern + FixedSize + 'a> ArrayElement<'a> for WithParent<'a, R> {
    type Args = ();
    type Store = SliceStore<'a, R>;
    type Output = Self;

    #[inline]
    fn read(store: SliceStore<'a, R>, item: &'a R, _: ()) -> Self {
        Self::new(item, store.parent())
    }
}

impl<'a, R: AnyBitPattern + FixedSize + 'a> Array<'a, WithParent<'a, R>> {
    /// Builds an array of `count` records beginning at `start` within `data`.
    ///
    /// This is the one bounds check: it reads the whole run of records as a
    /// slice, and everything after it is total.
    pub fn of_zerocopy_records(data: Bytes<'a>, start: usize, count: usize) -> Option<Self> {
        Some(Self::with_store(SliceStore::new(data, start, count)?, ()))
    }

    /// As [`of_zerocopy_records`][Self::of_zerocopy_records], but empty when
    /// the run is not there.
    ///
    /// The shape a table accessor uses beyond `MIN_SIZE`: a value rather than
    /// an `Option`, reading as no elements when truncated.
    pub fn of_zerocopy_records_or_empty(data: Bytes<'a>, start: usize, count: usize) -> Self {
        Self::of_zerocopy_records(data, start, count)
            .unwrap_or_else(|| Self::with_store(SliceStore::with_slice(&[], data), ()))
    }
}

impl<R: core::fmt::Debug> core::fmt::Debug for WithParent<'_, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.record.fmt(f)
    }
}

impl<R: PartialEq> PartialEq for WithParent<'_, R> {
    /// Compares the records; the base they resolve against is not part of the
    /// value.
    fn eq(&self, other: &Self) -> bool {
        self.record == other.record
    }
}
