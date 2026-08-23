//! Arrays of offsets with dynamic resolution
//!
//! This module provides a number of types that wrap arrays of offsets, dynamically
//! resolving individual offsets as they are accessed.

use crate::offset::ResolveNullableOffset;
#[cfg(any(test, feature = "codegen_test"))]
use crate::sanitize::{FastResolveNullableOffset, FastResolveOffset, Sanitize};
use font_types::{BigEndian, Nullable, Offset16, Scalar};

use crate::{FontData, FontRead, Offset, ReadArgs, ReadError, ResolveOffset};

/// An array of offsets that can be resolved on access.
///
/// This bundles up the raw offsets with the data used to resolve them, along
/// with any arguments needed to resolve those offsets; it provides a simple
/// ergonomic interface that unburdens the user from needing to manually
/// determine the appropriate input data and arguments for a raw offset.
#[derive(Clone)]
pub struct ArrayOfOffsets<'a, T: ReadArgs, O: Scalar = Offset16> {
    offsets: &'a [BigEndian<O>],
    data: FontData<'a>,
    args: T::Args,
}

/// An array of nullable offsets that can be resolved on access.
///
/// This is identical to [`ArrayOfOffsets`], except that each offset is
/// allowed to be null.
#[derive(Clone)]
pub struct ArrayOfNullableOffsets<'a, T: ReadArgs, O: Scalar = Offset16> {
    offsets: &'a [BigEndian<Nullable<O>>],
    data: FontData<'a>,
    args: T::Args,
}

impl<'a, T, O> ArrayOfOffsets<'a, T, O>
where
    O: Scalar,
    T: ReadArgs,
{
    pub(crate) fn new(offsets: &'a [BigEndian<O>], data: FontData<'a>, args: T::Args) -> Self {
        Self {
            offsets,
            data,
            args,
        }
    }
}

impl<'a, T, O> ArrayOfOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: FontRead<'a>,
    T::Args: Copy + 'static,
{
    /// The number of offsets in the array
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `true` if the array is empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Resolve the offset at the provided index.
    ///
    /// Note: if the index is invalid this will return the `InvalidCollectionIndex`
    /// error variant instead of `None`.
    pub fn get(&self, idx: usize) -> Result<T, ReadError> {
        self.offsets
            .get(idx)
            .ok_or(ReadError::InvalidCollectionIndex(idx as _))
            .and_then(|o| o.get().resolve_with_args(self.data, self.args))
    }

    /// Iterate over all of the offset targets.
    ///
    /// Each offset will be resolved as it is encountered.
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let mut iter = self.offsets.iter();
        let args = self.args;
        let data = self.data;
        std::iter::from_fn(move || {
            iter.next()
                .map(|off| off.get().resolve_with_args(data, args))
        })
    }

    /// Iterate over all of the offset targets.
    ///
    /// Offset is treated as nullable and each offset will be resolved as it is encountered.
    pub(crate) fn iter_as_nullable(
        &self,
    ) -> impl Iterator<Item = Option<Result<T, ReadError>>> + 'a {
        self.iter().map(|off| match off {
            Err(ReadError::NullOffset) => None,
            other => Some(other),
        })
    }
}

impl<'a, T, O> ArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadArgs,
{
    pub(crate) fn new(
        offsets: &'a [BigEndian<Nullable<O>>],
        data: FontData<'a>,
        args: T::Args,
    ) -> Self {
        Self {
            offsets,
            data,
            args,
        }
    }
}

