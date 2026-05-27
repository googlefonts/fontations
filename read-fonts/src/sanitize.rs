//! the traits we'll need to generate for sanitize

use bytemuck::AnyBitPattern;
use types::{BigEndian, FixedSize, Nullable, Scalar};

use crate::{
    array::VarLenArray, font_data::Cursor, read::VarSize, ComputeSize, FontData, FontRead,
    FontReadWithArgs, Offset, ReadArgs, ReadError, ResolveOffset,
};

/// The bytes of the current table being sanitized, along with shared sanitize state.
///
/// This is bundled together because we need to update the shared state as we
/// navigate the bytes.
pub struct SanitizeContext<'a> {
    cursor: Cursor<'a>,
    state: &'a mut SanitizeState,
}

/// State tracked during during a sanitize pass
#[derive(Clone, Debug, Default)]
pub(crate) struct SanitizeState {
    // only used in COLRv1
    _recursion_depth: u32,
    // gpos/gsub
    _subtable_depth: u32,
    _max_ops: u32,
    // some stuff goes in here?
}

impl<'a> SanitizeContext<'a> {
    pub(crate) fn new(data: FontData<'a>, state: &'a mut SanitizeState) -> Self {
        Self {
            cursor: data.cursor(),
            state,
        }
    }

    pub(crate) fn data(&self) -> FontData<'a> {
        self.cursor.data
    }

    /// Read a scalar and advance the cursor
    #[must_use]
    pub(crate) fn read<T: Scalar>(&mut self) -> Result<T, ReadError> {
        self.cursor.read()
    }

    /// Read a scalar at a specific offset without advancing the cursor.
    ///
    /// the position is absolute in the underlying data; this is only expected
    /// to be called when parsing a format group.
    pub(crate) fn peek_at<T: Scalar>(&self, offset: usize) -> Result<T, ReadError> {
        assert_eq!(self.cursor.position(), Ok(0));
        self.cursor.data.read_at(offset)
    }

    /// Recursively sanitize an offset, and advance the cursor
    #[must_use]
    pub(crate) fn sanitize_offset<O, T>(&mut self, args: T::Args) -> Result<(), ReadError>
    where
        O: Offset + Scalar,
        T: Sanitize,
    {
        let offset = self.read::<O>()?;
        self.descend_into_offset(offset, |ctx| T::sanitize(ctx, args))
    }

    /// Track state while descending into a child offset.
    ///
    /// Most importantly, this updates the context's data so it points to the
    /// the new offset's position, so that any subsequent offsets are resolved
    /// relative to that.
    fn descend_into_offset(
        &mut self,
        offset: impl Offset,
        f: impl FnOnce(&mut SanitizeContext) -> Result<(), ReadError>,
    ) -> Result<(), ReadError> {
        let offset = match offset.to_usize() {
            0 => return Ok(()),
            other => other,
        };

        let offset_data = self
            .cursor
            .data
            .split_off(offset)
            .ok_or(ReadError::OutOfBounds)?;

        //TODO: track descent here?
        let mut child_ctx = SanitizeContext {
            cursor: offset_data.cursor(),
            state: self.state,
        };

        f(&mut child_ctx)
    }

    /// Advance the cursor past a scalar
    pub(crate) fn advance<T: Scalar>(&mut self) {
        self.cursor.advance::<T>();
    }

    /// Advance the cursor by an arbitrary number of bytes
    pub(crate) fn advance_by(&mut self, n_bytes: usize) {
        self.cursor.advance_by(n_bytes);
    }

    /// advance the cursor by the length of the array, if the length doesn't overflow.
    #[must_use]
    pub(crate) fn sanitize_array<T: FixedSize>(&mut self, count: usize) -> Result<(), ReadError> {
        let len = count
            .checked_mul(T::RAW_BYTE_LEN)
            .ok_or(ReadError::OutOfBounds)?;
        self.advance_by(len);
        Ok(())
    }

    /// Advance the cursor by the length of the array, and recursively visit the offsets
    #[must_use]
    pub(crate) fn sanitize_array_of_offsets<O, T>(
        &mut self,
        count: usize,
        args: T::Args,
    ) -> Result<(), ReadError>
    where
        O: Offset + Scalar,
        T: Sanitize,
        BigEndian<O>: AnyBitPattern + FixedSize,
    {
        let array = self.cursor.read_array::<BigEndian<O>>(count)?;
        array.sanitize_offset::<T>(self, args)
    }

    /// Sanitize an offset that points to an array.
    ///
    /// this has a slightly funny signature because it needs to handle both
    /// scalar and struct members, and the structs might need to be recursed
    #[must_use]
    pub(crate) fn sanitize_offset_to_array<O, T, F>(
        &mut self,
        count: u16,
        len_only: bool,
        f: F,
    ) -> Result<(), ReadError>
    where
        O: Offset + Scalar,
        T: AnyBitPattern + FixedSize,
        F: Fn(&T, &mut SanitizeContext) -> Result<(), ReadError>,
    {
        let offset = self.read::<O>()?;
        if offset.to_usize() == 0 {
            return Ok(());
        }
        let array: &[T] = offset.resolve_with_args(self.cursor.data, &count)?;
        if !len_only {
            self.descend_into_offset(offset, |ctx| array.iter().try_for_each(|t| f(t, ctx)))
        } else {
            Ok(())
        }
    }

    /// Sanitize an offset-to-array where we already have the offset value.
    ///
    /// Used in records, where the offset is accessed via a getter rather than
    /// read from the cursor.
    #[must_use]
    pub(crate) fn sanitize_resolved_offset_to_array<O, T, F>(
        &mut self,
        offset: O,
        count: u16,
        len_only: bool,
        f: F,
    ) -> Result<(), ReadError>
    where
        O: Offset + Scalar,
        T: AnyBitPattern + FixedSize,
        F: Fn(&T, &mut SanitizeContext) -> Result<(), ReadError>,
    {
        if offset.to_usize() == 0 {
            return Ok(());
        }
        let array: &[T] = offset.resolve_with_args(self.cursor.data, &count)?;
        if !len_only {
            self.descend_into_offset(offset, |ctx| array.iter().try_for_each(|t| f(t, ctx)))
        } else {
            Ok(())
        }
    }

    /// Advance the cursor by the length of the array, recursing if necessesary
    #[must_use]
    pub(crate) fn sanitize_array_of_structs<T: FixedSize + AnyBitPattern + SanitizeStruct>(
        &mut self,
        count: usize,
        args: T::Args,
    ) -> Result<(), ReadError> {
        if T::can_skip() {
            self.sanitize_array::<T>(count)
        } else {
            let array = self.cursor.read_array::<T>(count)?;
            array.iter().try_for_each(|t| t.sanitize_struct(self, args))
        }
    }

    #[must_use]
    pub(crate) fn sanitize_computed_array<T>(
        &mut self,
        count: usize,
        args: T::Args,
        recurse: bool,
    ) -> Result<(), ReadError>
    where
        T: ComputeSize + SanitizeStruct + FontReadWithArgs<'a>,
    {
        if recurse {
            let array = self.cursor.read_computed_array::<T>(count, &args)?;
            array
                .iter()
                .try_for_each(|t| t.and_then(|t| t.sanitize_struct(self, args)))
        } else {
            T::compute_size(&args)
                .and_then(|len| len.checked_mul(count).ok_or(ReadError::OutOfBounds))
                .map(|n_bytes| self.advance_by(n_bytes))
        }
    }

    #[must_use]
    pub(crate) fn sanitize_var_len_array<T>(
        &mut self,
        count: usize,
        recurse: bool,
    ) -> Result<(), ReadError>
    where
        T: VarSize + SanitizeStruct<Args = ()> + FontRead<'a>,
    {
        let remaining = self.cursor.remaining().ok_or(ReadError::OutOfBounds)?;
        let total_len = T::total_len_for_count(remaining, count)?;
        // FIXME: do we actually ever recurse here?
        if recurse {
            let array = VarLenArray::<T>::read(remaining)?;
            for item in array.iter().take(count) {
                item?.sanitize_struct(self, ())?;
            }
        }
        self.advance_by(total_len);
        Ok(())
    }

    /// Validate the state for this table, returning an error if sanitize failed
    #[must_use]
    pub(crate) fn finish(&self) -> Result<(), ReadError> {
        //TODO: this would be a good place to check max ops, unless we're worried
        //about DDOS that doesn't touch offsets?
        self.cursor.position().map(|_| ())
    }
}

