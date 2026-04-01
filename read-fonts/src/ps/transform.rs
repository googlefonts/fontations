//! Font transform types.

use super::num;
use crate::Cursor;
use types::Fixed;

/// Combination of a matrix and optional scale.
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub struct Transform {
    /// Affine font matrix.
    pub matrix: FontMatrix,
    /// Fixed point scale factor.
    ///
    /// This is assumed to convert from font units to 26.6 values.
    pub scale: Option<Fixed>,
}

impl Transform {
    /// The transform that doesn't modify coordinates or metrics.
    pub const IDENTITY: Self = Self {
        matrix: FontMatrix::IDENTITY,
        scale: None,
    };

    /// Computes a scale factor for the given ppem and upem values.
    pub fn compute_scale(ppem: f32, upem: i32) -> Fixed {
        Fixed::from_bits((ppem * 64.0) as i32) / Fixed::from_bits(upem.max(1))
    }

    /// Applies the transform to a horizontal metric such as an advance
    /// width.
    pub fn transform_h_metric(&self, metric: Fixed) -> Fixed {
        let mut metric = Fixed::from_bits(metric.to_i32());
        let matrix = &self.matrix.0;
        if matrix[0] != Fixed::ONE {
            // x scale
            metric *= matrix[0];
        }
        // x translation
        metric += matrix[4];
        if let Some(scale) = self.scale {
            // Multiplying by scale converts to 26.6 but we want to keep the
            // result in 16.16
            Fixed::from_bits((metric * scale).to_bits() << 10)
        } else {
            // Metric is currently in font units. Convert back to 16.16
            Fixed::from_bits(metric.to_bits() << 16)
        }
    }
}

/// An affine matrix defining a font transform.
///
/// Components are in the order `[sx, ky, kx, sy, dx, dy]`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FontMatrix(pub [Fixed; 6]);

impl FontMatrix {
    /// The identity matrix.
    pub const IDENTITY: Self = Self([
        Fixed::ONE,
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::ONE,
        Fixed::ZERO,
        Fixed::ZERO,
    ]);

    /// Applies the matrix to the given point.
    pub fn transform(&self, x: Fixed, y: Fixed) -> (Fixed, Fixed) {
        let matrix = &self.0;
        (
            matrix[0] * x + matrix[2] * y + matrix[4],
            matrix[1] * x + matrix[3] * y + matrix[5],
        )
    }

    /// Simple fixed point matrix multiplication with a scaling factor.
    ///
    /// Note: this transforms the translation component of `other` by the upper 2x2 of
    /// `self`. This matches the offset transform FreeType uses when concatenating
    /// the matrices from the top and font dicts.
    pub fn combine_scaled(&self, other: &Self, scale: i32) -> Self {
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/base/ftcalc.c#L719>
        let a = &self.0;
        let b = &other.0;
        let val = Fixed::from_i32(scale);
        let xx = a[0].mul_div(b[0], val) + a[2].mul_div(b[1], val);
        let yx = a[1].mul_div(b[0], val) + a[3].mul_div(b[1], val);
        let xy = a[0].mul_div(b[2], val) + a[2].mul_div(b[3], val);
        let yy = a[1].mul_div(b[2], val) + a[3].mul_div(b[3], val);
        let x = b[4];
        let y = b[5];
        let dx = x.mul_div(a[0], val) + y.mul_div(a[2], val);
        let dy = x.mul_div(a[1], val) + y.mul_div(a[3], val);
        Self([xx, yx, xy, yy, dx, dy])
    }

    /// Check for a degenerate matrix.
    /// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/f1cd6dbfa0c98f352b698448f40ac27e8fb3832e/src/base/ftcalc.c#L725>
    pub(crate) fn is_degenerate(&self) -> bool {
        let [mut xx, mut yx, mut xy, mut yy, ..] = self.0.map(|x| x.to_bits() as i64);
        let val = xx.abs() | yx.abs() | xy.abs() | yy.abs();
        if val == 0 || val > 0x7FFFFFFF {
            return true;
        }
        // Scale the matrix to avoid temp1 overflow
        let msb = 32 - (val as i32).leading_zeros() - 1;
        let shift = msb as i32 - 12;
        if shift > 0 {
            xx >>= shift;
            xy >>= shift;
            yx >>= shift;
            yy >>= shift;
        }
        let temp1 = 32 * (xx * yy - xy * yx).abs();
        let temp2 = (xx * xx) + (xy * xy) + (yx * yx) + (yy * yy);
        if temp1 <= temp2 {
            return true;
        }
        false
    }
}