impl<'a, T, O> ArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: FontRead<'a>,
    T::Args: Copy + 'static,
{
    /// The number of offsets in the array
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `true` if the array is empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Resolve the offset at the provided index.
    ///
    /// This will return `None` only if the offset *exists*, but is null. if the
    /// provided index does not exist, this will return the `InvalidCollectionIndex`
    /// error variant.
    pub fn get(&self, idx: usize) -> Option<Result<T, ReadError>> {
        let Some(offset) = self.offsets.get(idx) else {
            return Some(Err(ReadError::InvalidCollectionIndex(idx as _)));
        };
        offset.get().resolve_with_args(self.data, self.args)
    }

    /// Iterate over all of the offset targets.
    ///
    /// Each offset will be resolved as it is encountered.
    pub fn iter(&self) -> impl Iterator<Item = Option<Result<T, ReadError>>> + 'a {
        let mut iter = self.offsets.iter();
        let args = self.args;
        let data = self.data;
        std::iter::from_fn(move || {
            iter.next()
                .map(|off| off.get().resolve_with_args(data, args))
        })
    }
}

/// An array of offsets that resolves using `read_fast` (post-sanitize).
///
/// This is identical to [`ArrayOfOffsets`], except that each offset is resolved
/// using `fast_resolve` instead of `resolve_with_args`, skipping re-validation.
#[cfg(any(test, feature = "codegen_test"))]
#[derive(Clone)]
pub struct SanitizedArrayOfOffsets<'a, T: ReadArgs, O: Scalar = Offset16> {
    offsets: &'a [BigEndian<O>],
    data: FontData<'a>,
    args: T::Args,
}

/// An array of nullable offsets that resolves using `read_fast` (post-sanitize).
///
/// This is identical to [`ArrayOfNullableOffsets`], except that each offset is
/// resolved using `fast_resolve` instead of `resolve_with_args`, skipping
/// re-validation.
#[cfg(any(test, feature = "codegen_test"))]
#[derive(Clone)]
pub struct SanitizedArrayOfNullableOffsets<'a, T: ReadArgs, O: Scalar = Offset16> {
    offsets: &'a [BigEndian<Nullable<O>>],
    data: FontData<'a>,
    args: T::Args,
}

#[cfg(any(test, feature = "codegen_test"))]
impl<'a, T, O> SanitizedArrayOfOffsets<'a, T, O>
where
    O: Scalar,
    T: ReadArgs,
{
    pub(crate) fn new(offsets: &'a [BigEndian<O>], data: FontData<'a>, args: T::Args) -> Self {
        Self {
            offsets,
            data,
            args,
        }
    }
}

#[cfg(any(test, feature = "codegen_test"))]
impl<'a, T, O> SanitizedArrayOfOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadArgs + Sanitize<'a> + Default,
    T::Args: Copy + 'static,
{
    /// The number of offsets in the array
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `true` if the array is empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Resolve the offset at the provided index.
    ///
    /// Note: if the index is invalid this will return the `InvalidCollectionIndex`
    /// error variant instead of `None`.
    pub fn get(&self, idx: usize) -> Result<T, ReadError> {
        self.offsets
            .get(idx)
            .ok_or(ReadError::InvalidCollectionIndex(idx as _))
            .and_then(|o| o.get().fast_resolve(self.data, self.args))
    }

    /// Iterate over all of the offset targets.
    ///
    /// Each offset will be resolved as it is encountered.
    pub fn iter(&self) -> impl Iterator<Item = Result<T, ReadError>> + 'a {
        let mut iter = self.offsets.iter();
        let args = self.args;
        let data = self.data;
        std::iter::from_fn(move || iter.next().map(|off| off.get().fast_resolve(data, args)))
    }
}

#[cfg(any(test, feature = "codegen_test"))]
impl<'a, T, O> SanitizedArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadArgs,
{
    pub(crate) fn new(
        offsets: &'a [BigEndian<Nullable<O>>],
        data: FontData<'a>,
        args: T::Args,
    ) -> Self {
        Self {
            offsets,
            data,
            args,
        }
    }
}