// okay so: the sanitize context _does_ kinda want to mutate, and it wants to mutate
// alongside access to the table data? probably?
//
// the annoying bit here is going to be figuring out how we keep track of our
// position as we... do stuff?

pub trait Sanitize: ReadArgs {
    /// recursively sanitizes this + all subgraphs.
    ///
    /// does not need to be called manually? we'll do this automatically?
    fn sanitize(ctx: &mut SanitizeContext<'_>, args: Self::Args) -> Result<(), ReadError>;
}

/// Sanitize functionality that is called on concrete types, instead of just
/// with raw bytes.
///
/// This is used for offsets and records.
pub trait SanitizeStruct: ReadArgs {
    /// If the struct doesn't include offsets, we can just skip it.
    fn can_skip() -> bool {
        false
    }

    /// Sanitize `self`, recursing into any offsets
    fn sanitize_struct(
        &self,
        ctx: &mut SanitizeContext<'_>,
        args: Self::Args,
    ) -> Result<(), ReadError>;
}

/// Recursively sanitize the table pointed at by an offset.
pub trait SanitizeOffset {
    fn sanitize_offset<T: Sanitize>(
        &self,
        ctx: &mut SanitizeContext<'_>,
        args: T::Args,
    ) -> Result<(), ReadError>;
}

impl<O: Offset> SanitizeOffset for O {
    #[must_use]
    fn sanitize_offset<T: Sanitize>(
        &self,
        ctx: &mut SanitizeContext<'_>,
        args: T::Args,
    ) -> Result<(), ReadError> {
        ctx.descend_into_offset(*self, |ctx| T::sanitize(ctx, args))
    }
}

impl<O: Offset> SanitizeOffset for Nullable<O> {
    #[must_use]
    fn sanitize_offset<T: Sanitize>(
        &self,
        ctx: &mut SanitizeContext<'_>,
        args: T::Args,
    ) -> Result<(), ReadError> {
        self.offset().sanitize_offset::<T>(ctx, args)
    }
}

impl<O: SanitizeOffset + Scalar> SanitizeOffset for BigEndian<O> {
    #[must_use]
    fn sanitize_offset<T: Sanitize>(
        &self,
        ctx: &mut SanitizeContext<'_>,
        args: T::Args,
    ) -> Result<(), ReadError> {
        self.get().sanitize_offset::<T>(ctx, args)
    }
}

impl<O: SanitizeOffset> SanitizeOffset for &[O] {
    #[must_use]
    fn sanitize_offset<T: Sanitize>(
        &self,
        ctx: &mut SanitizeContext<'_>,
        args: T::Args,
    ) -> Result<(), ReadError> {
        self.iter()
            .try_for_each(|off| off.sanitize_offset::<T>(ctx, args))
    }
}

#[cfg(test)]
mod tests {
    use types::Offset16;

    use super::*;

    #[test]
    fn verifty_that_various_things_compile() {
        fn sanitize<O: SanitizeOffset>() -> () {
            ()
        }

        sanitize::<Offset16>();
        sanitize::<Nullable<Offset16>>();
        sanitize::<BigEndian<Offset16>>();
        sanitize::<BigEndian<Nullable<Offset16>>>();
        sanitize::<&[BigEndian<Nullable<Offset16>>]>();
        sanitize::<&[BigEndian<Offset16>]>();
        sanitize::<&[Offset16]>();
    }
}
