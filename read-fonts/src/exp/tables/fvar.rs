//! fvar, generated into the reworked framework.
//!
//! `InstanceRecord` is hand-written today. Here it is generated: see
//! `resources/codegen_inputs/exp_fvar.rs` for what that took.

use super::super::prelude::*;

include!("../../../generated/exp/generated_fvar.rs");

impl InstanceRecord<'_> {
    /// The PostScript name ID, or `None` if the font does not provide one.
    ///
    /// The spec gives `0xFFFF` the meaning "no PostScript name equivalent", so
    /// the raw accessor's `Some(0xFFFF)` is not a name. That is a semantic the
    /// DSL has no way to state, so it stays hand-written — but it is now a
    /// three-line wrapper over a generated accessor rather than a reason to
    /// hand-write the whole record.
    ///
    /// <https://learn.microsoft.com/en-us/typography/opentype/spec/fvar#instancerecord>
    pub fn post_script_name(&self) -> Option<NameId> {
        self.post_script_name_id()
            .filter(|id| id.to_u16() != 0xFFFF)
    }
}
