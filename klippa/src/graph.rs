//! Define a graph struct that represents a serialized table
//! Implement methods to modify and reorder the graph

use crate::{
    priority_queue::PriorityQueue,
    serialize::{Link, LinkWidth, ObjIdx, Object, Serializer},
};
use fnv::FnvHashMap;
use write_fonts::read::collections::IntSet;

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) struct RepackErrorFlags(u16);

impl RepackErrorFlags {
    pub(crate) const ERROR_NONE: Self = Self(0x0000);
    pub(crate) const GRAPH_ERROR_ORPHANED_NODES: Self = Self(0x0001);
    pub(crate) const GRAPH_ERROR_INVALID_OBJ_INDEX: Self = Self(0x0002);
    pub(crate) const GRAPH_ERROR_INVALID_LINK_POSITION: Self = Self(0x0004);
    pub(crate) const GRAPH_ERROR_CYCLE_DETECTED: Self = Self(0x0008);
    #[allow(dead_code)]
    pub(crate) const GRAPH_ERROR_INVALID_ROOT: Self = Self(0x0010);
    #[allow(dead_code)]
    pub(crate) const REPACK_ERROR_SPLIT_SUBTABLE: Self = Self(0x0020);
    #[allow(dead_code)]
    pub(crate) const REPACK_ERROR_EXT_PROMOTION: Self = Self(0x0040);
    #[allow(dead_code)]
    pub(crate) const REPACK_ERROR_NO_RESOLUTION: Self = Self(0x0080);
}

impl std::ops::BitOrAssign for RepackErrorFlags {
    /// Adds the set of flags.
    #[inline]
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl std::ops::Not for RepackErrorFlags {
    type Output = bool;
    #[inline]
    fn not(self) -> bool {
        self == RepackErrorFlags::ERROR_NONE
    }
}

#[derive(Default, Debug)]
struct Vertex {
    head: usize,
    tail: usize,
    // real_links: link position-> Link mapping
    // real links are associated with actual offsets
    real_links: FnvHashMap<u32, Link>,
    // virtual links not associated with actual offsets,
    // they exist merely to enforce an ordering constraint.
    virtual_links: Vec<Link>,

    distance: u64,
    space: u32,
    priority: u8,
    //start: usize,
    //end: usize,
    incoming_edges: usize,
    has_incoming_virtual_edges: bool,
    parents: FnvHashMap<ObjIdx, u16>,
}

impl Vertex {
    fn from_object(obj: &Object) -> Self {
        Self {
            head: obj.head(),
            tail: obj.tail(),
            real_links: obj.real_links(),
            virtual_links: obj.virtual_links(),
            ..Default::default()
        }
    }

    fn table_size(&self) -> usize {
        self.tail - self.head
    }

    fn reset_parents(&mut self) {
        self.incoming_edges = 0;
        self.has_incoming_virtual_edges = false;
        self.parents.clear();
    }

    fn add_parent(&mut self, parent_idx: ObjIdx, is_virtual: bool) {
        self.has_incoming_virtual_edges |= is_virtual;
        self.parents
            .entry(parent_idx)
            .and_modify(|c| *c += 1)
            .or_insert(1);

        self.incoming_edges += 1;
    }

    fn link_positions_valid(&self, num_objs: usize) -> bool {
        let table_size = self.table_size();
        let mut assigned_bytes = IntSet::empty();
        for (pos, l) in &self.real_links {
            if l.obj_idx() >= num_objs {
                return false;
            }

            let width = l.link_width() as u8;
            if width < 2 {
                return false;
            }

            let start = *pos;
            let end = start + width as u32 - 1;
            if end as usize >= table_size {
                return false;
            }

            if assigned_bytes.intersects_range(start..=end) {
                return false;
            }
            assigned_bytes.insert_range(start..=end);
        }
        true
    }

    #[allow(dead_code)]
    fn has_max_priority(&self) -> bool {
        self.priority >= 3
    }

