//! Font subsetting.
//!
//! This is intended as a proof of concept of how a subsetter might be implemented,
//! in order to evaluate performance.

mod gpos;
mod layout;

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

use font_types::GlyphId;
use write_fonts::{NullableOffsetMarker, OffsetMarker};

/// Input to a subsetting operation.
pub struct Input {
    glyph_ids: BTreeSet<GlyphId>,
    retain_gids: bool,
}

impl Input {
    pub fn from_gids(mut glyph_ids: BTreeSet<GlyphId>, retain_gids: bool) -> Self {
        glyph_ids.insert(GlyphId::NOTDEF); // always include .notdef
        Input {
            glyph_ids,
            retain_gids,
        }
    }

    pub fn make_plan(&self) -> Plan {
        let gid_map = if self.retain_gids {
            self.glyph_ids.iter().map(|gid| (*gid, *gid)).collect()
        } else {
            self.glyph_ids
                .iter()
                .enumerate()
                .map(|(i, gid)| (*gid, GlyphId::new(u16::try_from(i).unwrap())))
                .collect()
        };
        Plan {
            gid_map,
            gpos_lookup_map: Default::default(),
            gpos_feature_map: Default::default(),
        }
    }
}

/// The 'plan' for a subsetting operation.
///
/// This tells us what all we're supposed to subset, as well as stores data used
/// for remapping various identifiers.
pub struct Plan {
    gid_map: HashMap<GlyphId, GlyphId>,
    /// map from old lookup indicies to new ones
    //NOTE: this is only a refcell so that during development I can do some
    //mutation without needing to change all my type signatures 🤷
    gpos_lookup_map: RefCell<Vec<Option<u16>>>,
    gpos_feature_map: RefCell<Vec<Option<u16>>>,
}

impl Plan {
    pub fn remap_gid(&self, gid: GlyphId) -> Option<GlyphId> {
        self.gid_map.get(&gid).copied()
    }

    pub fn set_gpos_lookup_map(&self, map: Vec<Option<u16>>) {
        self.gpos_lookup_map.replace(map);
    }

    pub fn remap_gpos_lookup(&self, idx: u16) -> Option<u16> {
        self.gpos_lookup_map
            .borrow()
            .get(idx as usize)
            .copied()
            .flatten()
    }

    pub fn set_gpos_feature_map(&self, map: Vec<Option<u16>>) {
        self.gpos_feature_map.replace(map);
    }

    pub fn remap_gpos_feature(&self, idx: u16) -> Option<u16> {
        self.gpos_feature_map
            .borrow()
            .get(idx as usize)
            .copied()
            .flatten()
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Error {
    msg: String,
}

impl Error {
    pub fn new(s: impl Into<String>) -> Self {
        Error { msg: s.into() }
    }
}

pub trait Subset {
    /// Subset this object. Returns `true` if the object should be retained.
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error>;
}

impl<const N: usize, T: Subset> Subset for OffsetMarker<T, N> {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let retain = self
            .get_mut()
            .map(|t| t.subset(plan))
            .transpose()?
            .unwrap_or(false);
        if !retain {
            self.clear();
        }
        Ok(retain)
    }
}

impl<const N: usize, T: Subset> Subset for NullableOffsetMarker<T, N> {
    fn subset(&mut self, plan: &Plan) -> Result<bool, Error> {
        let retain = self
            .get_mut()
            .map(|t| t.subset(plan))
            .transpose()?
            .unwrap_or(false);
        if !retain {
            self.clear()
        }
        Ok(retain)
    }
}
