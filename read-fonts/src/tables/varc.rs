//! the [VARC (Variable Composite/Component)](https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md) table

pub use super::layout::CoverageTable;

include!("../../generated/generated_varc.rs");

#[cfg(test)]
mod tests {
    use crate::{FontRef, TableProvider};

    #[test]
    fn read_cjk_0x6868() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let table = font.varc().unwrap();
        table.coverage().unwrap(); // should have coverage
    }
}
