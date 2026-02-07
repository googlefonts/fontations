//! the [VARC (Variable Composite/Component)](https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md) table

use super::variations::{PackedDeltas, NO_VARIATION_INDEX};
pub use super::{
    layout::{Condition, CoverageTable},
    postscript::Index2,
};

#[cfg(feature = "libm")]
#[allow(unused_imports)]
use core_maths::*;

include!("../../generated/generated_varc.rs");

/// Let's us call self.something().get(i) instead of get(self.something(), i)
trait Get<'a> {
    fn get(self, nth: usize) -> Result<&'a [u8], ReadError>;
}

impl<'a> Get<'a> for Option<Result<Index2<'a>, ReadError>> {
    fn get(self, nth: usize) -> Result<&'a [u8], ReadError> {
        self.transpose()?
            .ok_or(ReadError::NullOffset)
            .and_then(|index| index.get(nth).map_err(|_| ReadError::OutOfBounds))
    }
}

impl Varc<'_> {
    /// Friendlier accessor than directly using raw data via [Index2]
    pub fn axis_indices(&self, nth: usize) -> Result<PackedDeltas<'_>, ReadError> {
        let raw = self.axis_indices_list().get(nth)?;
        Ok(PackedDeltas::consume_all(raw.into()))
    }

    /// Friendlier accessor than directly using raw data via [Index2]
    ///
    /// nth would typically be obtained by looking up a [GlyphId] in [Self::coverage].
    pub fn glyph(&self, nth: usize) -> Result<VarcGlyph<'_>, ReadError> {
        let raw = Some(self.var_composite_glyphs()).get(nth)?;
        Ok(VarcGlyph {
            table: self,
            data: raw.into(),
        })
    }
}

/// A VARC glyph doesn't have any root level attributes, it's just a list of components
///
/// <https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md#variable-composite-description>
pub struct VarcGlyph<'a> {
    table: &'a Varc<'a>,
    data: FontData<'a>,
}

impl<'a> VarcGlyph<'a> {
    /// <https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otTables.py#L404-L409>
    pub fn components(&self) -> impl Iterator<Item = Result<VarcComponent<'a>, ReadError>> {
        VarcComponentIter {
            table: self.table,
            cursor: self.data.cursor(),
        }
    }
}

struct VarcComponentIter<'a> {
    table: &'a Varc<'a>,
    cursor: Cursor<'a>,
}

impl<'a> Iterator for VarcComponentIter<'a> {
    type Item = Result<VarcComponent<'a>, ReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.is_empty() {
            return None;
        }
        Some(VarcComponent::parse(self.table, &mut self.cursor))
    }
}

pub struct VarcComponent<'a> {
    flags: VarcFlags,
    gid: GlyphId,
    condition_index: Option<u32>,
    axis_indices_index: Option<u32>,
    axis_values: Option<PackedDeltas<'a>>,
    axis_values_var_index: Option<u32>,
    transform_var_index: Option<u32>,
    transform: DecomposedTransform,
}

