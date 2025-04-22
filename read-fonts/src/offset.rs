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

/// A trait for tables that are used as the base for resolving offsets.
///
/// By convention, when an offset exists in a table, the offset is resolved
/// relative to the start of that table. When offsets exist in records, however,
/// they are _generally_ resolved relative to the start of the parent table.
///
/// In this case, we can't generate a typed getter for the offset, since we do
/// not (from the record) have access to the parent table. Instead we generate
/// a getter that requires the user to pass in the data explicitly.
///
/// This can be confusing, so in these cases we use this trait to encode the
/// expected type of the offset data. Thus we can do,
///
/// ```no_run
///  use read_fonts::{OffsetSource, tables::gpos::Gpos};
///  # fn get_gpos() -> read_fonts::tables::gpos::Gpos<'static> { todo!() }
///
///  let gpos: Gpos = get_gpos();
///  let script_list = gpos.script_list().unwrap();
///  let first_record = script_list.script_records()[0];
///  // the argument type is `impl OffsetSource<ScriptList>`, so we pass in a
///  // reference to the `ScriptList` table.
///  let script = first_record.script(&script_list).unwrap();
/// ```
pub trait OffsetSource<'a, T> {
    fn offset_source(&self) -> FontData<'a>;
}

/// so that old code still works, implement this for `FontData` itself.
impl<'a, T> OffsetSource<'a, T> for FontData<'a> {
    fn offset_source(&self) -> FontData<'a> {
        *self
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
        match self.offset().resolve(data) {
            Ok(thing) => Some(Ok(thing)),
            Err(ReadError::NullOffset) => None,
            Err(e) => Some(Err(e)),
        }
    }

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Option<Result<T, ReadError>> {
        match self.offset().resolve_with_args(data, args) {
            Ok(thing) => Some(Ok(thing)),
            Err(ReadError::NullOffset) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl<O: Offset> ResolveOffset for O {
    fn resolve<'a, T: FontRead<'a>>(&self, data: FontData<'a>) -> Result<T, ReadError> {
        self.non_null()
            .ok_or(ReadError::NullOffset)
            .and_then(|off| data.split_off(off).ok_or(ReadError::OutOfBounds))
            .and_then(T::read)
    }

    fn resolve_with_args<'a, T: FontReadWithArgs<'a>>(
        &self,
        data: FontData<'a>,
        args: &T::Args,
    ) -> Result<T, ReadError> {
        self.non_null()
            .ok_or(ReadError::NullOffset)
            .and_then(|off| data.split_off(off).ok_or(ReadError::OutOfBounds))
            .and_then(|data| T::read_with_args(data, args))
    }
}
