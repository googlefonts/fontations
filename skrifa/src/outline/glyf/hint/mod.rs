//! TrueType hinting.

mod call_stack;
mod code;
mod cow_slice;
mod engine;
mod error;
mod graphics_state;
mod math;
mod value_stack;

use read_fonts::{
    tables::glyf::PointFlags,
    types::{F26Dot6, F2Dot14, Fixed, Point},
};

use crate::outline::{EmbeddedHinting, LcdLayout};

use super::Outlines;

pub use call_stack::{CallRecord, CallStack};
pub use code::{CodeDefinition, CodeDefinitionSlice, Decoder};
pub use cow_slice::{CowSlice, Cvt, Storage};
pub use engine::Engine;
pub use error::{HintError, HintErrorKind};
pub use graphics_state::{
    CoordAxis, GraphicsState, RetainedGraphicsState, RoundMode, RoundState, Zone, ZonePointer,
};
pub use value_stack::ValueStack;

/// The persistent instance state.
///
/// Captures interpreter state that is set by the control value program.
#[derive(Copy, Clone, Default, Debug)]
pub struct InstanceState {
    pub graphics: graphics_state::RetainedGraphicsState,
    pub ppem: u16,
    pub scale: i32,
    pub compat: bool,
    pub mode: EmbeddedHinting,
}

impl InstanceState {
    /// Returns true if hinting is enabled for this state.
    pub fn hinting_enabled(&self) -> bool {
        self.graphics.instruct_control & 1 == 0
    }

    /// Returns true if compatibility mode is enabled for this state.
    pub fn compat_enabled(&self) -> bool {
        self.compat
    }
}

/// Outline data that is passed to the hinter.
pub struct HintOutline<'a> {
    pub unscaled: &'a mut [Point<i32>],
    pub scaled: &'a mut [Point<F26Dot6>],
    pub original_scaled: &'a mut [Point<i32>],
    pub flags: &'a mut [PointFlags],
    pub contours: &'a [u16],
    pub phantom: &'a mut [Point<F26Dot6>],
    pub bytecode: &'a [u8],
    pub is_composite: bool,
    pub coords: &'a [F2Dot14],
}

#[derive(Clone, Default)]
pub struct HintInstance {
    functions: Vec<CodeDefinition>,
    instructions: Vec<CodeDefinition>,
    cvt: Vec<i32>,
    storage: Vec<i32>,
    axis_count: u16,
    max_stack: u16,
    max_twilight_points: u16,
    state: InstanceState,
}

impl HintInstance {
    pub fn reconfigure(
        &mut self,
        outlines: &Outlines,
        scale: i32,
        ppem: u16,
        mode: EmbeddedHinting,
        coords: &[F2Dot14],
    ) -> Result<(), HintError> {
        self.setup(outlines, scale);
        let twilight_len = self.max_twilight_points as usize;
        // Temporary buffers. For now just allocate them.
        let mut stack_buffer = vec![0; self.max_stack as usize];
        let mut twilight_unscaled = vec![Point::default(); twilight_len];
        let mut twilight_original = vec![Point::default(); twilight_len];
        let mut twilight_points = vec![Point::default(); twilight_len];
        let mut twilight_flags = vec![PointFlags::default(); twilight_len];
        let twilight_contours = [self.max_twilight_points];
        let twilight_zone = Zone::new(
            &mut twilight_unscaled,
            &mut twilight_original,
            &mut twilight_points,
            &mut twilight_flags,
            &twilight_contours,
        );
        let glyph_zone = Zone::default();
        let stack = ValueStack::new(&mut stack_buffer);
        let mut engine = Engine::new(
            stack,
            CowSlice::new_mut(&mut self.storage),
            CowSlice::new_mut(&mut self.cvt),
            CodeDefinitionSlice::Mut(&mut self.functions),
            CodeDefinitionSlice::Mut(&mut self.instructions),
            twilight_zone,
            glyph_zone,
            coords,
            self.axis_count,
        );
        if !outlines.fpgm.is_empty() {
            engine.run_font_program(&mut self.state, outlines.fpgm)?;
        }
        if !outlines.prep.is_empty() {
            engine.run_cv_program(
                &mut self.state,
                mode,
                outlines.fpgm,
                outlines.prep,
                ppem,
                scale,
            );
        }
        Ok(())
    }

    pub fn is_hinting_disabled(&self) -> bool {
        self.state.graphics.instruct_control & 1 != 0
    }