impl<'a> VarcComponent<'a> {
    /// Requires access to VARC fields to fully parse.
    ///
    ///  * HarfBuzz [VarComponent::get_path_at](https://github.com/harfbuzz/harfbuzz/blob/0c2f5ecd51d11e32836ee136a1bc765d650a4ec0/src/OT/Var/VARC/VARC.cc#L132)
    fn parse(table: &Varc, cursor: &mut Cursor<'a>) -> Result<Self, ReadError> {
        let raw_flags = cursor.read_u32_var()?;
        let flags = VarcFlags::from_bits_truncate(raw_flags);
        // Ref https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md#variable-component-record

        // This is a GlyphID16 if GID_IS_24BIT bit of flags is clear, else GlyphID24.
        let gid = if raw_flags & VarcFlags::GID_IS_24BIT.bits != 0 {
            GlyphId::new(cursor.read::<Uint24>()?.to_u32())
        } else {
            GlyphId::from(cursor.read::<u16>()?)
        };

        let condition_index = if raw_flags & VarcFlags::HAVE_CONDITION.bits != 0 {
            Some(cursor.read_u32_var()?)
        } else {
            None
        };

        let (axis_indices_index, axis_values) = if raw_flags & VarcFlags::HAVE_AXES.bits != 0 {
            // <https://github.com/harfbuzz/harfbuzz/blob/0c2f5ecd51d11e32836ee136a1bc765d650a4ec0/src/OT/Var/VARC/VARC.cc#L195-L206>
            let axis_indices_index = cursor.read_u32_var()?;
            let num_axis_values = table
                .axis_indices(axis_indices_index as usize)?
                .count_or_compute();
            // we need to consume num_axis_values entries in packed delta format
            let deltas = if num_axis_values > 0 {
                let Some(data) = cursor.remaining() else {
                    return Err(ReadError::OutOfBounds);
                };
                let deltas = PackedDeltas::new(data, num_axis_values);
                *cursor = deltas.iter().end(); // jump past the packed deltas
                Some(deltas)
            } else {
                None
            };
            (Some(axis_indices_index), deltas)
        } else {
            (None, None)
        };

        let axis_values_var_index = if raw_flags & VarcFlags::AXIS_VALUES_HAVE_VARIATION.bits != 0 {
            Some(cursor.read_u32_var()?)
        } else {
            None
        };

        let transform_var_index = if raw_flags & VarcFlags::TRANSFORM_HAS_VARIATION.bits != 0 {
            Some(cursor.read_u32_var()?)
        } else {
            None
        };

        // Keep transform values in raw encoded units while parsing:
        // - rotation/skew: F4Dot12 raw units
        // - scale: F6Dot10 raw units
        // Division to normalized values can be deferred by consumers.
        let mut transform = DecomposedTransform {
            scale_x: 1024.0,
            scale_y: 1024.0,
            ..Default::default()
        };
        let translate_mask = VarcFlags::HAVE_TRANSLATE_X.bits | VarcFlags::HAVE_TRANSLATE_Y.bits;
        if raw_flags & translate_mask != 0 {
            if raw_flags & VarcFlags::HAVE_TRANSLATE_X.bits != 0 {
                transform.translate_x = cursor.read::<FWord>()?.to_i16() as f32
            }
            if raw_flags & VarcFlags::HAVE_TRANSLATE_Y.bits != 0 {
                transform.translate_y = cursor.read::<FWord>()?.to_i16() as f32
            }
        }
        if raw_flags & VarcFlags::HAVE_ROTATION.bits != 0 {
            transform.rotation = cursor.read::<F4Dot12>()?.to_bits() as f32
        }
        let scale_mask = VarcFlags::HAVE_SCALE_X.bits | VarcFlags::HAVE_SCALE_Y.bits;
        if raw_flags & scale_mask != 0 {
            if raw_flags & VarcFlags::HAVE_SCALE_X.bits != 0 {
                transform.scale_x = cursor.read::<F6Dot10>()?.to_bits() as f32
            }
            transform.scale_y = if raw_flags & VarcFlags::HAVE_SCALE_Y.bits != 0 {
                cursor.read::<F6Dot10>()?.to_bits() as f32
            } else {
                transform.scale_x
            };
        }
        let center_mask = VarcFlags::HAVE_TCENTER_X.bits | VarcFlags::HAVE_TCENTER_Y.bits;
        if raw_flags & center_mask != 0 {
            if raw_flags & VarcFlags::HAVE_TCENTER_X.bits != 0 {
                transform.center_x = cursor.read::<FWord>()?.to_i16() as f32
            }
            if raw_flags & VarcFlags::HAVE_TCENTER_Y.bits != 0 {
                transform.center_y = cursor.read::<FWord>()?.to_i16() as f32
            }
        }
        let skew_mask = VarcFlags::HAVE_SKEW_X.bits | VarcFlags::HAVE_SKEW_Y.bits;
        if raw_flags & skew_mask != 0 {
            if raw_flags & VarcFlags::HAVE_SKEW_X.bits != 0 {
                transform.skew_x = cursor.read::<F4Dot12>()?.to_bits() as f32
            }
            if raw_flags & VarcFlags::HAVE_SKEW_Y.bits != 0 {
                transform.skew_y = cursor.read::<F4Dot12>()?.to_bits() as f32
            }
        }

        // Optional, process and discard one uint32var per each set bit in RESERVED_MASK.
        let reserved = raw_flags & VarcFlags::RESERVED_MASK.bits;
        if reserved != 0 {
            let num_reserved = reserved.count_ones();
            for _ in 0..num_reserved {
                cursor.read_u32_var()?;
            }
        }
        Ok(VarcComponent {
            flags,
            gid,
            condition_index,
            axis_indices_index,
            axis_values,
            axis_values_var_index,
            transform_var_index,
            transform,
        })
    }

