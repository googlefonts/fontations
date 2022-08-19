//! the [GPOS] table
//!
//! [GPOS]: https://docs.microsoft.com/en-us/typography/opentype/spec/gpos

#[path = "./value_record.rs"]
mod value_record;

#[cfg(feature = "traversal")]
use std::ops::Deref;

use crate::array::ComputedArray;

/// reexport stuff from layout that we use
pub use super::{
    ChainedSequenceContext, ClassDef, CoverageTable, Device, FeatureList, FeatureVariations,
    Lookup, LookupList, ScriptList, SequenceContext, TypedLookup,
};
pub use value_record::ValueRecord;

#[cfg(test)]
#[path = "../tests/gpos.rs"]
mod tests;

/// 'GPOS'
pub const TAG: Tag = Tag::new(b"GPOS");

include!("../../generated/generated_gpos.rs");

/// A typed GPOS LookupList table
pub struct PositionLookupList<'a>(LookupList<'a>);

/// A typed GPOS Lookup table
pub enum PositionLookup<'a> {
    Single(TypedLookup<'a, SinglePos<'a>>),
    Pair(TypedLookup<'a, PairPos<'a>>),
    Cursive(TypedLookup<'a, CursivePosFormat1<'a>>),
    MarkToBase(TypedLookup<'a, MarkBasePosFormat1<'a>>),
    MarkToMark(TypedLookup<'a, MarkMarkPosFormat1<'a>>),
    MarkToLig(TypedLookup<'a, MarkLigPosFormat1<'a>>),
    Contextual(TypedLookup<'a, SequenceContext<'a>>),
    ChainContextual(TypedLookup<'a, ChainedSequenceContext<'a>>),
    Extension(TypedLookup<'a, ExtensionSubtable<'a>>),
}

/// A typed extension subtable
//TODO: would be very nice to have codegen for this pattern...
pub enum ExtensionSubtable<'a> {
    Single(TypedExtension<'a, SinglePos<'a>>),
    Pair(TypedExtension<'a, PairPos<'a>>),
    Cursive(TypedExtension<'a, CursivePosFormat1<'a>>),
    MarkToBase(TypedExtension<'a, MarkBasePosFormat1<'a>>),
    MarkToLig(TypedExtension<'a, MarkLigPosFormat1<'a>>),
    MarkToMark(TypedExtension<'a, MarkMarkPosFormat1<'a>>),
    Contextual(TypedExtension<'a, SequenceContext<'a>>),
    ChainContextual(TypedExtension<'a, ChainedSequenceContext<'a>>),
}

/// A typed position extension table.
///
/// This is a way of associating generic type information.
pub struct TypedExtension<'a, T> {
    inner: ExtensionPosFormat1<'a>,
    phantom: std::marker::PhantomData<T>,
}

