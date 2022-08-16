//! Traits for converting from parsed font data to their compile equivalents

use read_fonts::parse_prelude::FontData;

/// A trait for types that can fully resolve themselves.
///
/// This means that any offsets held in this type are resolved relative to the
/// start of the table itself (and not some parent table)
pub trait FromTableRef<T>: FromObjRef<T> {
    fn from_table_ref(from: &T) -> Self {
        let data = FontData::new(&[]);
        Self::from_obj_ref(from, &data)
    }
}

/// A trait for types that can resolve themselves when provided data to resolve offsets.
///
/// It is possible that the generated object is malformed; for instance offsets
/// may be null where it is not allowed. This can be checked by calling [`validate`][]
/// on the generated object.
///
/// This is implemented for the majority of parse types. Those that are the base
/// for offset data the provided data and use their own.
///
/// [`validate`]: [crate::Validate::validate]
pub trait FromObjRef<T: ?Sized>: Sized {
    //type Owned;
    fn from_obj_ref(from: &T, data: &FontData) -> Self;
}

/// A conversion from a parsed font object type to an owned version, resolving
/// offsets.
///
/// You should avoid implementing this trait manually. Like [`std::convert::Into`],
/// it is provided as a blanket impl when you implement [`FromObjRef<T>`].
pub trait ToOwnedObj<T> {
    fn to_owned_obj(&self, data: &FontData) -> T;
}

/// A conversion from a fully resolveable parsed font table to its owned equivalent.
///
/// As with [`ToOwnedObj`], you should not need to implement this manually.
pub trait ToOwnedTable<T>: ToOwnedObj<T> {
    fn to_owned_table(&self) -> T;
}

impl<U, T> ToOwnedObj<U> for T
where
    U: FromObjRef<T>,
{
    fn to_owned_obj(&self, data: &FontData) -> U {
        U::from_obj_ref(self, data)
    }
}

impl<U, T> ToOwnedTable<U> for T
where
    U: FromTableRef<T>,
{
    fn to_owned_table(&self) -> U {
        U::from_table_ref(self)
    }
}
