//! Managing the storage area.
//!
//! See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#managing-the-storage-area>

// 2 instructions

use super::{Engine, OpResult};

impl<'a> Engine<'a> {
    /// Read store.
    ///
    /// RS[] (0x43)
    ///
    /// Pops: location: Storage Area location
    /// Pushes: value: Storage Area value
    ///
    /// This instruction reads a 32 bit value from the Storage Area location
    /// popped from the stack and pushes the value read onto the stack. It pops
    /// an address from the stack and pushes the value found in that Storage
    /// Area location to the top of the stack. The number of available storage
    /// locations is specified in the maxProfile table in the font file.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#read-store>
    pub(super) fn op_rs(&mut self) -> OpResult {
        let location = self.value_stack.pop_usize()?;
        let maybe_value = self.storage.get(location);
        let value = if self.is_pedantic {
            maybe_value?
        } else {
            maybe_value.unwrap_or(0)
        };
        self.value_stack.push(value)
    }

    /// Write store.
    ///
    /// WS[] (0x42)
    ///
    /// Pops: value: Storage Area value,
    ///       location: Storage Area location
    ///
    /// This instruction writes a 32 bit value into the storage location
    /// indexed by locations. It works by popping a value and then a location
    /// from the stack. The value is placed in the Storage Area location
    /// specified by that address. The number of storage locations is specified
    /// in the maxProfile table in the font file.
    ///
    /// See <https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions#write-store>
    pub(super) fn op_ws(&mut self) -> OpResult {
        let value = self.value_stack.pop()?;
        let location = self.value_stack.pop_usize()?;
        let result = self.storage.set(location, value);
        if self.is_pedantic {
            result
        } else {
            Ok(())
        }
    }
}
