//! Reading and writing data.
//!
//! Implements 2 instructions.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#reading-and-writing-data>

use super::{Engine, OpResult};

impl<'a> Engine<'a> {
    /// Measure pixels per em.
    ///
    /// MPPEM[] (0x4B)
    ///
    /// Pushes: ppem: pixels per em (uint32)
    ///
    /// This instruction pushes the number of pixels per em onto the stack.
    /// Pixels per em is a function of the resolution of the rendering device
    /// and the current point size and the current transformation matrix.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#measure-pixels-per-em>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L2609>
    pub(super) fn op_mppem(&mut self) -> OpResult {
        self.value_stack.push(self.graphics_state.ppem)
    }

    /// Measure point size.
    ///
    /// MPS[] (0x4C)
    ///
    /// Pushes: pointSize: the size in points of the current glyph (F26Dot6)
    ///
    /// Measure point size can be used to obtain a value which serves as the
    /// basis for choosing whether to branch to an alternative path through the
    /// instruction stream. It makes it possible to treat point sizes below or
    /// above a certain threshold differently.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#measure-point-size>
    /// and <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L2623>
    pub(super) fn op_mps(&mut self) -> OpResult {
        // Note: FreeType computes this at
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttdriver.c#L392>
        // which is mul_div(ppem, 64 * 72, resolution) where resolution
        // is always 72 for our purposes (Skia), resulting in ppem * 64.
        self.value_stack.push(self.graphics_state.ppem * 64)
    }
}

#[cfg(test)]
mod tests {
    use super::super::MockEngine;

    #[test]
    fn measure_ppem_and_point_size() {
        let mut mock = MockEngine::new();
        let mut engine = mock.engine();
        let ppem = 20;
        engine.graphics_state.ppem = ppem;
        engine.op_mppem().unwrap();
        assert_eq!(engine.value_stack.pop().unwrap(), ppem);
        engine.op_mps().unwrap();
        assert_eq!(engine.value_stack.pop().unwrap(), ppem * 64);
    }
}
