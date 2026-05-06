use alloc::vec::Vec;

use crate::outline::autohint::topo::{BlueProvenance, Dimension};

/// Hinting action for a point.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PointAction {
    IpBefore,
    IpAfter,
    IpOn,
    IpBetween,
}

/// Hinting action for an edge.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EdgeAction {
    Blue,
    BlueAnchor,
    Anchor,
    Adjust,
    Link,
    Stem,
    Serif,
    SerifAnchor,
    SerifLink1,
    SerifLink2,
    Bound,
}

/// Hinting information for a point.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PointHint {
    pub action: PointAction,
    pub dim: Dimension,
    pub point_ix: u16,
    pub edge_ix: Option<u16>,
    pub edge2_ix: Option<u16>,
}

/// Hinting information for an edge.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct EdgeHint {
    pub action: EdgeAction,
    pub dim: Dimension,
    pub edge_ix: u16,
    pub edge2_ix: Option<u16>,
    pub edge3_ix: Option<u16>,
    pub lower_bound_ix: Option<u16>,
    pub upper_bound_ix: Option<u16>,
    pub blue: Option<BlueProvenance>,
}

/// Hinting information for a point or an edge.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Hint {
    Point(PointHint),
    Edge(EdgeHint),
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct HintsRecorder {
    pub records: Vec<Hint>,
}

impl HintsRecorder {
    pub fn record_ip_before(&mut self, dim: Dimension, point_ix: usize) {
        self.records.push(Hint::Point(PointHint {
            action: PointAction::IpBefore,
            dim,
            point_ix: narrow(point_ix),
            edge_ix: None,
            edge2_ix: None,
        }));
    }

    pub fn record_ip_after(&mut self, dim: Dimension, point_ix: usize) {
        self.records.push(Hint::Point(PointHint {
            action: PointAction::IpAfter,
            dim,
            point_ix: narrow(point_ix),
            edge_ix: None,
            edge2_ix: None,
        }));
    }

    pub fn record_ip_on(&mut self, dim: Dimension, point_ix: usize, edge_ix: usize) {
        self.records.push(Hint::Point(PointHint {
            action: PointAction::IpOn,
            dim,
            point_ix: narrow(point_ix),
            edge_ix: Some(narrow(edge_ix)),
            edge2_ix: None,
        }));
    }

    pub fn record_ip_between(
        &mut self,
        dim: Dimension,
        point_ix: usize,
        before_edge_ix: usize,
        after_edge_ix: usize,
    ) {
        self.records.push(Hint::Point(PointHint {
            action: PointAction::IpBetween,
            dim,
            point_ix: narrow(point_ix),
            edge_ix: Some(narrow(before_edge_ix)),
            edge2_ix: Some(narrow(after_edge_ix)),
        }));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_edge(
        &mut self,
        dim: Dimension,
        action: EdgeAction,
        edge_ix: usize,
        edge2_ix: Option<usize>,
        edge3_ix: Option<usize>,
        lower_bound_ix: Option<usize>,
        upper_bound_ix: Option<usize>,
        blue: Option<BlueProvenance>,
    ) {
        self.records.push(Hint::Edge(EdgeHint {
            action,
            dim,
            edge_ix: narrow(edge_ix),
            edge2_ix: edge2_ix.map(narrow),
            edge3_ix: edge3_ix.map(narrow),
            lower_bound_ix: lower_bound_ix.map(narrow),
            upper_bound_ix: upper_bound_ix.map(narrow),
            blue,
        }));
    }
}

fn narrow(ix: usize) -> u16 {
    debug_assert!(ix <= u16::MAX as usize);
    ix as u16
}
