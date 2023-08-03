//! The [glyf (Glyph Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf) table

use crate::{
    from_obj::{FromObjRef, FromTableRef},
    validate::{Validate, ValidationCtx},
    FontWrite, OtRound, TableWriter,
};

use font_types::Tag;
use kurbo::Rect;
use read_fonts::{FontRead, TopLevelTable};

mod composite;
mod simple;

pub use composite::{Anchor, Component, ComponentFlags, CompositeGlyph, Transform};
pub use simple::{Contour, MalformedPath, SimpleGlyph};

/// The [glyf (Glyph Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/glyf) table
///
/// Compiling the glyf table requires additional logic, since the positions
/// of glyfs are stored in the 'loca' type.
pub struct Glyf(Vec<u8>);

impl TopLevelTable for Glyf {
    /// 'glyf'
    const TAG: Tag = Tag::new(b"glyf");
}

/// A simple or composite glyph
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Glyph {
    Simple(SimpleGlyph),
    Composite(CompositeGlyph),
}

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

impl Glyph {
    /// The bounding box for the glyph
    pub fn bbox(&self) -> Bbox {
        match self {
            Glyph::Simple(glyph) => glyph.bbox,
            Glyph::Composite(glyph) => glyph.bbox,
        }
    }

    /// 'true' if the glyph contains no contours or components
    pub fn is_empty(&self) -> bool {
        match self {
            Glyph::Simple(glyph) => glyph.contours().is_empty(),
            Glyph::Composite(glyph) => glyph.components().is_empty(),
        }
    }
}

impl Bbox {
    /// Return the smallest bounding box covering `self` and `other`
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

impl<'a> FromObjRef<read_fonts::tables::glyf::Glyph<'a>> for Glyph {
    fn from_obj_ref(
        from: &read_fonts::tables::glyf::Glyph<'a>,
        data: read_fonts::FontData,
    ) -> Self {
        match from {
            read_fonts::tables::glyf::Glyph::Simple(glyph) => {
                Self::Simple(SimpleGlyph::from_obj_ref(glyph, data))
            }
            read_fonts::tables::glyf::Glyph::Composite(glyph) => {
                Self::Composite(CompositeGlyph::from_obj_ref(glyph, data))
            }
        }
    }
}

impl<'a> FromTableRef<read_fonts::tables::glyf::Glyph<'a>> for Glyph {}

impl<'a> FontRead<'a> for Glyph {
    fn read(data: read_fonts::FontData<'a>) -> Result<Self, read_fonts::ReadError> {
        read_fonts::tables::glyf::Glyph::read(data).map(|g| Glyph::from_table_ref(&g))
    }
}

impl From<SimpleGlyph> for Glyph {
    fn from(value: SimpleGlyph) -> Self {
        Glyph::Simple(value)
    }
}

impl From<CompositeGlyph> for Glyph {
    fn from(value: CompositeGlyph) -> Self {
        Glyph::Composite(value)
    }
}

impl Validate for Glyph {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        match self {
            Glyph::Simple(glyph) => glyph.validate_impl(ctx),
            Glyph::Composite(glyph) => glyph.validate_impl(ctx),
        }
    }
}

impl FontWrite for Glyph {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        match self {
            Glyph::Simple(glyph) => glyph.write_into(writer),
            Glyph::Composite(glyph) => glyph.write_into(writer),
        }
    }
}

impl Validate for Glyf {
    fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
}

impl FontWrite for Glyf {
    fn write_into(&self, writer: &mut TableWriter) {
        writer.write_slice(&self.0)
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