    pub fn flags(&self) -> VarcFlags {
        self.flags
    }
    pub fn gid(&self) -> GlyphId {
        self.gid
    }
    pub fn condition_index(&self) -> Option<u32> {
        self.condition_index
    }
    pub fn transform(&self) -> &DecomposedTransform {
        &self.transform
    }
    pub fn axis_indices_index(&self) -> Option<u32> {
        self.axis_indices_index
    }
    pub fn axis_values(&self) -> Option<&PackedDeltas<'a>> {
        self.axis_values.as_ref()
    }
    pub fn axis_values_var_index(&self) -> Option<u32> {
        self.axis_values_var_index
    }
    pub fn transform_var_index(&self) -> Option<u32> {
        self.transform_var_index
    }
}

/// <https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/misc/transform.py#L410>
#[derive(Clone, Copy)]
pub struct DecomposedTransform {
    translate_x: f32,
    translate_y: f32,
    rotation: f32, // multiples of Pi, counter-clockwise
    scale_x: f32,
    scale_y: f32,
    skew_x: f32, // multiples of Pi, clockwise
    skew_y: f32, // multiples of Pi, counter-clockwise
    center_x: f32,
    center_y: f32,
}

impl Default for DecomposedTransform {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            skew_x: 0.0,
            skew_y: 0.0,
            center_x: 0.0,
            center_y: 0.0,
        }
    }
}

impl DecomposedTransform {
    pub fn translate_x(&self) -> f32 {
        self.translate_x
    }

    pub fn translate_y(&self) -> f32 {
        self.translate_y
    }

    pub fn rotation(&self) -> f32 {
        self.rotation
    }

    pub fn scale_x(&self) -> f32 {
        self.scale_x
    }

    pub fn scale_y(&self) -> f32 {
        self.scale_y
    }

    pub fn skew_x(&self) -> f32 {
        self.skew_x
    }

    pub fn skew_y(&self) -> f32 {
        self.skew_y
    }

    pub fn center_x(&self) -> f32 {
        self.center_x
    }

    pub fn center_y(&self) -> f32 {
        self.center_y
    }

    pub fn set_translate_x(&mut self, value: f32) {
        self.translate_x = value;
    }

    pub fn set_translate_y(&mut self, value: f32) {
        self.translate_y = value;
    }

    pub fn set_rotation(&mut self, value: f32) {
        self.rotation = value;
    }

    pub fn set_scale_x(&mut self, value: f32) {
        self.scale_x = value;
    }

    pub fn set_scale_y(&mut self, value: f32) {
        self.scale_y = value;
    }

    pub fn set_skew_x(&mut self, value: f32) {
        self.skew_x = value;
    }

    pub fn set_skew_y(&mut self, value: f32) {
        self.skew_y = value;
    }

    pub fn set_center_x(&mut self, value: f32) {
        self.center_x = value;
    }

    pub fn set_center_y(&mut self, value: f32) {
        self.center_y = value;
    }