#[cfg(any(test, feature = "codegen_test"))]
impl<'a, T, O> SanitizedArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset,
    T: ReadArgs + Sanitize<'a> + Default,
    T::Args: Copy + 'static,
{
    /// The number of offsets in the array
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// `true` if the array is empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Resolve the offset at the provided index.
    ///
    /// This will return `None` only if the offset *exists*, but is null. if the
    /// provided index does not exist, this will return the `InvalidCollectionIndex`
    /// error variant.
    pub fn get(&self, idx: usize) -> Option<Result<T, ReadError>> {
        let Some(offset) = self.offsets.get(idx) else {
            return Some(Err(ReadError::InvalidCollectionIndex(idx as _)));
        };
        offset.get().fast_resolve(self.data, self.args)
    }

    /// Iterate over all of the offset targets.
    ///
    /// Each offset will be resolved as it is encountered.
    pub fn iter(&self) -> impl Iterator<Item = Option<Result<T, ReadError>>> + 'a {
        let mut iter = self.offsets.iter();
        let args = self.args;
        let data = self.data;
        std::iter::from_fn(move || iter.next().map(|off| off.get().fast_resolve(data, args)))
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a, T, O> crate::traversal::SomeArray<'a> for ArrayOfOffsets<'a, T, O>
where
    O: Scalar + Offset + Into<crate::traversal::OffsetType>,
    T: FontRead<'a> + crate::traversal::SomeTable<'a> + 'a,
    T::Args: Copy + 'static,
{
    fn type_name(&self) -> &str {
        let full_name = std::any::type_name::<T>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }

    fn get(&self, idx: usize) -> Option<crate::traversal::FieldType<'a>> {
        let off = self.offsets.get(idx)?;
        let raw = off.get();
        let result = raw.resolve_with_args::<T>(self.data, self.args);
        Some(crate::traversal::FieldType::offset(raw, result))
    }
}

#[cfg(feature = "experimental_traverse")]
impl<'a, T, O> crate::traversal::SomeArray<'a> for ArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset + Into<crate::traversal::OffsetType> + Clone,
    T: FontRead<'a> + crate::traversal::SomeTable<'a> + 'a,
    T::Args: Copy + 'static,
{
    fn type_name(&self) -> &str {
        let full_name = std::any::type_name::<T>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }

    fn get(&self, idx: usize) -> Option<crate::traversal::FieldType<'a>> {
        let off = self.offsets.get(idx)?;
        let raw = off.get();
        let result = raw.resolve_with_args::<T>(self.data, self.args);
        Some(crate::traversal::FieldType::offset(raw, result))
    }
}

#[cfg(all(feature = "experimental_traverse", any(test, feature = "codegen_test")))]
impl<'a, T, O> crate::traversal::SomeArray<'a> for SanitizedArrayOfOffsets<'a, T, O>
where
    O: Scalar + Offset + Into<crate::traversal::OffsetType>,
    T: ReadArgs + Sanitize<'a> + Default + crate::traversal::SomeTable<'a> + 'a,
    T::Args: Copy + 'static,
{
    fn type_name(&self) -> &str {
        let full_name = std::any::type_name::<T>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }

    fn get(&self, idx: usize) -> Option<crate::traversal::FieldType<'a>> {
        let off = self.offsets.get(idx)?;
        let raw = off.get();
        let result = raw.fast_resolve::<T>(self.data, self.args);
        Some(crate::traversal::FieldType::offset(raw, result))
    }
}

#[cfg(all(feature = "experimental_traverse", any(test, feature = "codegen_test")))]
impl<'a, T, O> crate::traversal::SomeArray<'a> for SanitizedArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset + Into<crate::traversal::OffsetType> + Clone,
    T: ReadArgs + Sanitize<'a> + Default + crate::traversal::SomeTable<'a> + 'a,
    T::Args: Copy + 'static,
{
    fn type_name(&self) -> &str {
        let full_name = std::any::type_name::<T>();
        full_name.split("::").last().unwrap_or(full_name)
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }

    fn get(&self, idx: usize) -> Option<crate::traversal::FieldType<'a>> {
        let off = self.offsets.get(idx)?;
        let raw = off.get();
        let result = raw.fast_resolve::<T>(self.data, self.args);
        Some(crate::traversal::FieldType::offset(raw, result))
    }
}