impl Default for FontMatrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// An affine matrix with a scaling factor.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ScaledFontMatrix {
    /// The matrix.
    pub matrix: FontMatrix,
    /// The dynamic scale factor used when parsing this matrix.
    pub scale: i32,
}

impl ScaledFontMatrix {
    /// Parses a font matrix using dynamic scaling factors.
    ///
    /// Returns the matrix and an adjusted upem factor.
    pub(crate) fn parse(cursor: &mut Cursor) -> Option<Self> {
        let mut values = [Fixed::ZERO; 6];
        let mut scalings = [0i32; 6];
        let mut max_scaling = i32::MIN;
        let mut min_scaling = i32::MAX;
        for (value, scaling) in values.iter_mut().zip(&mut scalings) {
            let (v, s) = num::parse_fixed_dynamic(cursor).ok()?;
            if v != Fixed::ZERO {
                max_scaling = max_scaling.max(s);
                min_scaling = min_scaling.min(s);
            }
            *value = v;
            *scaling = s;
        }
        if !(-9..=0).contains(&max_scaling)
            || (max_scaling - min_scaling < 0)
            || (max_scaling - min_scaling) > 9
        {
            return None;
        }
        for (value, scaling) in values.iter_mut().zip(scalings) {
            if *value == Fixed::ZERO {
                continue;
            }
            let divisor = num::BCD_POWER_TENS[(max_scaling - scaling) as usize];
            let half_divisor = divisor >> 1;
            if *value < Fixed::ZERO {
                if i32::MIN + half_divisor < value.to_bits() {
                    *value = Fixed::from_bits((value.to_bits() - half_divisor) / divisor);
                } else {
                    *value = Fixed::from_bits(i32::MIN / divisor);
                }
            } else if i32::MAX - half_divisor > value.to_bits() {
                *value = Fixed::from_bits((value.to_bits() + half_divisor) / divisor);
            } else {
                *value = Fixed::from_bits(i32::MAX / divisor);
            }
        }
        let matrix = FontMatrix(values);
        // Check for a degenerate matrix
        if matrix.is_degenerate() {
            return None;
        }
        let scale = num::BCD_POWER_TENS[(-max_scaling) as usize];
        Some(Self { matrix, scale })
    }

