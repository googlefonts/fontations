//! Cycle detection for DFS traversal of a graph.
//!
//! Copied from `skrifa::decycler`, which is itself based on `hb_decycler_t` in
//! HarfBuzz (<https://github.com/harfbuzz/harfbuzz/blob/a2ea5d28cb5387f4de2049802474b817be15ad5b/src/hb-decycler.hh>),
//! an extension of Floyd's tortoise and hare to DFS traversals.
//!
//! The one change from skrifa's copy is the interface: that version hands back
//! a guard whose `Drop` pops the stack, which is the right shape for a
//! hand-written recursive walk. The sanitizer's walk is generated as flat
//! `enter`/`exit` pairs, so this version exposes those directly.
//!
//! What matters here is that it holds a fixed-size array and so allocates
//! nothing. A visited set would be cheaper on a graph that shares subtables
//! widely, but it would allocate, and the fast path is the one that runs on
//! every font.

/// The deepest a walk may nest.
///
/// This bounds the *Rust* stack: the walk recurses through generated
/// `sanitize_in` frames, so without a limit a font whose offsets chain deeply
/// enough would overflow it. Nothing in OpenType legitimately nests anywhere
/// near this.
pub const MAX_DEPTH: usize = 64;

/// Why a node could not be entered.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Rejected {
    /// The node is already on the path from the root: following it would loop.
    Cycle,
    /// The walk is already [`MAX_DEPTH`] deep.
    TooDeep,
}

/// Cycle detector for a depth-first walk.
///
/// Nodes are identified by a `T` that is unique per node. Detection is against
/// the *ancestors* of the current node, not against everything ever seen, so a
/// subtable reachable from two places is walked twice — which is what makes the
/// structure a fixed-size array rather than a set.
pub struct Decycler<T, const D: usize = MAX_DEPTH> {
    node_ids: [T; D],
    depth: usize,
}

impl<T: Copy + PartialEq + Default, const D: usize> Decycler<T, D> {
    pub fn new() -> Self {
        Self {
            node_ids: [T::default(); D],
            depth: 0,
        }
    }

    /// How deep the walk currently is.
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Enters the node `node_id` identifies.
    ///
    /// On success the caller must pair this with [`exit`][Self::exit].
    pub fn enter(&mut self, node_id: T) -> Result<(), Rejected> {
        if self.depth >= D {
            return Err(Rejected::TooDeep);
        }
        // the hare is at `depth`, the tortoise at `depth / 2`; if they meet,
        // the path has looped
        if self.depth != 0 && self.node_ids[self.depth / 2] == node_id {
            return Err(Rejected::Cycle);
        }
        self.node_ids[self.depth] = node_id;
        self.depth = self.depth.saturating_add(1);
        Ok(())
    }

    /// Leaves the node most recently entered.
    pub fn exit(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

impl<T: Copy + PartialEq + Default, const D: usize> Default for Decycler<T, D> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Test = Decycler<usize, 8>;

    #[test]
    fn a_straight_chain_is_fine() {
        let mut d = Test::new();
        for i in 0..8 {
            assert_eq!(d.enter(i), Ok(()), "at {i}");
        }
        assert_eq!(d.depth(), 8);
    }

    #[test]
    fn depth_is_bounded() {
        let mut d = Test::new();
        for i in 0..8 {
            d.enter(i).unwrap();
        }
        assert_eq!(d.enter(99), Err(Rejected::TooDeep));
    }

    #[test]
    fn a_loop_is_caught() {
        let mut d = Test::new();
        // 0 → 1 → 2 → 3 → back to 1
        for i in [0, 1, 2, 3] {
            d.enter(i).unwrap();
        }
        // the tortoise is at index 2 (depth 4 / 2), holding node 2... walk on
        // until hare meets tortoise
        let mut hit = false;
        for i in [1, 2, 3, 1, 2, 3] {
            if d.enter(i).is_err() {
                hit = true;
                break;
            }
        }
        assert!(hit, "expected the cycle to be caught");
    }

    #[test]
    fn exiting_frees_the_slot() {
        let mut d = Test::new();
        d.enter(1).unwrap();
        d.enter(2).unwrap();
        d.exit();
        d.exit();
        assert_eq!(d.depth(), 0);
        // the same nodes can be walked again down a different branch
        assert_eq!(d.enter(1), Ok(()));
    }

    #[test]
    fn a_shared_node_is_not_a_cycle() {
        let mut d = Test::new();
        d.enter(1).unwrap();
        d.enter(2).unwrap();
        d.exit();
        // sibling branch reaching the same node: allowed, it is not an ancestor
        assert_eq!(d.enter(2), Ok(()));
    }
}
