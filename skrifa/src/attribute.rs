//! Primary attributes typically used for font classification and selection.

use read_fonts::{
    tables::{
        head::{Head, MacStyle},
        os2::{Os2, SelectionFlags},
        post::Post,
    },
    FontRef, TableProvider,
};

pub use read_fonts::types::{Stretch, Style, Weight};

/// Stretch, style and weight attributes of a font.
///
/// Variable fonts may contain axes that modify these attributes. The
/// [new](Self::new) method on this type returns values for the default
/// instance.
///
/// These are derived from values in the
/// [OS/2](https://learn.microsoft.com/en-us/typography/opentype/spec/os2) if
/// available. Otherwise, they are retrieved from the
/// [head](https://learn.microsoft.com/en-us/typography/opentype/spec/head)
/// table.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct Attributes {
    pub stretch: Stretch,
    pub style: Style,
    pub weight: Weight,
}

impl Attributes {
    /// Extracts the stretch, style and weight attributes for the default
    /// instance of the given font.
    pub fn new(font: &FontRef) -> Self {
        if let Ok(os2) = font.os2() {
            // Prefer values from the OS/2 table if it exists. We also use
            // the post table to extract the angle for oblique styles.
            Self::from_os2_post(os2, font.post().ok())
        } else if let Ok(head) = font.head() {
            // Otherwise, fall back to the macStyle field of the head table.
            Self::from_head(head)
        } else {
            Self::default()
        }
    }

    fn from_os2_post(os2: Os2, post: Option<Post>) -> Self {
        let stretch = Stretch::from_width_class(os2.us_width_class());
        // Bits 1 and 9 of the fsSelection field signify italic and
        // oblique, respectively.
        // See: <https://learn.microsoft.com/en-us/typography/opentype/spec/os2#fsselection>
        let fs_selection = os2.fs_selection();
        let style = if fs_selection.contains(SelectionFlags::ITALIC) {
            Style::Italic
        } else if fs_selection.contains(SelectionFlags::OBLIQUE) {
            let angle = post.map(|post| post.italic_angle().to_f64() as f32);
            Style::Oblique(angle)
        } else {
            Style::Normal
        };
        // The usWeightClass field is specified with a 1-1000 range, but
        // we don't clamp here because variable fonts could potentially
        // have a value outside of that range.
        // See <https://learn.microsoft.com/en-us/typography/opentype/spec/os2#usweightclass>
        let weight = Weight::new(os2.us_weight_class() as f32);
        Self {
            stretch,
            style,
            weight,
        }
    }

    fn from_head(head: Head) -> Self {
        let mac_style = head.mac_style();
        let style = if mac_style.contains(MacStyle::ITALIC) {
            Style::Italic
        } else {
            Default::default()
        };
        let weight = if mac_style.contains(MacStyle::BOLD) {
            Weight::BOLD
        } else {
            Default::default()
        };
        Self {
            stretch: Stretch::default(),
            style,
            weight,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn missing_os2() {
        let font = FontRef::new(font_test_data::CMAP12_FONT1).unwrap();
        let attrs = font.attributes();
        assert_eq!(attrs.stretch, Stretch::NORMAL);
        assert_eq!(attrs.style, Style::Italic);
        assert_eq!(attrs.weight, Weight::BOLD);
    }

    #[test]
    fn so_stylish() {
        let font = FontRef::new(font_test_data::CMAP14_FONT1).unwrap();
        let attrs = font.attributes();
        assert_eq!(attrs.stretch, Stretch::SEMI_CONDENSED);
        assert_eq!(attrs.style, Style::Oblique(Some(-14.0)));
        assert_eq!(attrs.weight, Weight::EXTRA_BOLD);
    }
}
