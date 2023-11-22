//! Wherein we read and write fonts.

pub use font_types::*;

/// Reading fonts.
pub mod read {
    pub use read_fonts::*;
}

/// Writing fonts.
#[cfg_attr(doc_cfg, doc(cfg(feature = "write")))]
#[cfg(feature = "write")]
pub mod write {
    pub use write_fonts::*;
}
