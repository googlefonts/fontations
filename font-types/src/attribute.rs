//! Primary attributes typically used for font classification and selection.

/// Visual width of a font-- a relative change from the normal aspect
/// ratio, typically in the range 0.5 to 2.0.
///
/// In variable fonts, this can be controlled with the `wdth` axis.
///
/// See <https://fonts.google.com/knowledge/glossary/width>
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct Stretch(f32);

impl Stretch {
    /// Width that is 50% of normal.
    pub const ULTRA_CONDENSED: Self = Self(0.5);

    /// Width that is 62.5% of normal.
    pub const EXTRA_CONDENSED: Self = Self(0.625);

    /// Width that is 75% of normal.
    pub const CONDENSED: Self = Self(0.75);

    /// Width that is 87.5% of normal.
    pub const SEMI_CONDENSED: Self = Self(0.875);

    /// Width that is 100% of normal.
    pub const NORMAL: Self = Self(1.0);

    /// Width that is 112.5% of normal.
    pub const SEMI_EXPANDED: Self = Self(1.125);

    /// Width that is 125% of normal.
    pub const EXPANDED: Self = Self(1.25);

    /// Width that is 150% of normal.
    pub const EXTRA_EXPANDED: Self = Self(1.5);

    /// Width that is 200% of normal.
    pub const ULTRA_EXPANDED: Self = Self(2.0);
}

impl Stretch {
    /// Creates a new stretch attribute with the given ratio.
    pub const fn new(ratio: f32) -> Self {
        Self(ratio)
    }

    /// Creates a new stretch attribute from the
    /// [usWidthClass](<https://learn.microsoft.com/en-us/typography/opentype/spec/os2#uswidthclass>)
    /// field of the OS/2 table.
    pub fn from_width_class(width_class: u16) -> Self {
        // The specified range is 1-9 and Skia simply clamps out of range
        // values. We follow.
        // See <https://skia.googlesource.com/skia/+/21b7538fe0757d8cda31598bc9e5a6d0b4b54629/include/core/SkFontStyle.h#52>
        match width_class {
            0..=1 => Stretch::ULTRA_CONDENSED,
            2 => Stretch::EXTRA_CONDENSED,
            3 => Stretch::CONDENSED,
            4 => Stretch::SEMI_CONDENSED,
            5 => Stretch::NORMAL,
            6 => Stretch::SEMI_EXPANDED,
            7 => Stretch::EXPANDED,
            8 => Stretch::EXTRA_EXPANDED,
            _ => Stretch::ULTRA_EXPANDED,
        }
    }

    /// Returns the stretch attribute as a ratio.
    ///
    /// This is a linear scaling factor with 1.0 being "normal" width.
    pub const fn ratio(self) -> f32 {
        self.0
    }

    /// Returns the stretch attribute as a percentage value.
    ///
    /// This is generally the value associated with the `wdth` axis.
    pub fn percentage(self) -> f32 {
        self.0 * 100.0
    }
}

impl Default for Stretch {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Visual style or 'slope' of a font.
///
/// In variable fonts, this can be controlled with the `ital`
/// and `slnt` axes for italic and oblique styles, respectively.
///
/// See <https://fonts.google.com/knowledge/glossary/style>
#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub enum Style {
    /// An upright or "roman" style.
    #[default]
    Normal,
    /// Generally a slanted style, originally based on semi-cursive forms.
    /// This often has a different structure from the normal style.
    Italic,
    /// Oblique (or slanted) style with an optional angle in degrees,
    /// counter-clockwise from the vertical.
    Oblique(Option<f32>),
}

/// Visual weight class of a font, typically on a scale from 1.0 to 1000.0.
///
/// In variable fonts, this can be controlled with the `wght` axis.
///
/// See <https://fonts.google.com/knowledge/glossary/weight>
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct Weight(f32);

impl Weight {
    /// Weight value of 100.
    pub const THIN: Self = Self(100.0);

    /// Weight value of 200.
    pub const EXTRA_LIGHT: Self = Self(200.0);

    /// Weight value of 300.
    pub const LIGHT: Self = Self(300.0);

    /// Weight value of 350.
    pub const SEMI_LIGHT: Self = Self(350.0);

    /// Weight value of 400.
    pub const NORMAL: Self = Self(400.0);

    /// Weight value of 500.
    pub const MEDIUM: Self = Self(500.0);

    /// Weight value of 600.
    pub const SEMI_BOLD: Self = Self(600.0);

    /// Weight value of 700.
    pub const BOLD: Self = Self(700.0);

    /// Weight value of 800.
    pub const EXTRA_BOLD: Self = Self(800.0);

    /// Weight value of 900.
    pub const BLACK: Self = Self(900.0);

    /// Weight value of 950.
    pub const EXTRA_BLACK: Self = Self(950.0);
}

impl Weight {
    /// Creates a new weight attribute with the given value.
    pub const fn new(weight: f32) -> Self {
        Self(weight)
    }

    /// Returns the underlying weight value.
    pub const fn value(self) -> f32 {
        self.0
    }
}

impl Default for Weight {
    fn default() -> Self {
        Self::NORMAL
    }
}
