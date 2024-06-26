#![no_main]
use std::{error::Error, fmt::Display};

use libfuzzer_sys::fuzz_target;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use skrifa::{
    instance::Size,
    outline::{DrawError, DrawSettings, HintingInstance, HintingMode, LcdLayout, OutlinePen},
    raw::tables::glyf::ToPathStyle,
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

/// Drawing glyph outlines is fun and flexible! Try to test lots of options.
///
/// See
/// * <https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html>
/// * <https://docs.rs/skrifa/latest/skrifa/outline/index.html>
#[derive(Default, Debug, Clone)]
struct OutlineRequest {
    /// None => unscaled
    size: Option<f32>,
    axis_positions: Vec<f32>,
    hinted: bool,
    hinted_pedantic: bool,
    hinting_mode: HintingMode,
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

fn do_glyf_things(outline_request: OutlineRequest, data: &[u8]) -> Result<(), Box<dyn Error>> {
    let font = FontRef::new(data)?;
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
            HintingInstance::new(&outlines, size, &location, outline_request.hinting_mode)
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

fn create_rng(data: &[u8]) -> ChaCha8Rng {
    let mut seed = [0u8; 32];
    for (i, entry) in seed.iter_mut().enumerate() {
        *entry = data.get(i).copied().unwrap_or_default();
    }
    ChaCha8Rng::from_seed(seed)
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

fuzz_target!(|data: &[u8]| {
    // data from corpus is likely to be a font. If we chope off the head to make an outline request
    // it is likely data is no longer a font. So, take the cross product of likely values for various options
    // If a lot of values are possible choose randomly with rng seeded from data to ensure reproducible results.
    let mut rng = create_rng(data);
    let random_position = (0..8)
        .map(|_| rng.gen_range(-1000.0..1000.0))
        .collect::<Vec<_>>();
    let memory_sizes = vec![4096, 16384, rng.gen::<u16>(), rng.gen::<u16>()];

    let mut requests = vec![OutlineRequest::default()];
    requests = requests
        .into_iter()
        .flat_map(|r| {
            [None, Some(64.0), Some(512.0)]
                .into_iter()
                .map(move |size| OutlineRequest { size, ..r.clone() })
        })
        .flat_map(|r| {
            vec![
                // default
                (0..8).map(|_| 0.0).collect(),
                // random
                random_position.clone(),
            ]
            .into_iter()
            .map(move |axis_positions| OutlineRequest {
                axis_positions,
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
            [true, false]
                .into_iter()
                .map(move |with_memory| OutlineRequest {
                    with_memory,
                    ..r.clone()
                })
        })
        .flat_map(|r| {
            if r.with_memory {
                memory_sizes.clone()
            } else {
                vec![0]
            }
            .into_iter()
            .map(move |memory_size| OutlineRequest {
                memory_size,
                ..r.clone()
            })
        })
        .collect();

    for request in requests {
        let _ = do_glyf_things(request, data);
    }
});
