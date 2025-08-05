//! Handling offsets

use super::read::{FontRead, ReadError};
use crate::{font_data::FontData, read::FontReadWithArgs};
use types::{Nullable, Offset16, Offset24, Offset32};

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

/// A helper trait providing a 'resolve' method for offset types
pub trait ResolveOffset {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Result<T, ReadError>;

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Result<T, ReadError>;
}

/// A helper trait providing a 'resolve' method for nullable offset types
pub trait ResolveNullableOffset {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Option<Result<T, ReadError>>;

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Option<Result<T, ReadError>>;
}

impl<O: Offset> ResolveNullableOffset for Nullable<O> {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Option<Result<T, ReadError>> {
        Some(T::read(data.with_offset(self.offset().non_null()?)))
    }

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Option<Result<T, ReadError>> {
        Some(T::read_with_args(
            data.with_offset(self.offset().non_null()?),
            args,
        ))
    }
}

impl<O: Offset> ResolveOffset for O {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Result<T, ReadError> {
        T::read(data.with_offset(self.to_usize()))
    }

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Result<T, ReadError> {
        T::read_with_args(data.with_offset(self.to_usize()), args)
    }
}