    pub fn hint(&self, outlines: &Outlines, outline: &mut HintOutline) -> Option<bool> {
        let twilight_len = self.max_twilight_points as usize;
        // Temporary buffers. For now just allocate them.
        let mut stack_buffer = vec![0; self.max_stack as usize];
        let mut twilight_unscaled = vec![Point::default(); twilight_len];
        let mut twilight_original = vec![Point::default(); twilight_len];
        let mut twilight_points = vec![Point::default(); twilight_len];
        let mut twilight_flags = vec![PointFlags::default(); twilight_len];
        let twilight_contours = [self.max_twilight_points];
        let twilight_zone = Zone::new(
            &mut twilight_unscaled,
            &mut twilight_original,
            &mut twilight_points,
            &mut twilight_flags,
            &twilight_contours,
        );
        // SAFETY: We're casting &mut [Point<F26Dot6>] to &mut [Point<i32>].
        // The Point type is repr(C) and contains two fields with no padding.
        // The component types are bit layout equivalent.
        let scaled = unsafe {
            let slice = core::slice::from_raw_parts_mut(
                outline.scaled.as_mut_ptr() as *mut _,
                outline.scaled.len(),
            );
            // Set outline.scaled to an emtpy slice to avoid the potential
            // for mutable aliasing.
            outline.scaled = &mut [];
            slice
        };
        let glyph_zone = Zone::new(
            outline.unscaled,
            outline.original_scaled,
            scaled,
            outline.flags,
            outline.contours,
        );
        let stack = ValueStack::new(&mut stack_buffer);
        let mut storage = vec![0i32; self.storage.len()];
        let mut cvt = vec![0i32; self.cvt.len()];
        let mut interp = Engine::new(
            stack,
            CowSlice::new(&self.storage, &mut storage),
            CowSlice::new(&self.cvt, &mut cvt),
            CodeDefinitionSlice::Ref(&self.functions),
            CodeDefinitionSlice::Ref(&self.instructions),
            twilight_zone,
            glyph_zone,
            outline.coords,
            self.axis_count,
        );
        let mut instance_state = self.state;
        let result = interp.run_glyph_program(
            &mut instance_state,
            outlines.fpgm,
            outlines.prep,
            outline.bytecode,
            outline.is_composite,
        );
        if !self.state.compat_enabled() {
            for (i, p) in (scaled[scaled.len() - 4..]).iter().enumerate() {
                outline.phantom[i] = p.map(F26Dot6::from_bits);
            }
        }
        Some(result)
    }

    /// Captures limits, resizes buffers and scales the CVT.
    fn setup(&mut self, outlines: &Outlines, scale: i32) {
        self.axis_count = outlines
            .gvar
            .as_ref()
            .map(|gvar| gvar.axis_count())
            .unwrap_or_default();
        self.functions.clear();
        self.functions.resize(
            outlines.max_function_defs as usize,
            CodeDefinition::default(),
        );
        self.instructions.resize(
            outlines.max_instruction_defs as usize,
            CodeDefinition::default(),
        );
        self.cvt.clear();
        let scale = Fixed::from_bits(scale >> 6);
        self.cvt.extend(
            outlines
                .cvt
                .iter()
                .map(|value| (Fixed::from_bits(value.get() as i32 * 64) * scale).to_bits()),
        );
        self.storage.clear();
        self.storage.resize(outlines.max_storage as usize, 0);
        // Add 4 for phantom points
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttobjs.c#L1188>
        self.max_twilight_points = outlines.max_twilight_points + 4;
        // Add 32 to match FreeType's heuristic for buggy fonts
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/truetype/ttinterp.c#L356>
        self.max_stack = outlines.max_stack_elements + 32;
        self.state = InstanceState::default();
    }
}

impl EmbeddedHinting {
    fn is_grayscale(&self) -> bool {
        false
    }

    fn is_aliased(&self) -> bool {
        matches!(self, Self::Aliased)
    }

    fn is_antialiased(&self) -> bool {
        matches!(self, Self::AntiAliased { .. })
    }

    fn is_grayscale_cleartype(&self) -> bool {
        matches!(
            self,
            Self::AntiAliased {
                lcd_subpixel: None,
                ..
            }
        )
    }

    fn is_vertical_lcd(&self) -> bool {
        matches!(
            self,
            Self::AntiAliased {
                lcd_subpixel: Some(LcdLayout::Vertical),
                ..
            }
        )
    }

    fn retain_linear_metrics(&self) -> bool {
        matches!(
            self,
            Self::AntiAliased {
                retain_linear_metrics: true,
                ..
            }
        )
    }
}
