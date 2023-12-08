//! TrueType hinting.

mod call_stack;
mod code;
mod engine;
mod error;
mod graphics;
mod math;
mod value_stack;

use read_fonts::{
    tables::glyf::PointFlags,
    types::{F26Dot6, F2Dot14, Fixed, Point},
};

use crate::scale::Hinting;

use super::Outlines;

use self::engine::{CvtOrStorage, Engine, MaybeMut};

pub use call_stack::{CallRecord, CallStack};
pub use code::{CodeDefinition, DecodeError, Decoder};
pub use error::{HintError, HintErrorKind};
pub use graphics::{
    CoordAxis, GraphicsState, RetainedGraphicsState, RoundMode, RoundState, Zone, ZoneData,
};
pub use value_stack::ValueStack;

/// The persistent instance state.
///
/// Captures interpreter state that is set by the control value program.
#[derive(Copy, Clone, Debug)]
pub struct InstanceState {
    pub graphics: graphics::RetainedGraphicsState,
    pub ppem: u16,
    pub scale: i32,
    pub compat: bool,
    pub mode: Hinting,
}

impl Default for InstanceState {
    fn default() -> Self {
        Self {
            graphics: Default::default(),
            ppem: 0,
            scale: 0,
            compat: false,
            mode: Hinting::VerticalSubpixel,
        }
    }
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
pub struct HinterOutline<'a> {
    pub unscaled: &'a mut [Point<i32>],
    pub scaled: &'a mut [Point<F26Dot6>],
    pub original_scaled: &'a mut [Point<F26Dot6>],
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
    pub fn init(
        &mut self,
        outlines: &Outlines,
        scale: i32,
        ppem: u16,
        mode: Hinting,
        coords: &[F2Dot14],
    ) -> Option<()> {
        self.setup_state(outlines, scale)?;
        let twilight_len = self.max_twilight_points as usize;
        // Temporary buffers. For now just allocate them.
        let mut stack_buffer = vec![0; self.max_stack as usize];
        let mut twilight_unscaled = vec![Point::default(); twilight_len];
        let mut twilight_original = vec![Point::default(); twilight_len];
        let mut twilight_points = vec![Point::default(); twilight_len];
        let mut twilight_flags = vec![PointFlags::default(); twilight_len];
        let twilight_contours = [self.max_twilight_points];
        let twilight_zone = ZoneData::new(
            &mut twilight_unscaled,
            &mut twilight_original,
            &mut twilight_points,
            &mut twilight_flags,
            &twilight_contours,
        );
        let glyph_zone = ZoneData::new(&mut [], &mut [], &mut [], &mut [], &[]);
        let stack = ValueStack::new(&mut stack_buffer);
        let mut interp = Engine::new(
            stack,
            CvtOrStorage::new_mut(&mut self.storage),
            CvtOrStorage::new_mut(&mut self.cvt),
            MaybeMut::Mut(&mut self.functions),
            MaybeMut::Mut(&mut self.instructions),
            twilight_zone,
            glyph_zone,
            coords,
            self.axis_count,
        );
        if !outlines.fpgm.is_empty() {
            interp.run_fpgm(&mut self.state, outlines.fpgm);
        }
        if !outlines.prep.is_empty() {
            interp.run_prep(
                &mut self.state,
                mode,
                outlines.fpgm,
                outlines.prep,
                ppem,
                scale,
            );
        }
        Some(())
    }

    pub fn is_hinting_disabled(&self) -> bool {
        self.state.graphics.instruct_control & 1 != 0
    }

    pub fn hint(&self, outlines: &Outlines, glyph: &mut HinterOutline) -> Option<bool> {
        let twilight_len = self.max_twilight_points as usize;
        // Temporary buffers. For now just allocate them.
        let mut stack_buffer = vec![0; self.max_stack as usize];
        let mut twilight_unscaled = vec![Point::default(); twilight_len];
        let mut twilight_original = vec![Point::default(); twilight_len];
        let mut twilight_points = vec![Point::default(); twilight_len];
        let mut twilight_flags = vec![PointFlags::default(); twilight_len];
        let twilight_contours = [self.max_twilight_points];
        let twilight_zone = ZoneData::new(
            &mut twilight_unscaled,
            &mut twilight_original,
            &mut twilight_points,
            &mut twilight_flags,
            &twilight_contours,
        );
        let (scaled, original, phantom) = unsafe {
            use core::slice::from_raw_parts_mut;
            (
                from_raw_parts_mut(glyph.scaled.as_mut_ptr() as *mut _, glyph.scaled.len()),
                from_raw_parts_mut(
                    glyph.original_scaled.as_mut_ptr() as *mut _,
                    glyph.original_scaled.len(),
                ),
                from_raw_parts_mut(glyph.phantom.as_mut_ptr() as *mut _, glyph.phantom.len()),
            )
        };
        let glyph_zone = ZoneData::new(
            glyph.unscaled,
            original,
            scaled,
            glyph.flags,
            glyph.contours,
        );
        let stack = ValueStack::new(&mut stack_buffer);
        let mut storage = vec![0i32; self.storage.len()];
        let mut cvt = vec![0i32; self.cvt.len()];
        let mut interp = Engine::new(
            stack,
            CvtOrStorage::new(&self.storage, &mut storage),
            CvtOrStorage::new(&self.cvt, &mut cvt),
            MaybeMut::Ref(&self.functions),
            MaybeMut::Ref(&self.instructions),
            twilight_zone,
            glyph_zone,
            glyph.coords,
            self.axis_count,
        );
        let mut instance_state = self.state;
        let result = interp.run(
            &mut instance_state,
            outlines.fpgm,
            outlines.prep,
            glyph.bytecode,
            glyph.is_composite,
        );
        if !self.state.compat_enabled() {
            for (i, p) in (scaled[scaled.len() - 4..]).iter().enumerate() {
                phantom[i] = *p;
            }
        }
        Some(result)
    }

    /// Captures limits, resizes buffers and scales the CVT.
    fn setup_state(&mut self, outlines: &Outlines, scale: i32) -> Option<()> {
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
        self.max_twilight_points = outlines.max_twilight_points + 4;
        // Add 32 to match FreeType
        // See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/truetype/ttinterp.c#L356>
        self.max_stack = outlines.max_stack_elements + 32;
        self.state = InstanceState::default();
        Some(())
    }
}
