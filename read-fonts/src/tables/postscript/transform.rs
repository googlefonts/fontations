//! PostScript specific transform.

use super::dict::FontMatrix;
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
}
