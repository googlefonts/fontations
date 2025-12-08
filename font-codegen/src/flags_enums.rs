//! codegen for bitflags and raw enums

use proc_macro2::TokenStream;
use quote::quote;

use super::parsing::{BitFlags, RawEnum};

pub(crate) fn generate_flags(raw: &BitFlags) -> proc_macro2::TokenStream {
    let name = &raw.name;
    let docs = &raw.docs;
    let typ = &raw.typ;
    let variant_decls = raw.variants.iter().map(|variant| {
        let const_name = &variant.name;
        let value = &variant.value;
        let docs = &variant.attrs.docs;
        quote! {
            #( #docs )*
            pub const #const_name: Self = Self { bits: #value };
        }
    });

    let all_names = raw.variants.iter().map(|var| var.name.to_string());
    let all_values = raw.variants.iter().map(|var| &var.name).collect::<Vec<_>>();

    quote! {
        #( #docs )*
        #[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, bytemuck_derive::AnyBitPattern)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[repr(transparent)]
        pub struct #name { bits: #typ }
        impl #name {
            #( #variant_decls )*
        }

        // most of this impl is taken from the bitflags crate, under the MIT/Apache license
        // https://docs.rs/bitflags/latest/bitflags/
        impl #name {
            ///  Returns an empty set of flags.
            #[inline]
            pub const fn empty() -> Self {
                Self { bits: 0 }
            }

            /// Returns the set containing all flags.
            #[inline]
            pub const fn all() -> Self {
                Self { bits: #( Self::#all_values.bits )|* }
            }

            /// Returns the raw value of the flags currently stored.
            #[inline]
            pub const fn bits(&self) -> #typ {
                self.bits
            }

            /// Convert from underlying bit representation, unless that
            /// representation contains bits that do not correspond to a flag.
            #[inline]
            pub const fn from_bits(bits: #typ) -> Option<Self> {
                if (bits & !Self::all().bits()) == 0 {
                    Some(Self { bits })
                } else {
                    None
                }
            }

            /// Convert from underlying bit representation, dropping any bits
            /// that do not correspond to flags.
            #[inline]
            pub const fn from_bits_truncate(bits: #typ) -> Self {
                Self { bits: bits & Self::all().bits }
            }

             /// Returns `true` if no flags are currently stored.
            #[inline]
            pub const fn is_empty(&self) -> bool {
                self.bits() == Self::empty().bits()
            }

            /// Returns `true` if there are flags common to both `self` and `other`.
            #[inline]
            pub const fn intersects(&self, other: Self) -> bool {
                !(Self { bits: self.bits & other.bits}).is_empty()
            }

            /// Returns `true` if all of the flags in `other` are contained within `self`.
            #[inline]
            pub const fn contains(&self, other: Self) -> bool {
                (self.bits & other.bits) == other.bits
            }

            /// Inserts the specified flags in-place.
            #[inline]
            pub fn insert(&mut self, other: Self) {
                self.bits |= other.bits;
            }

            /// Removes the specified flags in-place.
            #[inline]
            pub fn remove(&mut self, other: Self) {
                self.bits &= !other.bits;
            }

            /// Toggles the specified flags in-place.
            #[inline]
            pub fn toggle(&mut self, other: Self) {
                self.bits ^= other.bits;
            }

            /// Returns the intersection between the flags in `self` and
            /// `other`.
            ///
            /// Specifically, the returned set contains only the flags which are
            /// present in *both* `self` *and* `other`.
            ///
            /// This is equivalent to using the `&` operator (e.g.
            /// [`ops::BitAnd`]), as in `flags & other`.
            ///
            /// [`ops::BitAnd`]: https://doc.rust-lang.org/std/ops/trait.BitAnd.html
            #[inline]
            #[must_use]
            pub const fn intersection(self, other: Self) -> Self {
                Self { bits: self.bits & other.bits }
            }

            /// Returns the union of between the flags in `self` and `other`.
            ///
            /// Specifically, the returned set contains all flags which are
            /// present in *either* `self` *or* `other`, including any which are
            /// present in both.
            ///
            /// This is equivalent to using the `|` operator (e.g.
            /// [`ops::BitOr`]), as in `flags | other`.
            ///
            /// [`ops::BitOr`]: https://doc.rust-lang.org/std/ops/trait.BitOr.html
            #[inline]
            #[must_use]
            pub const fn union(self, other: Self) -> Self {
                Self { bits: self.bits | other.bits }
            }

            /// Returns the difference between the flags in `self` and `other`.
            ///
            /// Specifically, the returned set contains all flags present in
            /// `self`, except for the ones present in `other`.
            ///
            /// It is also conceptually equivalent to the "bit-clear" operation:
            /// `flags & !other` (and this syntax is also supported).
            ///
            /// This is equivalent to using the `-` operator (e.g.
            /// [`ops::Sub`]), as in `flags - other`.
            ///
            /// [`ops::Sub`]: https://doc.rust-lang.org/std/ops/trait.Sub.html
            #[inline]
            #[must_use]
            pub const fn difference(self, other: Self) -> Self {
                Self { bits: self.bits & !other.bits }
            }
        }

        impl std::ops::BitOr for #name {
            type Output = Self;

            /// Returns the union of the two sets of flags.
            #[inline]
            fn bitor(self, other: #name) -> Self {
                Self { bits: self.bits | other.bits }
            }
        }

        impl std::ops::BitOrAssign for #name {
            /// Adds the set of flags.
            #[inline]
            fn bitor_assign(&mut self, other: Self) {
                self.bits |= other.bits;
            }
        }

        impl std::ops::BitXor for #name {
            type Output = Self;

            /// Returns the left flags, but with all the right flags toggled.
            #[inline]
            fn bitxor(self, other: Self) -> Self {
                Self { bits: self.bits ^ other.bits }
            }
        }

        impl std::ops::BitXorAssign for #name {
            /// Toggles the set of flags.
            #[inline]
            fn bitxor_assign(&mut self, other: Self) {
                self.bits ^= other.bits;
            }
        }

        impl std::ops::BitAnd for #name {
            type Output = Self;

            /// Returns the intersection between the two sets of flags.
            #[inline]
            fn bitand(self, other: Self) -> Self {
                Self { bits: self.bits & other.bits }
            }
        }

        impl std::ops::BitAndAssign for #name {
            /// Disables all flags disabled in the set.
            #[inline]
            fn bitand_assign(&mut self, other: Self) {
                self.bits &= other.bits;
            }
        }

        impl std::ops::Sub for #name {
            type Output = Self;

            /// Returns the set difference of the two sets of flags.
            #[inline]
            fn sub(self, other: Self) -> Self {
                Self { bits: self.bits & !other.bits }
            }
        }

        impl std::ops::SubAssign for #name {
            /// Disables all flags enabled in the set.
            #[inline]
            fn sub_assign(&mut self, other: Self) {
                self.bits &= !other.bits;
            }
        }

        impl std::ops::Not for #name {
            type Output = Self;

            /// Returns the complement of this set of flags.
            #[inline]
            fn not(self) -> Self {
                Self { bits: !self.bits } & Self::all()
            }
        }

        impl std::fmt::Debug for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let members: &[(&str, Self)] = &[#( (#all_names, Self::#all_values ), )*];
                let mut first = true;
                for (name, value) in members {
                    if self.contains(*value) {
                        if !first {
                            f.write_str(" | ")?;
                        }
                        first = false;
                        f.write_str(name)?;
                    }
                }
                if first {
                    f.write_str("(empty)")?;
                }
                Ok(())
            }
        }

        impl std::fmt::Binary for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                std::fmt::Binary::fmt(&self.bits, f)
            }
        }
        impl std::fmt::Octal for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                std::fmt::Octal::fmt(&self.bits, f)
            }
        }
        impl std::fmt::LowerHex for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                std::fmt::LowerHex::fmt(&self.bits, f)
            }
        }
        impl std::fmt::UpperHex for #name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                std::fmt::UpperHex::fmt(&self.bits, f)
            }
        }

        impl font_types::Scalar for #name {
            type Raw = <#typ as font_types::Scalar>::Raw;

            fn to_raw(self) -> Self::Raw {
                self.bits().to_raw()
            }

            fn from_raw(raw: Self::Raw) -> Self {
                let t = <#typ>::from_raw(raw);
                Self::from_bits_truncate(t)
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl<'a> From<#name> for FieldType<'a> {
            fn from(src: #name) -> FieldType<'a> {
                src.bits().into()
            }
        }
    }
}