    /// Convert decomposed form to 2x3 matrix form.
    ///
    /// The first two values are x,y x-basis vector,
    /// the second 2 values are x,y y-basis vector, and the third 2 are translation.
    ///
    /// In augmented matrix
    /// form, if this method returns `[a, b, c, d, e, f]` that is taken as:
    ///
    /// ```text
    /// | a c e |
    /// | b d f |
    /// | 0 0 1 |
    /// ```
    ///
    /// References:
    ///   FontTools Python implementation <https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/misc/transform.py#L484-L500>
    /// * Wikipedia [affine transformation](https://en.wikipedia.org/wiki/Affine_transformation)
    pub fn matrix(&self) -> [f32; 6] {
        // Python: t.translate(self.translateX + self.tCenterX, self.translateY + self.tCenterY)
        let mut transform = [
            1.0,
            0.0,
            0.0,
            1.0,
            self.translate_x + self.center_x,
            self.translate_y + self.center_y,
        ];

        // TODO: this produces very small floats for rotations, e.g. 90 degree rotation a basic scale
        // puts 1.2246467991473532e-16 into [0]. Should we special case? Round?

        // Python: t = t.rotate(self.rotation * math.pi)
        if self.rotation != 0.0 {
            let rot = self.rotation * core::f32::consts::PI;
            let (s, c) = rot.sin_cos();
            eprintln!(
                "VARC_TRIG kind=rotate in={:.16} sin={:.16} cos={:.16}",
                rot as f64, s as f64, c as f64
            );
            transform = transform.transform([c, s, -s, c, 0.0, 0.0]);
        }

        // Python: t = t.scale(self.scaleX, self.scaleY)
        if (self.scale_x, self.scale_y) != (1.0, 1.0) {
            transform = transform.transform([self.scale_x, 0.0, 0.0, self.scale_y, 0.0, 0.0]);
        }

        // Python: t = t.skew(-self.skewX * math.pi, self.skewY * math.pi)
        if (self.skew_x, self.skew_y) != (0.0, 0.0) {
            let skew_y = self.skew_y * core::f32::consts::PI;
            let skew_x = -self.skew_x * core::f32::consts::PI;
            let tan_y = skew_y.tan();
            let tan_x = skew_x.tan();
            eprintln!(
                "VARC_TRIG kind=skew in_x={:.16} in_y={:.16} tan_x={:.16} tan_y={:.16}",
                skew_x as f64, skew_y as f64, tan_x as f64, tan_y as f64
            );
            transform = transform.transform([
                1.0,
                tan_y,
                tan_x,
                1.0,
                0.0,
                0.0,
            ])
        }

        // Python: t = t.translate(-self.tCenterX, -self.tCenterY)
        if (self.center_x, self.center_y) != (0.0, 0.0) {
            transform = transform.transform([1.0, 0.0, 0.0, 1.0, -self.center_x, -self.center_y]);
        }

        transform
    }
}

trait Transform {
    fn transform(self, other: Self) -> Self;
}

impl Transform for [f32; 6] {
    fn transform(self, other: Self) -> Self {
        // Shamelessly copied from kurbo Affine Mul
        [
            self[0] * other[0] + self[2] * other[1],
            self[1] * other[0] + self[3] * other[1],
            self[0] * other[2] + self[2] * other[3],
            self[1] * other[2] + self[3] * other[3],
            self[0] * other[4] + self[2] * other[5] + self[4],
            self[1] * other[4] + self[3] * other[5] + self[5],
        ]
    }
}

impl<'a> MultiItemVariationData<'a> {
    /// An [Index2] where each item is a [PackedDeltas]
    pub fn delta_sets(&self) -> Result<Index2<'a>, ReadError> {
        Index2::read(self.raw_delta_sets().into())
    }

    /// Read a specific delta set.
    ///
    /// Equivalent to calling [Self::delta_sets], fetching item i, and parsing as [PackedDeltas]
    pub fn delta_set(&self, i: usize) -> Result<PackedDeltas<'a>, ReadError> {
        let index = self.delta_sets()?;
        let raw_deltas = index.get(i).map_err(|_| ReadError::OutOfBounds)?;
        Ok(PackedDeltas::consume_all(raw_deltas.into()))
    }
}

