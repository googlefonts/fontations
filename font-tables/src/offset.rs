//! Handling offsets

use super::read::{FontRead, ReadError};
use crate::{font_data::FontData, read::FontReadWithArgs};
use font_types::Offset;

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
