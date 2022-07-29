//! Typed font tables

use super::read::{FontRead, Format, ReadError};
use crate::{
    font_data::FontData,
    read::{FontReadWithArgs, ReadArgs},
};
use font_types::Offset;

/// Typed access to raw table data.
pub struct TableRef<'a, T> {
    pub(crate) shape: T,
    pub(crate) data: FontData<'a>,
}

/// A trait for types that describe the structure of a specific font table.
///
/// Instances of this type are constructed from the *specific* data of a particular
/// instance of a given table, and the successful construction of the type acts
/// as a validation of the input data.
///
/// In particular, the info type records the lengths of variable length fields,
/// the existence of version-dependent fields, and anything else that varies
/// between instances of a given table.
///
/// These stored values can be used at runtime to provide fast access to a table's
/// fields, without needing to perform redundant bounds checks.
pub trait TableInfo: Sized + Copy {
    fn parse<'a>(data: FontData<'a>) -> Result<TableRef<'a, Self>, ReadError>;
}

/// A trait for types that describe the structure of a font table, but require
/// additional information.
pub trait TableInfoWithArgs: Sized + Copy + ReadArgs {
    fn parse_with_args<'a>(
        data: FontData<'a>,
        args: &Self::Args,
    ) -> Result<TableRef<'a, Self>, ReadError>;
}

impl<'a, T> TableRef<'a, T> {
    /// Resolve the provided offset from the start of this table.
    pub fn resolve_offset<O: Offset, R: FontRead<'a>>(&self, offset: O) -> Result<R, ReadError> {
        offset.resolve(&self.data)
    }
}

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

// a blanket impl so that the format is available through a TableRef
impl<U, T: TableInfo + Format<U>> Format<U> for TableRef<'_, T> {
    const FORMAT: U = T::FORMAT;
}

// blanket impl of FontRead for any TableRef
impl<'a, T: TableInfo> FontRead<'a> for TableRef<'a, T> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        T::parse(data)
    }
}

impl<'a, T: ReadArgs> ReadArgs for TableRef<'a, T> {
    type Args = T::Args;
}

impl<'a, T: TableInfoWithArgs> FontReadWithArgs<'a> for TableRef<'a, T> {
    fn read_with_args(data: FontData<'a>, args: &Self::Args) -> Result<Self, ReadError> {
        T::parse_with_args(data, args)
    }
}
