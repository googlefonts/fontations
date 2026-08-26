//! What a table is, how a record's size is known, and how an offset is followed.

#![deny(clippy::arithmetic_side_effects)]

use types::{Nullable, Offset16, Offset24, Offset32};

use super::bytes::Bytes;

/// Something read from data that starts at its own first byte.
///
/// The data handed to [`read`][Self::read] begins at the table and runs to the
/// end of whatever contains it, so a table's offsets are measured from byte
/// zero of that data. This is the property that distinguishes a table from a
/// a record, and the reason the two are read differently: a record cannot
/// be given data sliced to itself without losing the base its offsets resolve
/// against.
///
/// # Validation
///
/// Reading a table checks one thing: that [`MIN_SIZE`][Self::MIN_SIZE] bytes
/// are present. Nothing else is validated, and nothing is read. Accessors for
/// fields covered by `MIN_SIZE` therefore return their value directly;
/// accessors for anything beyond it return [`Option`].
pub trait Table<'a>: Sized {
    /// External state needed to read this table. `()` for most.
    type Args: Copy;

    /// The size of the table's fixed header: the bytes that must be present
    /// for the table to exist at all.
    const MIN_SIZE: usize;

    /// Reads a table from data beginning at the table's first byte.
    ///
    /// Returns `None` if fewer than `MIN_SIZE` bytes are available.
    fn read_with_args(data: Bytes<'a>, args: Self::Args) -> Option<Self>;

    /// Reads a table that needs no external state.
    #[inline]
    fn read(data: Bytes<'a>) -> Option<Self>
    where
        Self: Table<'a, Args = ()>,
    {
        Self::read_with_args(data, ())
    }
}

/// How many bytes a record occupies, when that depends on its read args.
///
/// The middle row of the taxonomy in the [module docs][self]: the length is
/// computed rather than fixed or read, so a run of these is uniform and can be
/// addressed in `O(1)`.
///
/// This is the crate's existing `ComputeSize` with the `Result` dropped. A
/// length too large to represent saturates, and then fails the one extent check
/// made by whoever is locating the elements — it cannot fail here, and neither
/// can building the record, which reads nothing.
pub trait ComputedSize {
    /// External state the size depends on.
    type Args: Copy;

    /// The number of bytes one element occupies.
    fn computed_size(args: Self::Args) -> usize;
}

/// How many bytes something occupies in the font, when only the bytes can say.
///
/// The third row of the taxonomy on [`ComputedSize`]: the length is read from
/// the element rather than computed from args, so a run of these can only be
/// walked and an array of them has no `len`.
///
/// An element like this is handed a slice at itself, which is the same thing a
/// [`Table`] is handed — so it implements `Table` too, and that is not a
/// borrowed mechanism but the actual situation: what makes a record different
/// from a table is needing its parent, and a self-describing element does not.
/// Every variably sized thing in the crate holds no offsets at all.
pub trait VariableSize {
    /// The total bytes of the element at `pos`, including whatever field
    /// carries that length. `None` if the element is truncated.
    fn len_at(data: Bytes, pos: usize) -> Option<usize>;
}

/// Reads the inline discriminant that says which subtable follows.
///
/// Used by a generic group, where the type of the payload is chosen by a value
/// in the wrapper rather than by a format word in the payload itself.
pub trait Discriminant {
    fn read_discriminant(data: Bytes<'_>) -> Option<u16>;
}

/// Resolving an offset against the data it is measured from.
///
/// There is one method for each arity rather than one per nullability: a null
/// offset and an unreadable one both give `None`. Marking an offset non
/// nullable says what a well formed font contains, not what this one does, so
/// it cannot make the resolved accessor infallible — which is why the read side
/// no longer distinguishes the two at all.
pub trait Resolve<'a> {
    /// Resolves `offset` to a table, or `None` if it is null or unreadable.
    fn resolve<T: Table<'a, Args = ()>>(&self, offset: impl RawOffset) -> Option<T> {
        self.resolve_with_args(offset, ())
    }

    /// Resolves `offset` to a table needing external state.
    fn resolve_with_args<T: Table<'a>>(&self, offset: impl RawOffset, args: T::Args) -> Option<T>;
}

/// An offset field, whatever its width and whatever its declared nullability.
///
/// `Nullable<O>` and `O` both land here, and resolve the same way. The wrapper
/// survives only because the raw accessor's type is what the compile side reads
/// to decide whether a null is legal on the way out.
pub trait RawOffset: Copy {
    /// The offset as a byte count, where zero means null.
    fn to_offset(self) -> usize;
}

macro_rules! impl_raw_offset {
    ($($ty:ty),*) => {
        $(
            impl RawOffset for $ty {
                #[inline]
                fn to_offset(self) -> usize {
                    self.to_u32() as usize
                }
            }

            impl RawOffset for Nullable<$ty> {
                #[inline]
                fn to_offset(self) -> usize {
                    self.offset().to_u32() as usize
                }
            }
        )*
    };
}

impl_raw_offset!(Offset16, Offset24, Offset32);

impl<'a> Resolve<'a> for Bytes<'a> {
    #[inline]
    fn resolve_with_args<T: Table<'a>>(&self, offset: impl RawOffset, args: T::Args) -> Option<T> {
        let offset = offset.to_offset();
        if offset == 0 {
            return None;
        }
        T::read_with_args(self.split_off(offset)?, args)
    }
}