    fn modified_distance(&self, order: u32) -> i64 {
        let prev_dist = self.distance as i64;
        let table_size = (self.tail - self.head) as i64;

        let distance = match self.priority {
            0 => prev_dist,
            1 => prev_dist - table_size / 2,
            2 => prev_dist - table_size,
            _ => 0,
        }
        .clamp(0, 0x7FFFFFFFFFF_i64);

        (distance << 18) | (0x003FFFF & order as i64)
    }

    fn incoming_edges(&self) -> usize {
        self.incoming_edges
    }
}

//TODO: add support for space assignment and splitting
#[derive(Default, Debug)]
pub(crate) struct Graph {
    vertices: Vec<Vertex>,
    // an object's id will not change
    // the ordering vector stores sorted object ordering
    ordering: Vec<usize>,
    ordering_scratch: Vec<usize>,
    num_roots_for_space: Vec<usize>,
    #[allow(dead_code)]
    data: Vec<u8>,

    // graph state flags
    parents_invalid: bool,
    distance_invalid: bool,
    positions_invalid: bool,
    errors: RepackErrorFlags,
}

impl Graph {
    pub(crate) fn from_serializer(s: &Serializer) -> Result<Self, RepackErrorFlags> {
        let packed_obj_idxs = s.packed_obj_idxs();
        let count = packed_obj_idxs.len();
        let mut this = Graph {
            vertices: Vec::with_capacity(count),
            ordering: Vec::with_capacity(count),
            ordering_scratch: Vec::with_capacity(count),
            data: s.data(),
            parents_invalid: true,
            positions_invalid: true,
            distance_invalid: true,
            ..Default::default()
        };

        this.num_roots_for_space.push(1);
        for obj_idx in packed_obj_idxs.iter() {
            let obj = s
                .get_obj(*obj_idx)
                .ok_or(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX)?;

            let v = Vertex::from_object(obj);
            if !v.link_positions_valid(count) {
                return Err(RepackErrorFlags::GRAPH_ERROR_INVALID_LINK_POSITION);
            }
            this.vertices.push(v);
        }

        let mut i = 0;
        this.ordering.resize_with(count, || {
            i += 1;
            count - i
        });
        Ok(this)
    }

    #[allow(dead_code)]
    pub(crate) fn in_error(&self) -> bool {
        !!self.errors
    }

    #[allow(dead_code)]
    pub(crate) fn error(&self) -> RepackErrorFlags {
        self.errors
    }

    pub(crate) fn sort_shortest_distance(&mut self) -> Result<(), RepackErrorFlags> {
        if self.vertices.len() < 2 {
            // no need to do sorting when num of nodes < 2
            return Ok(());
        }

        self.positions_invalid = true;
        self.update_distances()?;

        let v_count = self.vertices.len();
        let mut queue = PriorityQueue::with_capacity(v_count);
        self.ordering_scratch.resize(v_count, 0);
        let mut removed_edges = vec![0_usize; v_count];

        self.update_parents()?;

        queue.push((self.root().modified_distance(0), self.root_idx()));
        let mut order = 1_u32;
        let mut pos = 0;

        let new_ordering = &mut self.ordering_scratch;
        while let Some((_, next_id)) = queue.pop() {
            if pos >= v_count {
                // we're out of ids, meaning we've visited the same node more than once
                return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_CYCLE_DETECTED));
            }
            new_ordering[pos] = next_id;
            pos += 1;

            let next_v = &self.vertices[next_id];
            for link in next_v
                .real_links
                .values()
                .chain(next_v.virtual_links.iter())
            {
                let child_idx = link.obj_idx();
                removed_edges[child_idx] += 1;

                let child_v = &self.vertices[child_idx];
                if child_v.incoming_edges() == removed_edges[child_idx] {
                    queue.push((child_v.modified_distance(order), child_idx));
                    order += 1;
                }
            }
        }

