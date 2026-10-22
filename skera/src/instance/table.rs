//! Table operations return changes; only the font coordinator assembles them.
use super::AxisPlan;
use crate::{SubsetError, SubsetFlags};
use read_fonts::FontRef;

/// The context names a table's input dependencies. It never contains the
/// font's output table map. Outputs may be bytes, owned tables awaiting a
/// shared-store remap, or an explicit change to a table.
pub(crate) trait Instance<C> {
    type Output;

    fn instance(&self, context: &mut C) -> Result<Self::Output, SubsetError>;
}

#[derive(Default)]
pub(crate) enum TableChange {
    #[default]
    Keep,
    Replace(Vec<u8>),
    Remove,
}

/// Read-only source data and axis requests shared by operations that need
/// another table, such as STAT's references to the original fvar axes.
pub(crate) struct FontContext<'a> {
    pub font: &'a FontRef<'a>,
    pub axes: &'a AxisPlan,
    pub flags: SubsetFlags,
}
