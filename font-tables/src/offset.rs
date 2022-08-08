//! Handling offsets

use super::read::{FontRead, ReadError};
use crate::{font_data::FontData, read::FontReadWithArgs};
use font_types::{Offset16, Offset24, Offset32};

/// Any offset type.
pub trait Offset: Copy {
    fn to_usize(self) -> usize;

    fn non_null(self) -> Option<usize> {
        match self.to_usize() {
            0 => None,
            other => Some(other),
        }
    }
}

macro_rules! impl_offset {
    ($name:ident, $width:literal) => {
        impl Offset for $name {
            #[inline]
            fn to_usize(self) -> usize {
                self.to_u32() as _
            }
        }
    };
}

impl_offset!(Offset16, 2);
impl_offset!(Offset24, 3);
impl_offset!(Offset32, 4);

/// a (temporary?) helper trait to blanket impl a resolve method for font_types::Offset
pub trait ResolveOffset {
    fn resolve<'a, T: FontRead<'a>>(&self, data: &FontData<'a>) -> Result<T, ReadError>;

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: &FontData<'a>,
        args: &T::Args,
    ) -> Result<T, ReadError>;

    fn resolve_nullable<'a, T: FontRead<'a>>(
        &self,
        data: &FontData<'a>,
    ) -> Option<Result<T, ReadError>>;

    fn resolve_nullable_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: &FontData<'a>,
        args: &T::Args,
    ) -> Option<Result<T, ReadError>>;
}

impl<O: Offset> ResolveOffset for O {
    fn resolve<'a, T: FontRead<'a>>(&self, data: &FontData<'a>) -> Result<T, ReadError> {
        match self.resolve_nullable(data) {
            Some(x) => x,
            None => Err(ReadError::NullOffset),
        }
    }

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: &FontData<'a>,
        args: &T::Args,
    ) -> Result<T, ReadError> {
        match self.resolve_nullable_with_args(data, args) {
            Some(x) => x,
            None => Err(ReadError::NullOffset),
        }
    }

    fn resolve_nullable<'a, T: FontRead<'a>>(
        &self,
        data: &FontData<'a>,
    ) -> Option<Result<T, ReadError>> {
        let non_null = self.non_null()?;
        Some(
            data.split_off(non_null)
                .ok_or(ReadError::OutOfBounds)
                .and_then(T::read),
        )
    }

    fn resolve_nullable_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: &FontData<'a>,
        args: &T::Args,
    ) -> Option<Result<T, ReadError>> {
        let non_null = self.non_null()?;
        Some(
            data.split_off(non_null)
                .ok_or(ReadError::OutOfBounds)
                .and_then(|data| T::read_with_args(data, args)),
        )
    }
}