        std::mem::swap(&mut self.ordering, new_ordering);
        if pos != v_count {
            return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_ORPHANED_NODES));
            //TODO: add print_orphaned_nodes()
        }
        Ok(())
    }

    fn root(&self) -> &Vertex {
        &self.vertices[self.root_idx()]
    }

    pub(crate) fn update_parents(&mut self) -> Result<(), RepackErrorFlags> {
        if !self.parents_invalid {
            return Ok(());
        }

        for v in self.vertices.iter_mut() {
            v.reset_parents();
        }

        let count = self.vertices.len();
        let mut real_links_idxes = Vec::with_capacity(count);
        let mut virtual_links_idxes = Vec::with_capacity(count);
        for idx in 0..count {
            let v = &self.vertices[idx];

            real_links_idxes.clear();
            virtual_links_idxes.clear();
            for l in v.real_links.values() {
                real_links_idxes.push(l.obj_idx());
            }

            for l in &v.virtual_links {
                virtual_links_idxes.push(l.obj_idx());
            }

            for child_idx in &real_links_idxes {
                let Some(v) = self.vertices.get_mut(*child_idx) else {
                    return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX));
                };
                v.add_parent(idx, false);
            }

            for child_idx in &virtual_links_idxes {
                let Some(v) = self.vertices.get_mut(*child_idx) else {
                    return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX));
                };
                v.add_parent(idx, true);
            }
        }
        Ok(())
    }

    // Finds the distance to each object in the graph from the root node
    // Uses Dijkstra's algorithm to find all of the shortest distances.
    // ref: <https://github.com/harfbuzz/harfbuzz/blob/3f70b6987830bba1e4922cad03028cdd9d78b3a1/src/graph/graph.hh#L1512C13-L1512C72>
    pub(crate) fn update_distances(&mut self) -> Result<(), RepackErrorFlags> {
        if !self.distance_invalid {
            return Ok(());
        }

        for i in self.vertices.iter_mut() {
            i.distance = u64::MAX / 2;
        }
        let root_idx = self.root_idx();
        self.vertices[root_idx].distance = 0;

        let count = self.vertices.len();
        let mut queue = PriorityQueue::with_capacity(count);
        queue.push((0_u64, root_idx));

        let mut visited = vec![false; count];
        let mut distance_map = vec![0; count];
        while let Some((_, next_idx)) = queue.pop() {
            if visited[next_idx] {
                continue;
            }

            let next_v = &self.vertices[next_idx];
            let next_distance = next_v.distance;
            visited[next_idx] = true;

            for link in next_v
                .real_links
                .values()
                .chain(next_v.virtual_links.iter())
            {
                let child_idx = link.obj_idx();
                if visited[child_idx] {
                    continue;
                }

                let child_v = &self.vertices[child_idx];
                let link_width = if link.link_width() == LinkWidth::Zero {
                    4
                } else {
                    link.link_width() as u8
                };

                let child_weight = child_v.tail - child_v.head
                    + (1 << (link_width * 8)) * (child_v.space as usize + 1);
                let child_distance = next_distance + child_weight as u64;
                if child_distance < child_v.distance {
                    distance_map[child_idx] = child_distance;
                    queue.push((child_distance, child_idx));
                }
            }
        }

        // to avoid multiple references issue, update distances from the map
        for (distance, v) in distance_map.iter().zip(self.vertices.iter_mut()) {
            v.distance = *distance;
        }

        if !queue.is_empty() {
            return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_ORPHANED_NODES));
        }

        self.distance_invalid = false;
        Ok(())
    }

    fn set_err(&mut self, error_type: RepackErrorFlags) -> RepackErrorFlags {
        self.errors |= error_type;
        self.errors
    }

    fn root_idx(&self) -> usize {
        self.ordering[0]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::serialize::OffsetWhence;
    use write_fonts::types::{FixedSize, Offset16};

    impl Vertex {
        // zeros all offsets
        fn normalize(&self, data_bytes: &mut [u8]) {
            let head = self.head;
            for (pos, l) in self.real_links.iter() {
                let pos = head + *pos as usize;
                let width = l.link_width() as u8;
                data_bytes
                    .get_mut(pos..pos + width as usize)
                    .unwrap()
                    .fill(0);
            }
        }

        //TODO: add debugging messages
        fn equals(&self, other: &Self, this_graph: &Graph, other_graph: &Graph) -> bool {
            if self.as_bytes(this_graph) != other.as_bytes(other_graph) {
                return false;
            }

            let real_links = &self.real_links;
            let other_real_links = &other.real_links;
            if real_links.len() != other_real_links.len() {
                return false;
            }

            for (link_a, link_b) in real_links.values().zip(other_real_links.values()) {
                if !links_equal(link_a, link_b, this_graph, other_graph) {
                    return false;
                }
            }
            true
        }

        fn as_bytes<'a>(&self, graph: &'a Graph) -> &'a [u8] {
            let head = self.head;
            let table_size = self.table_size();
            graph.data.get(head..head + table_size).unwrap()
        }
    }

    fn links_equal(link_a: &Link, link_b: &Link, graph_a: &Graph, graph_b: &Graph) -> bool {
        if !link_a.partial_equals(link_b) {
            return false;
        }

        let obj_a = link_a.obj_idx();
        let obj_b = link_b.obj_idx();
        graph_a.vertices[obj_a].equals(&graph_b.vertices[obj_b], graph_a, graph_b)
    }

    impl Graph {
        fn normalize(&mut self) {
            for v in &self.vertices {
                v.normalize(&mut self.data);
            }
        }
    }

    impl PartialEq for Graph {
        fn eq(&self, other: &Self) -> bool {
            self.root().equals(other.root(), self, other)
        }
    }

    fn extend(s: &mut Serializer, bytes: &[u8], len: usize) {
        let pos = s.allocate_size(len, true).unwrap();
        s.copy_assign_from_bytes(pos, bytes);
    }

    fn start_object(s: &mut Serializer, bytes: &[u8], len: usize) {
        s.push().unwrap();
        extend(s, bytes, len);
    }

    fn add_object(s: &mut Serializer, bytes: &[u8], len: usize) -> ObjIdx {
        start_object(s, bytes, len);
        s.pop_pack(false).unwrap()
    }

    fn add_offset(s: &mut Serializer, obj_idx: ObjIdx) {
        let offset_pos = s.allocate_size(Offset16::RAW_BYTE_LEN, true).unwrap();
        s.add_link(
            offset_pos..offset_pos + 2,
            obj_idx,
            OffsetWhence::Head,
            0,
            false,
        )
        .unwrap();
    }

    fn populate_serializer_complex_2(s: &mut Serializer) {
        let _ = s.start_serialize();
        let obj_5 = add_object(s, b"mn", 2);
        let obj_4 = add_object(s, b"jkl", 3);

        start_object(s, b"ghi", 3);
        add_offset(s, obj_4);
        let obj_3 = s.pop_pack(false).unwrap();

        start_object(s, b"def", 3);
        add_offset(s, obj_3);
        let obj_2 = s.pop_pack(false).unwrap();

        start_object(s, b"abc", 3);
        add_offset(s, obj_2);
        add_offset(s, obj_4);
        add_offset(s, obj_5);
        s.pop_pack(false);

        s.end_serialize();
    }

    #[test]
    fn test_sort_shortest() {
        let buf_size = 100;
        let mut a = Serializer::new(buf_size);
        let mut e = Serializer::new(buf_size);
        populate_serializer_complex_2(&mut a);

        let mut graph = Graph::from_serializer(&a).unwrap();
        assert_eq!(graph.sort_shortest_distance(), Ok(()));
        graph.normalize();
        assert!(!graph.in_error());

        // Expected graph
        let _ = e.start_serialize();

        let jkl = add_object(&mut e, b"jkl", 3);
        start_object(&mut e, b"ghi", 3);
        add_offset(&mut e, jkl);
        let ghi = e.pop_pack(false).unwrap();

        start_object(&mut e, b"def", 3);
        add_offset(&mut e, ghi);
        let def = e.pop_pack(false).unwrap();

        let mn = add_object(&mut e, b"mn", 2);

        start_object(&mut e, b"abc", 3);
        add_offset(&mut e, def);
        add_offset(&mut e, jkl);
        add_offset(&mut e, mn);
        e.pop_pack(false);
        e.end_serialize();

        let mut expected_graph = Graph::from_serializer(&e).unwrap();
        expected_graph.normalize();

        assert_eq!(graph, expected_graph);
    }
}
