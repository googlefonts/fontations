//! Font model interop for Skrifa and HarfRust.
//!
//! This should not be used directly.

use crate::model::font::{Font, Tables};
use alloc::{boxed::Box, sync::Arc};
use core::any::Any;

/// Internal interop point for HarfRust.
///
/// Calling this directly will make your `Font` **UNUSABLE** with
/// HarfRust.
pub fn _get_or_init_shaping_data(
    font: &Font,
    f: impl FnOnce() -> Box<dyn Any + Send + Sync>,
) -> &dyn Any {
    &**font.shared().0.shaping_data.get_or_init(|| f())
}

/// Internal interop point for HarfRust.
///
/// Returns the shared tables of an sfnt font, for use as the owner of
/// cached data that borrows from them. Returns `None` for other fonts.
pub fn _table_cache_host(font: &Font) -> Option<Arc<Tables>> {
    font.shared().tables_arc().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recover_shaping_data() {
        struct ShapingData {
            arg: i32,
        }
        let font = Font::new(font_test_data::AHEM, 0).unwrap();
        let data = _get_or_init_shaping_data(&font, || Box::new(ShapingData { arg: -42 }));
        let actual = data.downcast_ref::<ShapingData>().unwrap();
        assert_eq!(actual.arg, -42);
    }

    #[test]
    fn table_cache_host_shares_tables() {
        let font = Font::new(font_test_data::AHEM, 0).unwrap();
        let host = _table_cache_host(&font).unwrap();
        assert!(core::ptr::eq(&*host, font.tables()));
    }
}
