//! Common helpers

use read_fonts::{FileRef, ReadError};
use skrifa::{
    instance::{Location, Size},
    FontRef, MetadataProvider,
};

// We use allow dead here because the many-binary fuzzer rigging really likes to complain things are never used

#[allow(dead_code)]
const AXIS_LIMIT: usize = 5; // 3 options per axis * up to 5 axes

#[allow(dead_code)]
pub fn fuzz_sizes() -> Vec<Size> {
    vec![Size::unscaled(), Size::new(64.0), Size::new(512.0)]
}

#[allow(dead_code)]
pub fn fuzz_locations(font: &FontRef) -> Vec<Location> {
    let axes = font.axes();
    let mut locations = vec![Vec::new()];

    // Cross unique min,default,max on every axis
    for axis in axes.iter().take(AXIS_LIMIT) {
        let mut values = vec![axis.default_value()];
        if axis.min_value() != axis.default_value() {
            values.push(axis.min_value());
        }
        if axis.max_value() != axis.default_value() {
            values.push(axis.max_value());
        }
        locations = locations
            .into_iter()
            .flat_map(|l| {
                values.iter().map(move |v| {
                    let mut l = l.clone();
                    l.push(*v);
                    l
                })
            })
            .collect();
    }

    locations
        .iter()
        .map(|positions| create_location(font, positions))
        .collect()
}

fn create_location(font: &FontRef, axis_positions: &[f32]) -> Location {
    let raw_location = font
        .axes()
        .iter()
        .zip(axis_positions)
        .map(|(axis, pos)| (axis.tag(), *pos))
        .collect::<Vec<_>>();
    font.axes().location(raw_location)
}

/// Makes fuzzing agnostic of collection and non-collection inputs
///
/// Picks a single font if data is a collection.
pub(crate) fn select_font(data: &[u8]) -> Result<FontRef<'_>, ReadError> {
    // Take the last byte as the collection index to let the fuzzer guide
    let i = data.last().copied().unwrap_or_default();
    match FileRef::new(data)? {
        FileRef::Collection(cr) => {
            let _ = cr.len();
            let _ = cr.is_empty();
            let _ = cr.iter().count();
            cr.get(i.into())
        }
        FileRef::Font(f) => Ok(f),
    }
}