    /// Compute a new font matrix and UPEM scale factor where the Y scale of
    /// the matrix is 1.0.    
    #[must_use]
    pub fn normalize(&self) -> Self {
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/f1cd6dbfa0c98f352b698448f40ac27e8fb3832e/src/cff/cffobjs.c#L727>
        let mut matrix = self.matrix.0;
        let mut scaled_upem = self.scale;
        let factor = if matrix[3] != Fixed::ZERO {
            matrix[3].abs()
        } else {
            // Use yx if yy is zero
            matrix[1].abs()
        };
        if factor != Fixed::ONE {
            scaled_upem = (Fixed::from_bits(scaled_upem) / factor).to_bits();
            for value in &mut matrix {
                *value /= factor;
            }
        }
        // FT shifts off the fractional parts of the translation?
        for offset in matrix[4..6].iter_mut() {
            *offset = Fixed::from_bits(offset.to_bits() >> 16);
        }
        Self {
            matrix: FontMatrix(matrix),
            scale: scaled_upem,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_scale() {
        assert_eq!(Transform::compute_scale(1000.0, 1000).to_bits(), 64 << 16);
        assert_eq!(Transform::compute_scale(500.0, 1000).to_bits(), 32 << 16);
        assert_eq!(Transform::compute_scale(2000.0, 1000).to_bits(), 128 << 16);
        assert_eq!(Transform::compute_scale(16.0, 1000).to_bits(), 67109);
    }

    #[test]
    fn h_metric_identity_integral() {
        for metric in [Fixed::ZERO, Fixed::ONE, Fixed::NEG_ONE, Fixed::from_i32(42)] {
            assert_eq!(Transform::IDENTITY.transform_h_metric(metric), metric);
        }
    }

    #[test]
    fn h_metric_identity_fractional() {
        // Like FT, metrics are rounded to font units before applying matrix
        // and scale
        for metric in [
            Fixed::from_f64(42.5),
            Fixed::from_f64(-20.1),
            Fixed::from_f64(18.8),
        ] {
            assert_eq!(
                Transform::IDENTITY.transform_h_metric(metric),
                metric.round()
            );
        }
    }

    #[test]
    fn h_metric_matrix_scale() {
        let transform = Transform {
            matrix: FontMatrix([2.0, 0.0, 0.0, 1.0, 0.0, 0.0].map(Fixed::from_f64)),
            scale: None,
        };
        // metric.round() * 2
        let pairs = [(42.5, 86.0), (-20.1, -40.0), (18.8, 38.0)];
        for (metric, transformed_metric) in pairs {
            assert_eq!(
                transform
                    .transform_h_metric(Fixed::from_f64(metric))
                    .to_f64(),
                transformed_metric
            );
        }
    }

    #[test]
    fn h_metric_matrix_scale_offset() {
        let transform = Transform {
            matrix: FontMatrix([2.0, 0.0, 0.0, 1.0, 10.0 / 65536.0, 0.0].map(Fixed::from_f64)),
            scale: None,
        };
        // metric.round() * 2 + 10
        let pairs = [(42.5, 96.0), (-20.1, -30.0), (18.8, 48.0)];
        for (metric, transformed_metric) in pairs {
            assert_eq!(
                transform
                    .transform_h_metric(Fixed::from_f64(metric))
                    .to_f64(),
                transformed_metric
            );
        }
    }

    #[test]
    fn h_metric_scale() {
        let transform = Transform {
            matrix: FontMatrix::IDENTITY,
            // Scale by 0.5
            scale: Some(Fixed::from_i32(32)),
        };
        // metric.round() / 2
        let pairs = [(42.5, 21.5), (-20.1, -10.0), (18.8, 9.5)];
        for (metric, transformed_metric) in pairs {
            assert_eq!(
                transform
                    .transform_h_metric(Fixed::from_f64(metric))
                    .to_f64(),
                transformed_metric
            );
        }
    }

    #[test]
    fn h_metric_scale_matrix_scale_offset() {
        let transform = Transform {
            matrix: FontMatrix([4.0, 0.0, 0.0, 1.0, 10.0 / 65536.0, 0.0].map(Fixed::from_f64)),
            // Scale by 0.5
            scale: Some(Fixed::from_i32(32)),
        };
        // (metric.round() * 4 + 10) / 2
        let pairs = [(42.5, 91.0), (-20.1, -35.0), (18.8, 43.0)];
        for (metric, transformed_metric) in pairs {
            assert_eq!(
                transform
                    .transform_h_metric(Fixed::from_f64(metric))
                    .to_f64(),
                transformed_metric
            );
        }
    }

    /// See <https://github.com/googlefonts/fontations/issues/1595>
    #[test]
    fn degenerate_matrix_check_doesnt_overflow() {
        // Values taken from font in the above issue
        let matrix = FontMatrix([
            Fixed::from_bits(639999672),
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_bits(639999672),
            Fixed::ZERO,
            Fixed::ZERO,
        ]);
        // Just don't panic with overflow
        matrix.is_degenerate();
        // Try again with all max values
        FontMatrix([Fixed::MAX; 6]).is_degenerate();
        // And all min values
        FontMatrix([Fixed::MIN; 6]).is_degenerate();
    }

    #[test]
    fn normalize_matrix() {
        // This matrix has a y scale of 0.5 so we should produce a new matrix
        // with a y scale of 1.0 and a scale factor of 2
        let matrix = ScaledFontMatrix {
            matrix: FontMatrix([65536, 0, 0, 32768, 0, 0].map(Fixed::from_bits)),
            scale: 1,
        };
        let normalized = matrix.normalize();
        let expected_normalized = [131072, 0, 0, 65536, 0, 0].map(Fixed::from_bits);
        assert_eq!(normalized.matrix.0, expected_normalized);
        assert_eq!(normalized.scale, 2);
    }

    #[test]
    fn combine_matrix() {
        let a = [0.5, 0.75, -1.0, 2.0, 0.0, 0.0].map(Fixed::from_f64);
        let b = [1.5, -1.0, 0.25, -1.0, 1.0, 2.0].map(Fixed::from_f64);
        let expected = [1.75, -0.875, 1.125, -1.8125, -1.5, 4.75].map(Fixed::from_f64);
        let result = FontMatrix(a).combine_scaled(&FontMatrix(b), 1);
        assert_eq!(result.0, expected);
    }
}
