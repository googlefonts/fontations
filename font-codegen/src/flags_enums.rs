//! codegen for bitflags and raw enums

use proc_macro2::TokenStream;
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

        #[cfg(feature = "traversal")]
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

        #[cfg(feature = "traversal")]
        impl<'a> From<#name> for FieldType<'a> {
            fn from(src: #name) -> FieldType<'a> {
                (src as #typ).into()
            }
        }
    }
}

pub(crate) fn generate_raw_enum_compile(raw: &RawEnum) -> TokenStream {
    //NOTE: we can reuse the declarations of these from parsing, but you need
    //to import them manually; we only implement the traits.

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

    let match_arms = raw.variants.iter().map(|variant| {
        let name = &variant.name;
        let value = &variant.value;
        quote!( Self::#name => #value, )
    });

    quote! {
        #( #docs )*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(#typ)]
        pub enum #name {
            #( #variants )*
        }

        impl FontWrite for #name {
            fn write_into(&self, writer: &mut TableWriter) {
                let val: #typ = match self {
                    #( #match_arms )*
                };
                writer.write_slice(&val.to_be_bytes())
            }
        }
    }
}