impl<'a> MultiItemVariationStore<'a> {
    /// Adds tuple deltas for `var_idx` into `out` using float coordinates in raw
    /// 2.14 units (that is, `F2Dot14::to_bits() as f32`).
    ///
    /// If provided, `scalar_cache` should be indexed by region index and initialized
    /// to values greater than `1.0` (for example, `2.0`) to indicate "not cached".
    pub fn add_tuple_deltas_raw_f32(
        &self,
        region_list: &SparseVariationRegionList<'a>,
        var_idx: u32,
        coords: &[f32],
        tuple_len: usize,
        out: &mut [f32],
        mut scalar_cache: Option<&mut [f32]>,
    ) -> Result<(), ReadError> {
        if tuple_len == 0 || var_idx == NO_VARIATION_INDEX {
            return Ok(());
        }
        if out.len() < tuple_len {
            return Err(ReadError::OutOfBounds);
        }
        let out = &mut out[..tuple_len];
        let outer = (var_idx >> 16) as usize;
        let inner = (var_idx & 0xFFFF) as usize;
        let data = self
            .variation_data()
            .get(outer)
            .map_err(|_| ReadError::InvalidCollectionIndex(outer as _))?;
        let region_indices = data.region_indices();
        let mut deltas = data.delta_set(inner)?.fetcher();
        let regions = region_list.regions();

        let mut skip = 0usize;
        for region_index in region_indices.iter() {
            let region_idx = region_index.get() as usize;
            let scalar = if let Some(cache) = scalar_cache.as_deref_mut() {
                if let Some(slot) = cache.get_mut(region_idx) {
                    if *slot <= 1.0 {
                        *slot
                    } else {
                        let computed = regions.get(region_idx)?.compute_scalar_raw_f32(coords);
                        *slot = computed;
                        computed
                    }
                } else {
                    regions.get(region_idx)?.compute_scalar_raw_f32(coords)
                }
            } else {
                regions.get(region_idx)?.compute_scalar_raw_f32(coords)
            };

            if scalar == 0.0 {
                skip += tuple_len;
                continue;
            }

            if skip != 0 {
                deltas.skip(skip)?;
                skip = 0;
            }
            deltas.add_to_f32_scaled(out, scalar)?;
        }
        Ok(())
    }
}

impl SparseVariationRegion<'_> {
    /// Computes a scalar for coordinates in raw 2.14 units
    /// (that is, `F2Dot14::to_bits() as f32`).
    pub fn compute_scalar_raw_f32(&self, coords: &[f32]) -> f32 {
        let mut scalar = 1.0f32;
        for axis in self.region_axes() {
            let axis_index = axis.axis_index() as usize;
            let coord = coords.get(axis_index).copied().unwrap_or(0.0);
            let peak = axis.peak().to_bits() as f32;
            if peak == 0.0 || coord == peak {
                continue;
            }
            if coord == 0.0 {
                return 0.0;
            }
            let start = axis.start().to_bits() as f32;
            let end = axis.end().to_bits() as f32;
            // Match HarfBuzz behavior for malformed regions.
            if start > peak || peak > end {
                continue;
            }
            if start < 0.0 && end > 0.0 && peak != 0.0 {
                continue;
            }
            // Endpoints are out-of-range in HB.
            if coord <= start || end <= coord {
                return 0.0;
            } else {
                let factor = if coord < peak {
                    (coord - start) / (peak - start)
                } else {
                    (end - coord) / (end - peak)
                };
                if factor == 0.0 {
                    return 0.0;
                }
                scalar *= factor;
            }
        }
        scalar
    }
}

#[cfg(test)]
mod tests {
    use types::GlyphId16;

    use crate::{FontRef, ReadError, TableProvider};

    use super::{Condition, DecomposedTransform, Varc};

