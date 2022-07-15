//! codegen for bitflags and raw enums

use quote::quote;

use super::parsing::{BitFlags, RawEnum};

pub(crate) fn generate_flags(raw: &BitFlags) -> proc_macro2::TokenStream {
    let name = &raw.name;
    let docs = &raw.docs;
    let typ = &raw.typ;
    let variants = raw.variants.iter().map(|variant| {
        let name = &variant.name;
        let value = &variant.value;
        let docs = &variant.docs;
        quote! {
            #( #docs )*
            const #name = #value;
        }
    });

    quote! {
        bitflags::bitflags! {
            #( #docs )*
            pub struct #name: #typ {
                #( #variants )*
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

        impl ReadScalar for #name {
            const RAW_BYTE_LEN: usize = #typ::RAW_BYTE_LEN;
            fn read(bytes: &[u8]) -> Option<Self> {
                #typ::read(bytes).map(Self::from_bits_truncate)
            }
        }
    }
}

pub(crate) fn generate_raw_enum(raw: &RawEnum) -> proc_macro2::TokenStream {
    let name = &raw.name;
    let docs = &raw.docs;
    let typ = &raw.typ;
    let variants = raw.variants.iter().map(|variant| {
        let name = &variant.name;
        let value = &variant.value;
        let docs = &variant.docs;
        quote! {
            #( #docs )*
            #name = #value,
        }
    });
    let variant_inits = raw.variants.iter().map(|variant| {
        let name = &variant.name;
        let value = &variant.value;
        quote!(#value => Self::#name,)
    });

    quote! {
        #( #docs )*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(#typ)]
        pub enum #name {
            #( #variants )*
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

        impl ReadScalar for #name {
            const RAW_BYTE_LEN: usize = #typ::RAW_BYTE_LEN;
            fn read(bytes: &[u8]) -> Option<Self> {
                #typ::read(bytes).map(Self::new)
            }
        }
    }
}
