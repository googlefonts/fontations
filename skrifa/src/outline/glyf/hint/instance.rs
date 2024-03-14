//! Instance state for TrueType hinting.

use super::{
    super::Outlines,
    cow_slice::CowSlice,
    definition::{Definition, DefinitionMap, DefinitionState},
    engine::Engine,
    error::HintError,
    graphics_state::{RetainedGraphicsState, Zone},
    program::{Program, ProgramState},
    value_stack::ValueStack,
    HintOutline, HintingMode, PointFlags,
};
use raw::types::{F26Dot6, F2Dot14, Fixed, Point};

#[derive(Clone, Default)]
pub struct HintInstance {
    functions: Vec<Definition>,
    instructions: Vec<Definition>,
    cvt: Vec<i32>,
    storage: Vec<i32>,
    graphics: RetainedGraphicsState,
    twilight_scaled: Vec<Point<F26Dot6>>,
    twilight_original_scaled: Vec<Point<F26Dot6>>,
    twilight_flags: Vec<PointFlags>,
    axis_count: u16,
    max_stack: usize,
}

impl HintInstance {
    pub fn reconfigure(
        &mut self,
        outlines: &Outlines,
        scale: i32,
        ppem: i32,
        mode: HintingMode,
        coords: &[F2Dot14],
    ) -> Result<(), HintError> {
        self.setup(outlines, scale);
        let twilight_contours = [self.twilight_scaled.len() as u16];
        let twilight = Zone::new(
            &[],
            &mut self.twilight_original_scaled,
            &mut self.twilight_scaled,
            &mut self.twilight_flags,
            &twilight_contours,
        );
        let glyph = Zone::default();
        let mut stack_buf = vec![0; self.max_stack];
        let value_stack = ValueStack::new(&mut stack_buf);
        let graphics = RetainedGraphicsState {
            scale,
            ppem,
            mode,
            ..Default::default()
        };
        let mut engine = Engine::new(
            outlines,
            ProgramState::new(outlines.fpgm, outlines.prep, &[], Program::Font),
            graphics,
            DefinitionState::new(
                DefinitionMap::Mut(&mut self.functions),
                DefinitionMap::Mut(&mut self.instructions),
            ),
            CowSlice::new_mut(&mut self.cvt),
            CowSlice::new_mut(&mut self.storage),
            value_stack,
            twilight,
            glyph,
            self.axis_count,
            coords,
            false,
        );
        // Run the font program (fpgm)
        engine.run_program(Program::Font)?;
        // Run the control value program (prep)
        engine.run_program(Program::ControlValue)?;
        // Save the retained state from the CV program
        self.graphics = *engine.retained_graphics_state();
        Ok(())
    }

    /// Returns true if we should actually apply hinting.
    ///
    /// Hinting can be completely disabled by the control value program.
    pub fn is_enabled(&self) -> bool {
        // If bit 0 is set, disables hinting entirely
        self.graphics.instruct_control & 1 == 0
    }

    pub fn hint(&self, outlines: &Outlines, outline: &mut HintOutline) -> Result<(), HintError> {
        // Twilight zone
        let twilight_count = outline.twilight_scaled.len();
        let twilight_contours = [twilight_count as u16];
        outline
            .twilight_original_scaled
            .copy_from_slice(&self.twilight_original_scaled);
        outline
            .twilight_scaled
            .copy_from_slice(&self.twilight_scaled);
        outline.twilight_flags.copy_from_slice(&self.twilight_flags);
        let twilight = Zone::new(
            &[],
            outline.twilight_original_scaled,
            outline.twilight_scaled,
            outline.twilight_flags,
            &twilight_contours,
        );
        // Glyph zone
        let glyph = Zone::new(
            outline.unscaled,
            outline.original_scaled,
            outline.scaled,
            outline.flags,
            outline.contours,
        );
        let value_stack = ValueStack::new(outline.stack);
        let cvt = CowSlice::new(&self.cvt, outline.cvt).unwrap();
        let storage = CowSlice::new(&self.storage, outline.storage).unwrap();
        let mut engine = Engine::new(
            outlines,
            ProgramState::new(
                outlines.fpgm,
                outlines.prep,
                outline.bytecode,
                Program::Glyph,
            ),
            self.graphics,
            DefinitionState::new(
                DefinitionMap::Ref(&self.functions),
                DefinitionMap::Ref(&self.instructions),
            ),
            cvt,
            storage,
            value_stack,
            twilight,
            glyph,
            self.axis_count,
            outline.coords,
            outline.is_composite,
        );
        engine.run_program(Program::Glyph).map_err(|mut e| {
            e.glyph_id = Some(outline.glyph_id);
            e
        })?;
        // If we're not running in backward compatibility mode, capture
        // modified phantom points.
        if !engine.backward_compatibility() {
            for (i, p) in (outline.scaled[outline.scaled.len() - 4..])
                .iter()
                .enumerate()
            {
                outline.phantom[i] = *p;
            }
        }
        Ok(())
    }

    /// Captures limits, resizes buffers and scales the CVT.
    fn setup(&mut self, outlines: &Outlines, scale: i32) {
        let axis_count = outlines
            .gvar
            .as_ref()
            .map(|gvar| gvar.axis_count())
            .unwrap_or_default();
        self.functions.clear();
        self.functions
            .resize(outlines.max_function_defs as usize, Definition::default());
        self.instructions.resize(
            outlines.max_instruction_defs as usize,
            Definition::default(),
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
        let max_twilight_points = outlines.max_twilight_points as usize;
        self.twilight_scaled.clear();
        self.twilight_scaled
            .resize(max_twilight_points, Default::default());
        self.twilight_original_scaled.clear();
        self.twilight_original_scaled
            .resize(max_twilight_points, Default::default());
        self.twilight_flags.clear();
        self.twilight_flags
            .resize(max_twilight_points, Default::default());
        self.axis_count = axis_count;
        self.max_stack = outlines.max_stack_elements as usize;
        self.graphics = RetainedGraphicsState::default();
    }
}
