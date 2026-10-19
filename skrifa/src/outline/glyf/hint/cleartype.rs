//! ClearType policy and interpreter state.

/// The smoothing and grid fitting a font requests at a given size.
///
/// A rasterizer needs both properties. [`Self::symmetric_smoothing`] selects
/// antialiasing along both axes or only horizontally. [`Self::grid_fit`]
/// selects whether to hint at all. Windows reads the same two properties from
/// the same source, so honoring them makes text match Windows.
///
/// See <https://learn.microsoft.com/en-us/typography/opentype/spec/gasp>.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RasterPolicy {
    /// True if the rasterizer should smooth along both axes.
    ///
    /// When false, the font expects antialiasing only in the horizontal
    /// direction. A rasterizer that smooths in both directions produces
    /// softer horizontal edges than the font expects.
    ///
    /// This also sets the symmetric rendering bit of the `GETINFO`
    /// instruction.
    pub symmetric_smoothing: bool,
    /// True if TrueType grid fitting should be applied.
    pub grid_fit: bool,
}

impl RasterPolicy {}

impl Default for RasterPolicy {
    fn default() -> Self {
        Self {
            symmetric_smoothing: true,
            grid_fit: true,
        }
    }
}

/// The set of ClearType behaviors the TrueType interpreter applies.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ClearType {
    /// Every glyph is grid fitted, the symmetric rendering bit of `GETINFO`
    /// follows the hinting options alone, and horizontal moves round to whole
    /// pixels.
    Classic,
    /// A version 1 `gasp` table selects symmetric smoothing and grid fitting
    /// per size, and glyphs that opt out of backward compatibility keep
    /// fractional horizontal positions.
    Enhanced,
}

/// ClearType configuration for one hinting instance.
#[derive(Copy, Clone, Debug)]
pub struct ClearTypeState {
    /// The set of ClearType behaviors the interpreter applies.
    pub mode: ClearType,
    /// True if the font produces broken contours without hinting.
    ///
    /// Their glyph programs run without backward compatibility, and the native
    /// ClearType adjustments are skipped.
    pub is_hint_reliant: bool,
    /// The policy resolved from the `gasp` table for this instance's size.
    pub policy: RasterPolicy,
}

impl ClearTypeState {
    /// Returns true if the native ClearType adjustments to the interpreter
    /// are active.
    pub fn is_native_enhanced(&self) -> bool {
        self.mode == ClearType::Enhanced && !self.is_hint_reliant
    }

    /// Returns the resolved value for the symmetric rendering bit of
    /// `GETINFO`.
    pub fn symmetric_smoothing(&self) -> bool {
        self.mode == ClearType::Classic || self.policy.symmetric_smoothing
    }
}

impl Default for ClearTypeState {
    fn default() -> Self {
        Self {
            mode: ClearType::Classic,
            is_hint_reliant: false,
            policy: RasterPolicy::default(),
        }
    }
}
