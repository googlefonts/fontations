//! A client side implementation of the Incremental Font Transfer standard <https://w3c.github.io/IFT/Overview.html#glyph-keyed>
//!  
//! More specifically this provides:
//! - Implementation of parsing and reading incremental font patch mappings:
//!   <https://w3c.github.io/IFT/Overview.html#font-format-extensions>
//! - Implementation of parsing and apply incremental font patches:
//!   <https://w3c.github.io/IFT/Overview.html#font-patch-formats>
//!
//! Built on top of the read-fonts crate.

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![forbid(unsafe_code)]

pub mod font_patch;
pub mod glyph_keyed;
pub mod patch_group;
pub mod patchmap;
pub mod table_keyed;
