//! Common helpers

use skrifa::{
    instance::{Location, Size},
    FontRef, MetadataProvider,
};

const AXIS_LIMIT: usize = 5; // 3 options per axis * up to 5 axes

pub(crate) fn fuzz_sizes() -> Vec<Size> {
    vec![Size::unscaled(), Size::new(64.0), Size::new(512.0)]
}

pub(crate) fn fuzz_locations(font: &FontRef) -> Vec<Location> {
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
