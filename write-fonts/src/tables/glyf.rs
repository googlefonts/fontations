//! The [glyf (Glyph Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf) table

use crate::{FontWrite, OtRound};

use kurbo::Rect;

mod composite;
mod simple;

pub use composite::{Anchor, Component, ComponentFlags, CompositeGlyph, Transform};
pub use simple::{simple_glyphs_from_kurbo, BadKurbo, Contour, SimpleGlyph};

/// A Bounding box.
///
/// This should be the minimum rectangle which fully encloses the glyph outline;
/// importantly this can only be determined by computing the individual Bezier
/// segments, and cannot be determiend from points alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Bbox {
    pub x_min: i16,
    pub y_min: i16,
    pub x_max: i16,
    pub y_max: i16,
}

impl Bbox {
    pub fn union(self, other: Bbox) -> Bbox {
        Bbox {
            x_min: self.x_min.min(other.x_min),
            y_min: self.y_min.min(other.y_min),
            x_max: self.x_max.max(other.x_max),
            y_max: self.y_max.max(other.y_max),
        }
    }
}

impl From<Rect> for Bbox {
    fn from(value: Rect) -> Self {
        Bbox {
            x_min: value.min_x().ot_round(),
            y_min: value.min_y().ot_round(),
            x_max: value.max_x().ot_round(),
            y_max: value.max_y().ot_round(),
        }
    }
}

impl FontWrite for Bbox {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        let Bbox {
            x_min,
            y_min,
            x_max,
            y_max,
        } = *self;
        [x_min, y_min, x_max, y_max].write_into(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_box() {
        assert_eq!(
            Bbox {
                x_min: -1,
                y_min: -2,
                x_max: 5,
                y_max: 6
            },
            Bbox {
                x_min: 0,
                y_min: 0,
                x_max: 5,
                y_max: 6
            }
            .union(Bbox {
                x_min: -1,
                y_min: -2,
                x_max: 3,
                y_max: 4
            })
        )
    }
}
