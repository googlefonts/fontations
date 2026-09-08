//! Tables parsed once for a font and kept.
//!
//! A parsed table borrows the bytes it came from, so it cannot simply be
//! stored beside them. [`Yoke`] makes that expressible by holding the parsed
//! value together with the tables it borrows, behind an [`Arc`] that outlives
//! both however they are dropped.
//!
//! Every table gets its own cell, so reading one never drags in another.
//! That matters for `glyf` and `loca`, which outlines and metrics in both
//! directions all read, and for `gvar`, which is typically about half a
//! variable font and must not be read to answer what another table can.

use super::FontTables;
use crate::tables::{glyf::Glyf, gvar::Gvar, hvar::Hvar, loca::Loca};
use crate::TableProvider;
use alloc::sync::Arc;
use yoke::{Yoke, Yokeable};

/// A parsed table held beside the tables it borrows.
pub(crate) struct TableCache<Y: for<'a> Yokeable<'a>>(Yoke<Y, Arc<FontTables>>);

impl<Y: for<'a> Yokeable<'a>> TableCache<Y> {
    /// Parses once, with `read`, and keeps the result.
    pub(crate) fn read<F>(tables: Arc<FontTables>, read: F) -> Self
    where
        F: for<'a> FnOnce(&'a FontTables) -> <Y as Yokeable<'a>>::Output,
    {
        Self(Yoke::attach_to_cart(tables, read))
    }

    /// Returns the parsed table, borrowed for no longer than this.
    #[inline]
    pub(crate) fn get(&self) -> &<Y as Yokeable<'_>>::Output {
        self.0.get()
    }
}

/// The outline tables.
///
/// These are read as a pair because neither is usable alone: `loca` states
/// where in `glyf` a glyph begins.
#[derive(Clone, Default, Yokeable)]
pub(crate) struct GlyfLoca<'a>(pub(crate) Option<(Glyf<'a>, Loca<'a>)>);

impl<'a> GlyfLoca<'a> {
    pub(crate) fn read(tables: &impl TableProvider<'a>) -> Self {
        Self(tables.glyf().ok().zip(tables.loca(None).ok()))
    }
}

/// The table stating how a location changes horizontal metrics.
#[derive(Clone, Default, Yokeable)]
pub(crate) struct HvarTable<'a>(pub(crate) Option<Hvar<'a>>);

impl<'a> HvarTable<'a> {
    pub(crate) fn read(tables: &impl TableProvider<'a>) -> Self {
        Self(tables.hvar().ok())
    }
}

/// The table stating how a location changes outlines.
///
/// Metrics read this only where `HVAR` is absent, recovering the answer from
/// the phantom points it carries alongside each glyph.
#[derive(Clone, Default, Yokeable)]
pub(crate) struct GvarTable<'a>(pub(crate) Option<Gvar<'a>>);

impl<'a> GvarTable<'a> {
    pub(crate) fn read(tables: &impl TableProvider<'a>) -> Self {
        Self(tables.gvar().ok())
    }
}
