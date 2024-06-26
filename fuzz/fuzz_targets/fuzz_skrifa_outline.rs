#![no_main]
use std::{error::Error, fmt::Display};

use libfuzzer_sys::{
    arbitrary::{self, Arbitrary, Unstructured},
    fuzz_target,
};
use skrifa::{
    instance::Size,
    outline::{DrawError, DrawSettings, HintingInstance, HintingMode, LcdLayout, OutlinePen},
    raw::{tables::glyf::ToPathStyle, TableProvider},
    FontRef, MetadataProvider,
};

/// The pen for when you don't really care what gets drawn
struct NopPen;

impl OutlinePen for NopPen {
    fn move_to(&mut self, _x: f32, _y: f32) {
        // nop
    }

    fn line_to(&mut self, _x: f32, _y: f32) {
        // nop
    }

    fn quad_to(&mut self, _cx0: f32, _cy0: f32, _x: f32, _y: f32) {
        // nop
    }

    fn curve_to(&mut self, _cx0: f32, _cy0: f32, _cx1: f32, _cy1: f32, _x: f32, _y: f32) {
        // nop
    }

    fn close(&mut self) {
        // nop
    }
}

/// Each entry represents a set of options for [HintingMode]
///
/// Exists to fulfill [Arbitrary]
#[derive(Arbitrary, Debug)]
enum FuzzerHintingMode {
    Strong,
    Smooth {
        vertical_lcd: Option<bool>,
        preserve_linear_metrics: bool,
    },
}

impl From<FuzzerHintingMode> for HintingMode {
    fn from(value: FuzzerHintingMode) -> Self {
        match value {
            FuzzerHintingMode::Strong => HintingMode::Strong,
            FuzzerHintingMode::Smooth {
                vertical_lcd,
                preserve_linear_metrics,
            } => HintingMode::Smooth {
                lcd_subpixel: match vertical_lcd {
                    None => None,
                    Some(true) => Some(LcdLayout::Vertical),
                    Some(false) => Some(LcdLayout::Horizontal),
                },
                preserve_linear_metrics,
            },
        }
    }
}

/// Drawing glyph outlines is fun and flexible! Try to test lots of options.
///
/// See
/// * <https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html>
/// * <https://docs.rs/skrifa/latest/skrifa/outline/index.html>
#[derive(Arbitrary, Debug)]
struct OutlineRequest {
    /// None => unscaled
    size: Option<f32>,
    axis_positions: Vec<f32>,
    hinted: bool,
    hinted_pedantic: bool,
    hinting_mode: FuzzerHintingMode,
    with_memory: bool, // ~half tests should be with memory, half not
    memory_size: u16, // if we do test with_memory, how much of it? u16 to avoid asking for huge chunks.
    harfbuzz_pathstyle: bool,
}

#[derive(Debug)]
struct DrawErrorWrapper(DrawError);

impl Error for DrawErrorWrapper {}

impl Display for DrawErrorWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

fn do_glyf_things(data: &[u8]) -> Result<(), Box<dyn Error>> {
    let font = FontRef::new(data)?;

    // we use the bytes of the os2 table, (which are otherwise irrelevant in this test case)
    // to construct the `OutlineRequest`; this lets the fuzzer modify the request by
    // mutating those bytes
    let os2 = font.os2()?;
    let os2bytes = os2.offset_data();
    let mut unstructured = Unstructured::new(os2bytes.as_bytes());
    let outline_request: OutlineRequest = unstructured.arbitrary()?;

    let outlines = font.outline_glyphs();
    let size = outline_request
        .size
        .map(Size::new)
        .unwrap_or_else(Size::unscaled);
    let raw_location = font
        .axes()
        .iter()
        .zip(&outline_request.axis_positions)
        .map(|(axis, pos)| (axis.tag(), *pos))
        .collect::<Vec<_>>();
    let location = font.axes().location(raw_location);
    let mut buf: Vec<u8> = Vec::with_capacity(
        outline_request
            .with_memory
            .then_some(outline_request.memory_size)
            .unwrap_or_default() as usize,
    );
    let hinting_instance = if outline_request.hinted {
        Some(
            HintingInstance::new(
                &outlines,
                size,
                &location,
                outline_request.hinting_mode.into(),
            )
            .map_err(DrawErrorWrapper)?,
        )
    } else {
        None
    };

    for (_codepoint, gid) in font.charmap().mappings() {
        let Some(glyph) = outlines.get(gid) else {
            continue;
        };

        let mut settings = if let Some(instance) = &hinting_instance {
            DrawSettings::hinted(instance, outline_request.hinted_pedantic)
        } else {
            DrawSettings::unhinted(size, &location)
        };
        if outline_request.with_memory {
            settings = settings.with_memory(Some(&mut buf));
        }
        if outline_request.harfbuzz_pathstyle {
            settings = settings.with_path_style(ToPathStyle::HarfBuzz);
        }

        let _ = glyph.draw(settings, &mut NopPen {});
    }

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = do_glyf_things(data);
});