pub(crate) fn generate_flags_compile(raw: &BitFlags) -> TokenStream {
    // we reuse the type from the read-fonts crate, and so only implement our trait.

    let name = &raw.name;
    quote! {
        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                writer.write_slice(&self.bits().to_be_bytes())
            }
        }
    }
}

pub(crate) fn generate_raw_enum(raw: &RawEnum) -> TokenStream {
    let name = &raw.name;
    let docs = &raw.docs;
    let typ = &raw.typ;
    let variants = raw.variants.iter().map(|variant| {
        let name = &variant.name;
        let value = &variant.value;
        let docs = &variant.attrs.docs;
        let maybe_default = variant.attrs.default.as_ref().map(|_| quote!(#[default]));
        quote! {
            #( #docs )*
            #maybe_default
            #name = #value,
        }
    });

    let variant_inits = raw.variants.iter().map(|variant| {
        let name = &variant.name;
        let value = &variant.value;
        quote!(#value => Self::#name,)
    });

    let docstring = " If font data is malformed we will map unknown values to this variant";
    quote! {
        #( #docs )*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[repr(#typ)]
        #[allow(clippy::manual_non_exhaustive)]
        pub enum #name {
            #( #variants )*
            #[doc(hidden)]
            #[doc = #docstring]
            Unknown,
        }

        impl #name {
            /// Create from a raw scalar.
            ///
            /// This will never fail; unknown values will be mapped to the `Unknown` variant
            pub fn new(raw: #typ) -> Self {
                match raw {
                    #( #variant_inits )*
                    _ => Self::Unknown,
                }
            }
        }

        impl font_types::Scalar for #name {
            type Raw = <#typ as font_types::Scalar>::Raw;

            fn to_raw(self) -> Self::Raw {
                (self as #typ).to_raw()
            }

            fn from_raw(raw: Self::Raw) -> Self {
                let t = <#typ>::from_raw(raw);
                Self::new(t)
            }
        }

        #[cfg(feature = "experimental_traverse")]
        impl<'a> From<#name> for FieldType<'a> {
            fn from(src: #name) -> FieldType<'a> {
                (src as #typ).into()
            }
        }
    }
}

pub(crate) fn generate_raw_enum_compile(raw: &RawEnum) -> TokenStream {
    //NOTE: we reuse the decls of these from read-fonts, and only implement the traits.

    let name = &raw.name;
    let typ = &raw.typ;

    quote! {
        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                let val = *self as #typ;
                writer.write_slice(&val.to_be_bytes())
            }
        }
    }
}
