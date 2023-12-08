//! Managing the control value table.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#managing-the-control-value-table>

// 3 instructions

use super::{super::math::mul, Engine, OpResult};

impl<'a> Engine<'a> {
    /// Write control value table in pixel units.
    ///
    /// WCVTP[] (0x44)
    ///
    /// Pops: value: number in pixels (F26Dot6 fixed point number),
    ///       location: Control Value Table location (uint32)
    ///
    /// Pops a location and a value from the stack and puts that value in the
    /// specified location in the Control Value Table. This instruction assumes
    /// the value is in pixels and not in FUnits.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#write-control-value-table-in-pixel-units>
    pub(super) fn op_wcvtp(&mut self) -> OpResult {
        let value = self.value_stack.pop()?;
        let location = self.value_stack.pop_usize()?;
        let result = self.cvt.set(location, value);
        if self.is_pedantic {
            result
        } else {
            Ok(())
        }
    }

    /// Write control value table in font units.
    ///
    /// WCVTF[] (0x70)
    ///
    /// Pops: value: number in pixels (F26Dot6 fixed point number),
    ///       location: Control Value Table location (uint32)
    ///
    /// Pops a location and a value from the stack and puts the specified
    /// value in the specified address in the Control Value Table. This
    /// instruction assumes the value is expressed in FUnits and not pixels.
    /// The value is scaled before being written to the table.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#write-control-value-table-in-funits>
    pub(super) fn op_wcvtf(&mut self) -> OpResult {
        let value = self.value_stack.pop()?;
        let location = self.value_stack.pop_usize()?;
        let result = self.cvt.set(location, mul(value, self.instance.scale));
        if self.is_pedantic {
            result
        } else {
            Ok(())
        }
    }

    /// Read control value table.
    ///
    /// RCVT[] (0x45)
    ///
    /// Pops: location: CVT entry number
    /// Pushes: value: CVT value (F26Dot6)
    ///
    /// Pops a location from the stack and pushes the value in the location
    /// specified in the Control Value Table onto the stack.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#read-control-value-table>
    pub(super) fn op_rcvt(&mut self) -> OpResult {
        let location = self.value_stack.pop()? as usize;
        let maybe_value = self.cvt.get(location);
        let value = if self.is_pedantic {
            maybe_value?
        } else {
            maybe_value.unwrap_or(0)
        };
        self.value_stack.push(value)
    }
}
