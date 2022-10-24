//! The [CPAL](https://docs.microsoft.com/en-us/typography/opentype/spec/fvar) table

use font_types::Tag;

/// 'CPAL'
pub const TAG: Tag = Tag::new(b"fvar");

include!("../../generated/generated_fvar.rs");

#[cfg(test)]
mod tests {
    use crate::test_data;

    #[test]
    fn read_sample() {
        // TODO 
    }
}
