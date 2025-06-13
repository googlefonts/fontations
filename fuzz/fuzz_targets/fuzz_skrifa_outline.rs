#![no_main]
use std::{error::Error, fmt::Display};

use libfuzzer_sys::fuzz_target;
use skrifa::{
    instance::{Location, Size},
    outline::{
        pen::PathStyle, DrawError, DrawSettings, HintingInstance, HintingMode, InterpreterVersion,
        LcdLayout, OutlinePen,
    },
    FontRef, MetadataProvider,
};

mod helpers;
use helpers::*;

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

/// Drawing glyph outlines is fun and flexible! Try to test lots of options.
///
/// See
/// * <https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html>
/// * <https://docs.rs/skrifa/latest/skrifa/outline/index.html>
#[derive(Debug, Clone)]
struct OutlineRequest {
    /// None => unscaled
    size: Size,
    location: Location,
    hinted: bool,
    hinted_pedantic: bool,
    hinting_mode: HintingMode,
    interpreter_version: InterpreterVersion,
    with_memory: bool, // ~half tests should be with memory, half not
    memory_size: u16, // if we do test with_memory, how much of it? u16 to avoid asking for huge chunks.
    harfbuzz_pathstyle: bool,
}

impl Default for OutlineRequest {
    fn default() -> Self {
        Self {
            size: Size::unscaled(),
            location: Default::default(),
            hinted: Default::default(),
            hinted_pedantic: Default::default(),
            hinting_mode: Default::default(),
            interpreter_version: Default::default(),
            with_memory: Default::default(),
            memory_size: Default::default(),
            harfbuzz_pathstyle: Default::default(),
        }
    }
}

#[derive(Debug)]
struct DrawErrorWrapper(DrawError);

impl Error for DrawErrorWrapper {}

impl Display for DrawErrorWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

fn do_glyf_things(outline_request: OutlineRequest, font: &FontRef) -> Result<(), Box<dyn Error>> {
    let outlines = font.outline_glyphs();
    let size = outline_request.size;
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
                &outline_request.location,
                outline_request.hinting_mode,
                outline_request.interpreter_version,
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
            DrawSettings::unhinted(size, &outline_request.location)
        };
        if outline_request.with_memory {
            settings = settings.with_memory(Some(&mut buf));
        }
        if outline_request.harfbuzz_pathstyle {
            settings = settings.with_path_style(PathStyle::HarfBuzz);
        }

        let _ = glyph.draw(settings, &mut NopPen {});
    }

    Ok(())
}

fn hinting_modes(hinted: bool) -> Vec<HintingMode> {
    if !hinted {
        return vec![HintingMode::default()];
    }
    vec![
        HintingMode::Strong,
        HintingMode::Smooth {
            lcd_subpixel: None,
            preserve_linear_metrics: true,
        },
        HintingMode::Smooth {
            lcd_subpixel: None,
            preserve_linear_metrics: false,
        },
        HintingMode::Smooth {
            lcd_subpixel: Some(LcdLayout::Horizontal),
            preserve_linear_metrics: true,
        },
        HintingMode::Smooth {
            lcd_subpixel: Some(LcdLayout::Horizontal),
            preserve_linear_metrics: false,
        },
        HintingMode::Smooth {
            lcd_subpixel: Some(LcdLayout::Vertical),
            preserve_linear_metrics: true,
        },
        HintingMode::Smooth {
            lcd_subpixel: Some(LcdLayout::Vertical),
            preserve_linear_metrics: false,
        },
    ]
}

fn interpreter_versions(hinted: bool) -> Vec<InterpreterVersion> {
    if !hinted {
        return vec![InterpreterVersion::_40];
    }
    vec![InterpreterVersion::_35, InterpreterVersion::_40]
}

// To avoid consuming corpus data that is likely a font just build the cross product of likely values
// for various options
fn create_request_scenarios(font: &FontRef) -> Vec<OutlineRequest> {
    fuzz_sizes()
        .into_iter()
        .map(move |size| OutlineRequest {
            size,
            ..Default::default()
        })
        .flat_map(|r| {
            fuzz_locations(font)
                .into_iter()
                .map(move |location| OutlineRequest {
                    location,
                    ..r.clone()
                })
        })
        .flat_map(|r| {
            [true, false].into_iter().map(move |hinted| OutlineRequest {
                hinted,
                ..r.clone()
            })
        })
        .flat_map(|r| {
            hinting_modes(r.hinted)
                .into_iter()
                .map(move |hinting_mode| OutlineRequest {
                    hinting_mode,
                    ..r.clone()
                })
        })
        .flat_map(|r| {
            interpreter_versions(r.hinted)
                .into_iter()
                .map(move |interpreter_version| OutlineRequest {
                    interpreter_version,
                    ..r.clone()
                })
        })
        .flat_map(|r| {
            [true, false]
                .into_iter()
                .map(move |with_memory| OutlineRequest {
                    with_memory,
                    ..r.clone()
                })
        })
        .flat_map(|r| {
            if r.with_memory {
                vec![1024, 4096, 16384]
            } else {
                vec![0]
            }
            .into_iter()
            .map(move |memory_size| OutlineRequest {
                memory_size,
                ..r.clone()
            })
        })
        .collect::<Vec<_>>()
}

fuzz_target!(|data: &[u8]| {
    let Ok(font) = select_font(data) else {
        return;
    };
    for request in create_request_scenarios(&font) {
        let _ = do_glyf_things(request, &font);
    }
});