    impl Varc<'_> {
        fn conditions(&self) -> impl Iterator<Item = Condition<'_>> {
            self.condition_list()
                .expect("A condition list is present")
                .expect("We could read the condition list")
                .conditions()
                .iter()
                .enumerate()
                .map(|(i, c)| c.unwrap_or_else(|e| panic!("condition {i} {e}")))
        }

        fn axis_indices_count(&self) -> Result<usize, ReadError> {
            let Some(axis_indices_list) = self.axis_indices_list() else {
                return Ok(0);
            };
            let axis_indices_list = axis_indices_list?;
            Ok(axis_indices_list.count() as usize)
        }
    }

    fn round6(v: f32) -> f32 {
        (v * 1_000_000.0).round() / 1_000_000.0
    }

    trait Round {
        fn round_for_test(self) -> Self;
    }

    impl Round for [f32; 6] {
        fn round_for_test(self) -> Self {
            [
                round6(self[0]),
                round6(self[1]),
                round6(self[2]),
                round6(self[3]),
                round6(self[4]),
                round6(self[5]),
            ]
        }
    }

    #[test]
    fn read_cjk_0x6868() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let table = font.varc().unwrap();
        table.coverage().unwrap(); // should have coverage
    }

    #[test]
    fn identify_all_conditional_types() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();

        // We should have all 5 condition types in order
        assert_eq!(
            (1..=5).collect::<Vec<_>>(),
            table.conditions().map(|c| c.format()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format1_axis_range() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format1AxisRange(condition)) =
            table.conditions().find(|c| c.format() == 1)
        else {
            panic!("No such item");
        };

        assert_eq!(
            (0, 0.5, 1.0),
            (
                condition.axis_index(),
                condition.filter_range_min_value().to_f32(),
                condition.filter_range_max_value().to_f32(),
            )
        );
    }

    #[test]
    fn read_condition_format2_variable_value() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format2VariableValue(condition)) =
            table.conditions().find(|c| c.format() == 2)
        else {
            panic!("No such item");
        };

        assert_eq!((1, 2), (condition.default_value(), condition.var_index(),));
    }

    #[test]
    fn read_condition_format3_and() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format3And(condition)) = table.conditions().find(|c| c.format() == 3)
        else {
            panic!("No such item");
        };

        // Should reference a format 1 and a format 2
        assert_eq!(
            vec![1, 2],
            condition
                .conditions()
                .iter()
                .map(|c| c.unwrap().format())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format4_or() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format4Or(condition)) = table.conditions().find(|c| c.format() == 4)
        else {
            panic!("No such item");
        };

        // Should reference a format 1 and a format 2
        assert_eq!(
            vec![1, 2],
            condition
                .conditions()
                .iter()
                .map(|c| c.unwrap().format())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format5_negate() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format5Negate(condition)) =
            table.conditions().find(|c| c.format() == 5)
        else {
            panic!("No such item");
        };

        // Should reference a format 1
        assert_eq!(1, condition.condition().unwrap().format(),);
    }

    #[test]
    fn read_axis_indices_list() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        assert_eq!(table.axis_indices_count().unwrap(), 2);
        assert_eq!(
            vec![2, 3, 4, 5, 6],
            table.axis_indices(1).unwrap().iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_glyph_6868() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let gid = font.cmap().unwrap().map_codepoint(0x6868_u32).unwrap();
        let table = font.varc().unwrap();
        let idx = table.coverage().unwrap().get(gid).unwrap();

        let glyph = table.glyph(idx as usize).unwrap();
        assert_eq!(
            vec![GlyphId16::new(2), GlyphId16::new(5), GlyphId16::new(7)],
            glyph
                .components()
                .map(|c| c.unwrap().gid)
                .collect::<Vec<_>>()
        );
    }

    // Expected created using the Python DecomposedTransform
    #[test]
    fn decomposed_scale_to_matrix() {
        let scale_x = 2.0;
        let scale_y = 3.0;
        assert_eq!(
            [scale_x, 0.0, 0.0, scale_y, 0.0, 0.0],
            DecomposedTransform {
                scale_x,
                scale_y,
                ..Default::default()
            }
            .matrix()
            .round_for_test()
        );
    }

    // Expected created using the Python DecomposedTransform
    #[test]
    fn decomposed_rotate_to_matrix() {
        assert_eq!(
            [0.0, 1.0, -1.0, 0.0, 0.0, 0.0],
            DecomposedTransform {
                // Rotation is in multiples of Pi (90 degrees = 0.5 * Pi).
                rotation: 0.5,
                ..Default::default()
            }
            .matrix()
            .round_for_test()
        );
    }

    // Expected created using the Python DecomposedTransform
    #[test]
    fn decomposed_skew_to_matrix() {
        // Skew is in multiples of Pi.
        let skew_x: f32 = 1.0 / 6.0; // 30 degrees
        let skew_y: f32 = -1.0 / 3.0; // -60 degrees
        assert_eq!(
            [
                1.0,
                round6((skew_y * core::f32::consts::PI).tan()),
                round6((-skew_x * core::f32::consts::PI).tan()),
                1.0,
                0.0,
                0.0
            ],
            DecomposedTransform {
                skew_x,
                skew_y,
                ..Default::default()
            }
            .matrix()
            .round_for_test()
        );
    }

    // Expected created using the Python DecomposedTransform
    #[test]
    fn decomposed_scale_rotate_to_matrix() {
        let scale_x = 2.0;
        let scale_y = 3.0;
        assert_eq!(
            [0.0, scale_x, -scale_y, 0.0, 0.0, 0.0],
            DecomposedTransform {
                scale_x,
                scale_y,
                // 90 degrees = 0.5 * Pi.
                rotation: 0.5,
                ..Default::default()
            }
            .matrix()
            .round_for_test()
        );
    }

    // Expected created using the Python DecomposedTransform
    #[test]
    fn decomposed_scale_rotate_translate_to_matrix() {
        assert_eq!(
            [0.0, 2.0, -1.0, 0.0, 10.0, 20.0],
            DecomposedTransform {
                scale_x: 2.0,
                // 90 degrees = 0.5 * Pi.
                rotation: 0.5,
                translate_x: 10.0,
                translate_y: 20.0,
                ..Default::default()
            }
            .matrix()
            .round_for_test()
        );
    }

    // Expected created using the Python DecomposedTransform
    #[test]
    fn decomposed_scale_skew_translate_to_matrix() {
        assert_eq!(
            [-0.866026, 5.5, -2.5, 2.020726, 10.0, 20.0],
            DecomposedTransform {
                scale_x: 2.0,
                scale_y: 3.0,
                // Angles are in multiples of Pi.
                rotation: 1.0 / 6.0, // 30 degrees
                skew_x: 1.0 / 6.0,   // 30 degrees
                skew_y: 1.0 / 3.0,   // 60 degrees
                translate_x: 10.0,
                translate_y: 20.0,
                ..Default::default()
            }
            .matrix()
            .round_for_test()
        );
    }

    // Expected created using the Python DecomposedTransform
    #[test]
    fn decomposed_rotate_around_to_matrix() {
        assert_eq!(
            [1.732051, 1.0, -0.5, 0.866025, 10.267949, 19.267949],
            DecomposedTransform {
                scale_x: 2.0,
                // 30 degrees = 1/6 * Pi.
                rotation: 1.0 / 6.0,
                translate_x: 10.0,
                translate_y: 20.0,
                center_x: 1.0,
                center_y: 2.0,
                ..Default::default()
            }
            .matrix()
            .round_for_test()
        );
    }

    #[test]
    fn read_multivar_store_region_list() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let table = font.varc().unwrap();
        let varstore = table.multi_var_store().unwrap().unwrap();
        let regions = varstore.region_list().unwrap().regions();

        let sparse_regions = regions
            .iter()
            .map(|r| {
                r.unwrap()
                    .region_axes()
                    .iter()
                    .map(|a| {
                        (
                            a.axis_index(),
                            a.start().to_f32(),
                            a.peak().to_f32(),
                            a.end().to_f32(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        // Check a sampling of the regions
        assert_eq!(
            vec![
                vec![(0, 0.0, 1.0, 1.0),],
                vec![(0, 0.0, 1.0, 1.0), (1, 0.0, 1.0, 1.0),],
                vec![(6, -1.0, -1.0, 0.0),],
            ],
            [0, 2, 38]
                .into_iter()
                .map(|i| sparse_regions[i].clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_multivar_store_delta_sets() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let table = font.varc().unwrap();
        let varstore = table.multi_var_store().unwrap().unwrap();
        assert_eq!(
            vec![(3, 6), (33, 6), (10, 5), (25, 8),],
            varstore
                .variation_data()
                .iter()
                .map(|d| d.unwrap())
                .map(|d| (d.region_index_count(), d.delta_sets().unwrap().count()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            vec![-1, 33, 0, 0, 0, 0],
            varstore
                .variation_data()
                .get(0)
                .unwrap()
                .delta_set(5)
                .unwrap()
                .iter()
                .collect::<Vec<_>>()
        )
    }
}