impl<'a, T: FontRead<'a>> TypedExtension<'a, T> {
    fn new(inner: ExtensionPosFormat1<'a>) -> Self {
        TypedExtension {
            inner,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn get(&self) -> Result<T, ReadError> {
        self.inner.extension_offset().resolve(self.inner.data)
    }
}

impl<'a, T> std::ops::Deref for TypedExtension<'a, T> {
    type Target = ExtensionPosFormat1<'a>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> PositionLookupList<'a> {
    pub fn lookups(&self) -> impl Iterator<Item = Result<PositionLookup<'a>, ReadError>> + 'a {
        let data = self.data;
        self.0
            .lookup_offsets()
            .iter()
            .map(move |off| off.get().resolve(data))
    }
}

impl<'a> FontRead<'a> for PositionLookup<'a> {
    fn read(bytes: FontData<'a>) -> Result<Self, ReadError> {
        let lookup = Lookup::read(bytes)?;
        match lookup.lookup_type() {
            1 => Ok(PositionLookup::Single(TypedLookup::new(lookup))),
            2 => Ok(PositionLookup::Pair(TypedLookup::new(lookup))),
            3 => Ok(PositionLookup::Cursive(TypedLookup::new(lookup))),
            4 => Ok(PositionLookup::MarkToBase(TypedLookup::new(lookup))),
            5 => Ok(PositionLookup::MarkToLig(TypedLookup::new(lookup))),
            6 => Ok(PositionLookup::MarkToMark(TypedLookup::new(lookup))),
            7 => Ok(PositionLookup::Contextual(TypedLookup::new(lookup))),
            8 => Ok(PositionLookup::ChainContextual(TypedLookup::new(lookup))),
            9 => Ok(PositionLookup::Extension(TypedLookup::new(lookup))),
            other => Err(ReadError::InvalidFormat(other.into())),
        }
    }
}

impl<'a> std::ops::Deref for PositionLookup<'a> {
    type Target = Lookup<'a>;
    fn deref(&self) -> &Self::Target {
        match self {
            PositionLookup::Single(table) => table,
            PositionLookup::Pair(table) => table,
            PositionLookup::Cursive(table) => table,
            PositionLookup::MarkToBase(table) => table,
            PositionLookup::MarkToMark(table) => table,
            PositionLookup::MarkToLig(table) => table,
            PositionLookup::Contextual(table) => table,
            PositionLookup::ChainContextual(table) => table,
            PositionLookup::Extension(table) => table,
        }
    }
}

impl<'a> std::ops::Deref for PositionLookupList<'a> {
    type Target = LookupList<'a>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> FontRead<'a> for PositionLookupList<'a> {
    fn read(bytes: FontData<'a>) -> Result<Self, ReadError> {
        LookupList::read(bytes).map(Self)
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for PositionLookupList<'a> {
    fn type_name(&self) -> &str {
        self.deref().type_name()
    }

    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        let this = PositionLookupList(self.0.sneaky_copy());
        match idx {
            0 => Some(Field::new("lookup_count", self.lookup_count())),
            1 => Some(Field::new(
                "lookup_offsets",
                FieldType::offset_iter(move || {
                    Box::new(this.lookups().map(|item| item.into()))
                        as Box<dyn Iterator<Item = FieldType<'a>> + 'a>
                }),
            )),
            _ => None,
        }
    }
}

#[cfg(feature = "traversal")]
impl<'a> SomeTable<'a> for PositionLookup<'a> {
    fn get_field(&self, idx: usize) -> Option<Field<'a>> {
        match self {
            PositionLookup::Single(table) => table.get_field(idx),
            PositionLookup::Pair(table) => table.get_field(idx),
            PositionLookup::Cursive(table) => table.get_field(idx),
            PositionLookup::MarkToBase(table) => table.get_field(idx),
            PositionLookup::MarkToMark(table) => table.get_field(idx),
            PositionLookup::MarkToLig(table) => table.get_field(idx),
            PositionLookup::Contextual(table) => table.get_field(idx),
            PositionLookup::ChainContextual(table) => table.get_field(idx),
            PositionLookup::Extension(table) => table.get_field(idx),
        }
    }

    fn type_name(&self) -> &str {
        "Lookup"
    }
}

impl<'a> FontRead<'a> for ExtensionSubtable<'a> {
    fn read(data: FontData<'a>) -> Result<Self, ReadError> {
        let extension = ExtensionPosFormat1::read(data)?;
        match extension.extension_lookup_type() {
            1 => Ok(ExtensionSubtable::Single(TypedExtension::new(extension))),
            2 => Ok(ExtensionSubtable::Pair(TypedExtension::new(extension))),
            3 => Ok(ExtensionSubtable::Cursive(TypedExtension::new(extension))),
            4 => Ok(ExtensionSubtable::MarkToBase(TypedExtension::new(
                extension,
            ))),
            5 => Ok(ExtensionSubtable::MarkToMark(TypedExtension::new(
                extension,
            ))),
            6 => Ok(ExtensionSubtable::MarkToLig(TypedExtension::new(extension))),
            7 => Ok(ExtensionSubtable::Contextual(TypedExtension::new(
                extension,
            ))),
            8 => Ok(ExtensionSubtable::ChainContextual(TypedExtension::new(
                extension,
            ))),
            other => Err(ReadError::InvalidFormat(other.into())),
        }
    }
}
