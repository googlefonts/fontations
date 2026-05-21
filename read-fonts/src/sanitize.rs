//! the traits we'll need to generate for sanitize

use bytemuck::AnyBitPattern;
use types::{BigEndian, FixedSize, Nullable, Scalar};

use crate::{font_data::Cursor, FontData, Offset, ReadArgs, ReadError};

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
    fn new(data: FontData<'a>, state: &'a mut SanitizeState) -> Self {
        Self {
            cursor: data.cursor(),
            state,
        }
    }

    /// Read a scalar and advance the cursor
    #[must_use]
    pub(crate) fn read<T: Scalar>(&mut self) -> Result<T, ReadError> {
        self.cursor.read()
    }

    /// Recursively sanitize an offset, and advance the cursor
    #[must_use]
    pub(crate) fn sanitize_offset<O, T>(&mut self, args: T::Args) -> Result<(), ReadError>
    where
        O: Offset + Scalar,
        T: Sanitize,
    {
        let offset = self.read::<O>()?;
        offset.sanitize_offset::<T>(self, args)
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
        let offset = match self.to_usize() {
            0 => return Ok(()),
            other => other,
        };

        let offset_data = ctx
            .cursor
            .data
            .split_off(offset)
            .ok_or(ReadError::OutOfBounds)?;

        //TODO: track descent here?
        let mut child_ctx = SanitizeContext {
            cursor: offset_data.cursor(),
            state: ctx.state,
        };
        T::sanitize(&mut child_ctx, args)
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
