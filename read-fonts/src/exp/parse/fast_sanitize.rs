//! Deciding whether a font is well formed, without saying why.
//!
//! The counterpart to [`sanitize`][super::sanitize], and the one you would run
//! on every font you load. It walks the same graph and makes the same checks,
//! but it stops at the first problem and reports nothing about it — so it needs
//! no paths, no error list, and, the part that actually costs something in a
//! shipped binary, **no strings**.
//!
//! That is the whole reason the two are separate rather than one pass with a
//! flag. The detailed pass names every table and every field it checks, which
//! for the four modules generated so far is 568 string literals and about 9.7kB
//! of rodata. A caller that only wants a yes or no should not link any of it,
//! and with the walks split it does not.
//!
//! Enabling `sanitize` turns this on too: a build that can diagnose should also
//! be able to answer quickly.

#![deny(clippy::arithmetic_side_effects)]

use super::bytes::Bytes;
use super::decycler::{Decycler, Rejected};

/// How far into the graph a walk will go before giving up.
///
/// Cycles and depth are handled by the [`Decycler`], which needs no
/// configuration; these bound the rest.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The most tables to visit.
    pub tables: usize,
    /// The most elements of any one array to descend into.
    ///
    /// The array's own extent is always checked, so a count larger than the
    /// data is still caught; this caps only how many elements are *walked*.
    pub array_elements: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            tables: 1 << 16,
            array_elements: 1 << 14,
        }
    }
}

/// The state a walk carries.
///
/// Generated code drives this; there is no need to call it by hand.
pub struct Context {
    limits: Limits,
    tables_visited: usize,
    decycler: Decycler<(usize, usize)>,
}

impl Context {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            tables_visited: 0,
            decycler: Decycler::new(),
        }
    }

    /// How many tables the walk reached.
    pub fn tables_visited(&self) -> usize {
        self.tables_visited
    }

    /// Begins a table, or `false` if the walk cannot continue.
    ///
    /// A cycle, a depth limit or an exhausted table budget all give `false`,
    /// which the caller propagates as a failure. Stopping early is reported as
    /// unsound rather than sound, because a walk that did not finish cannot
    /// vouch for what it did not see.
    ///
    /// The node's identity is where its data starts plus its `MIN_SIZE`, which
    /// stands in for the type: the detailed pass uses the type's name, and the
    /// point of this one is to have no names.
    #[inline]
    pub fn enter(&mut self, data: Bytes, min_size: usize) -> bool {
        if self.tables_visited >= self.limits.tables {
            return false;
        }
        let id = (data.as_bytes().as_ptr() as usize, min_size);
        match self.decycler.enter(id) {
            Ok(()) => {}
            Err(Rejected::Cycle | Rejected::TooDeep) => return false,
        }
        self.tables_visited = self.tables_visited.saturating_add(1);
        true
    }

    #[inline]
    pub fn exit(&mut self) {
        self.decycler.exit();
    }

    /// How many elements of an array to walk.
    #[inline]
    pub fn element_budget(&self, len: usize) -> usize {
        len.min(self.limits.array_elements)
    }
}

/// A table that can check itself and everything it points at, quickly.
///
/// Generated alongside the table, behind the `fast_sanitize` feature.
pub trait FastSanitize<'a> {
    /// `false` as soon as anything is wrong, without saying what.
    ///
    /// The caller propagates a `false` straight out; nothing after it runs, and
    /// the context is discarded, so a failing walk does not bother unwinding
    /// the decycler.
    fn fast_sanitize_in(&self, ctx: &mut Context) -> bool;
}

/// `true` if nothing reachable from `table` is malformed.
pub fn is_sound<'a, T: FastSanitize<'a>>(table: &T) -> bool {
    is_sound_with(table, Limits::default())
}

/// As [`is_sound`], with explicit limits.
pub fn is_sound_with<'a, T: FastSanitize<'a>>(table: &T, limits: Limits) -> bool {
    table.fast_sanitize_in(&mut Context::new(limits))
}
