use alloc::vec::Vec;

use crate::outline::autohint::topo::{BlueProvenance, Dimension};

#[allow(dead_code)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    IpBefore,
    IpAfter,
    IpOn,
    IpBetween,
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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PointHintRecord {
    pub action: Action,
    pub dim: Dimension,
    pub point_ix: u16,
    pub edge_ix: Option<u16>,
    pub edge2_ix: Option<u16>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct EdgeHintRecord {
    pub action: Action,
    pub dim: Dimension,
    pub edge_ix: u16,
    pub edge2_ix: Option<u16>,
    pub edge3_ix: Option<u16>,
    pub lower_bound_ix: Option<u16>,
    pub upper_bound_ix: Option<u16>,
    pub blue: Option<BlueProvenance>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum HintRecord {
    Point(PointHintRecord),
    Edge(EdgeHintRecord),
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct HintsRecorder {
    pub records: Vec<HintRecord>,
}

impl HintsRecorder {
    pub fn record_ip_before(&mut self, dim: Dimension, point_ix: usize) {
        self.records.push(HintRecord::Point(PointHintRecord {
            action: Action::IpBefore,
            dim,
            point_ix: narrow(point_ix),
            edge_ix: None,
            edge2_ix: None,
        }));
    }

    pub fn record_ip_after(&mut self, dim: Dimension, point_ix: usize) {
        self.records.push(HintRecord::Point(PointHintRecord {
            action: Action::IpAfter,
            dim,
            point_ix: narrow(point_ix),
            edge_ix: None,
            edge2_ix: None,
        }));
    }

    pub fn record_ip_on(&mut self, dim: Dimension, point_ix: usize, edge_ix: usize) {
        self.records.push(HintRecord::Point(PointHintRecord {
            action: Action::IpOn,
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
        self.records.push(HintRecord::Point(PointHintRecord {
            action: Action::IpBetween,
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
        action: Action,
        edge_ix: usize,
        edge2_ix: Option<usize>,
        edge3_ix: Option<usize>,
        lower_bound_ix: Option<usize>,
        upper_bound_ix: Option<usize>,
        blue: Option<BlueProvenance>,
    ) {
        self.records.push(HintRecord::Edge(EdgeHintRecord {
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
