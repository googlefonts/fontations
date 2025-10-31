//! Define a graph struct that represents a serialized table
//! Implement methods to modify and reorder the graph

use crate::{
    priority_queue::PriorityQueue,
    serialize::{Link, LinkWidth, ObjIdx, Object, OffsetWhence, SerializeErrorFlags, Serializer},
};
use fnv::FnvHashMap;
use write_fonts::{read::collections::IntSet, types::Uint24};

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) struct RepackErrorFlags(u32);

impl RepackErrorFlags {
    pub(crate) const ERROR_NONE: Self = Self(0x0000);
    pub(crate) const GRAPH_ERROR_ORPHANED_NODES: Self = Self(0x0001);
    pub(crate) const GRAPH_ERROR_INVALID_OBJ_INDEX: Self = Self(0x0002);
    pub(crate) const GRAPH_ERROR_INVALID_LINK_POSITION: Self = Self(0x0004);
    pub(crate) const GRAPH_ERROR_CYCLE_DETECTED: Self = Self(0x0008);
    #[allow(dead_code)]
    pub(crate) const GRAPH_ERROR_INVALID_ROOT: Self = Self(0x0010);
    pub(crate) const REPACK_ERROR_SERIALIZE: Self = Self(0x0020);
    #[allow(dead_code)]
    pub(crate) const REPACK_ERROR_SPLIT_SUBTABLE: Self = Self(0x0040);
    #[allow(dead_code)]
    pub(crate) const REPACK_ERROR_EXT_PROMOTION: Self = Self(0x0080);
    #[allow(dead_code)]
    pub(crate) const REPACK_ERROR_NO_RESOLUTION: Self = Self(0x0100);
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
    start: usize,
    end: usize,
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

    fn remove_parent(&mut self, parent_idx: ObjIdx) {
        let Some(num_edges) = self.parents.get_mut(&parent_idx) else {
            return;
        };

        self.incoming_edges -= 1;
        if *num_edges > 1 {
            *num_edges -= 1;
        } else {
            self.parents.remove(&parent_idx);
        }
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

impl Clone for Vertex {
    fn clone(&self) -> Self {
        Self {
            head: self.head,
            tail: self.tail,
            real_links: self.real_links.clone(),
            virtual_links: self.virtual_links.clone(),
            distance: self.distance,
            space: self.space,
            ..Default::default()
        }
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

    // Generates a new topological sorting of graph ordered by the shortest
    // distance to each node if positions are marked as invalid.
    pub(crate) fn sort_shortest_distance_if_needed(&mut self) -> Result<(), RepackErrorFlags> {
        if !self.positions_invalid {
            return Ok(());
        }
        self.sort_shortest_distance()
    }

    // Generates a new topological sorting of graph ordered by the shortest
    // distance to each node.
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

    pub(crate) fn is_fully_connected(&mut self) -> Result<(), RepackErrorFlags> {
        self.update_parents()?;

        // Root cannot have parents
        if self.root().incoming_edges() > 0 {
            return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_INVALID_ROOT));
        }

        let root_idx = self.root_idx();
        if self
            .vertices
            .iter()
            .take(root_idx)
            .any(|v| v.incoming_edges() == 0)
        {
            return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_ORPHANED_NODES));
        }
        Ok(())
    }

    fn total_size_in_bytes(&self) -> usize {
        self.vertices
            .iter()
            .map(|v| v.tail - v.head)
            .reduce(|acc, e| acc + e)
            .unwrap_or(0)
    }

    pub(crate) fn serialize(&self) -> Result<Vec<u8>, SerializeErrorFlags> {
        let mut s = Serializer::new(self.total_size_in_bytes());
        s.start_serialize()?;

        let vertices = &self.vertices;
        let data = &self.data;
        // ref: <https://github.com/harfbuzz/harfbuzz/blob/07ee609f5abe59b591e4a6cf99db890be556501b/src/graph/serialize.hh#L245>
        let mut id_map = vec![0; self.ordering.len()];
        for i in self.ordering.iter().rev() {
            let v = &vertices[*i];

            let obj_bytes = &data[v.head..v.tail];
            let obj_size = obj_bytes.len();
            s.push()?;
            let start = s.embed_bytes(obj_bytes)?;

            for (link_pos, link) in &v.real_links {
                serialize_link(&mut s, link, *link_pos as usize, start, obj_size, &id_map)?;
            }

            let new_idx = s
                .pop_pack(false)
                .ok_or(SerializeErrorFlags::SERIALIZE_ERROR_OTHER)?;
            id_map[*i] = new_idx;
        }
        s.end_serialize();

        if s.in_error() {
            return Err(s.error());
        }

        Ok(s.copy_bytes())
    }

    // compute the serialized start/end positions for each vertex
    fn update_positions(&mut self) {
        if !self.positions_invalid {
            return;
        }

        let mut cur_pos = 0;
        let vertices = &mut self.vertices;
        for i in &self.ordering {
            let v = &mut vertices[*i];
            v.start = cur_pos;
            cur_pos += v.tail - v.head;
            v.end = cur_pos;
        }

        self.positions_invalid = false;
    }

    #[inline]
    fn offset_overflows(&self, parent_v: &Vertex, link: &Link) -> bool {
        let vertices = &self.vertices;
        let child_v = &vertices[link.obj_idx()];
        let mut offset = match link.whence() {
            OffsetWhence::Head => child_v.start - parent_v.start,
            OffsetWhence::Tail => child_v.start - parent_v.end,
            OffsetWhence::Absolute => child_v.start,
        };

        let bias = link.bias() as usize;
        assert!(offset >= bias);
        offset -= bias;

        //TODO: support signed offset?
        match link.link_width() {
            LinkWidth::Two => offset > u16::MAX as usize,
            LinkWidth::Three => offset > (Uint24::MAX).to_u32() as usize,
            LinkWidth::Four => offset > u32::MAX as usize,
            LinkWidth::Zero => false,
        }
    }

    //TODO: store all overflow info if needed
    pub(crate) fn has_overflows(&mut self) -> bool {
        self.update_positions();
        let vertices = &self.vertices;
        for parent_idx in &self.ordering {
            let parent_v = &vertices[*parent_idx];
            for link in parent_v.real_links.values() {
                if self.offset_overflows(parent_v, link) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn assign_spaces(&mut self) -> Result<bool, RepackErrorFlags> {
        self.update_parents()?;
        let (mut visited, mut roots) = self.find_space_roots()?;
        if roots.is_empty() {
            return Ok(false);
        }

        visited.invert();
        loop {
            let Some(next) = roots.first() else {
                break;
            };
            let mut connected_roots = IntSet::empty();
            self.find_connected_nodes(
                next as usize,
                &mut roots,
                &mut visited,
                &mut connected_roots,
            );

            self.isolate_subgraph(&mut connected_roots)?;
            let next_space = self.next_space() as u32;

            self.num_roots_for_space
                .push(connected_roots.len() as usize);
            self.distance_invalid = true;
            self.positions_invalid = true;

            let vertices = &mut self.vertices;
            for obj_idx in connected_roots.iter() {
                vertices[obj_idx as usize].space = next_space;
            }
        }
        Ok(true)
    }

    // Finds all nodes in targets that are reachable from start_idx, nodes in visited will be skipped.
    // For this search the graph is treated as being undirected.
    // Connected targets will be added to connected and removed from targets. All visited nodes will be added to visited.
    fn find_connected_nodes(
        &self,
        start_idx: ObjIdx,
        targets: &mut IntSet<u32>,
        visited: &mut IntSet<u32>,
        connected: &mut IntSet<u32>,
    ) {
        if !visited.insert(start_idx as u32) {
            return;
        }

        if targets.remove(start_idx as u32) {
            connected.insert(start_idx as u32);
        }

        let v = &self.vertices[start_idx];
        for l in v.real_links.values().chain(v.virtual_links.iter()) {
            self.find_connected_nodes(l.obj_idx(), targets, visited, connected);
        }

        for p in v.parents.keys() {
            self.find_connected_nodes(*p, targets, visited, connected);
        }
    }

    fn find_space_roots(&self) -> Result<(IntSet<u32>, IntSet<u32>), RepackErrorFlags> {
        let root_idx = self.root_idx();
        let mut visited = IntSet::empty();
        let mut roots = IntSet::empty();
        let vertices = &self.vertices;
        for i in &self.ordering {
            if visited.contains(*i as u32) {
                continue;
            }
            let Some(v) = vertices.get(*i) else {
                return Err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX);
            };
            for l in v.real_links.values() {
                if l.is_signed() {
                    continue;
                }

                match l.link_width() {
                    LinkWidth::Three => {
                        if *i == root_idx {
                            continue;
                        }
                        let mut sub_roots = IntSet::empty();
                        self.find_32bit_roots(l.obj_idx(), &mut sub_roots);
                        for idx in sub_roots.iter() {
                            roots.insert(idx);
                            self.find_subgraph_nodes(idx as usize, &mut visited);
                        }
                    }
                    LinkWidth::Four => {
                        let obj_idx = l.obj_idx();
                        roots.insert(obj_idx as u32);
                        self.find_subgraph_nodes(obj_idx, &mut visited);
                    }
                    _ => continue,
                }
            }
        }
        Ok((roots, visited))
    }

    fn find_subgraph_nodes(&self, obj_idx: ObjIdx, subgraph: &mut IntSet<u32>) {
        if !subgraph.insert(obj_idx as u32) {
            return;
        }

        let v = &self.vertices[obj_idx];
        for l in v.real_links.values().chain(v.virtual_links.iter()) {
            self.find_subgraph_nodes(l.obj_idx(), subgraph);
        }
    }

    fn find_subgraph_nodes_incoming_edges(
        &self,
        start_idx: ObjIdx,
        subgraph_map: &mut FnvHashMap<u32, usize>,
    ) {
        let v = &self.vertices[start_idx];
        for l in v.real_links.values().chain(v.virtual_links.iter()) {
            let obj_idx = l.obj_idx();
            let v = subgraph_map
                .entry(obj_idx as u32)
                .and_modify(|c| *c += 1)
                .or_insert(1);

            if *v > 1 {
                continue;
            }
            self.find_subgraph_nodes_incoming_edges(obj_idx, subgraph_map);
        }
    }

    // Finds the topmost children of 32bit offsets in the subgraph starting at obj_idx
    fn find_32bit_roots(&self, obj_idx: ObjIdx, roots: &mut IntSet<u32>) {
        let v = &self.vertices[obj_idx];
        for l in v.real_links.values() {
            let obj_idx = l.obj_idx();
            if !l.is_signed() && l.link_width() == LinkWidth::Four {
                roots.insert(obj_idx as u32);
                continue;
            }
            self.find_32bit_roots(obj_idx, roots);
        }
    }

    // Isolates the subgraph of nodes reachable from root. Any links to nodes in the subgraph
    // that originate from outside of the subgraph will be removed by duplicating the linked to
    // object
    // Indices stored in roots will be updated if any of the roots are duplicated to new indices.
    fn isolate_subgraph(&mut self, roots: &mut IntSet<u32>) -> Result<(), RepackErrorFlags> {
        self.update_parents()?;

        let mut parents = IntSet::empty();
        let mut subgraph_map = FnvHashMap::default();
        for root_idx in roots.iter() {
            subgraph_map.insert(root_idx, self.wide_parents(root_idx as usize, &mut parents));
            self.find_subgraph_nodes_incoming_edges(root_idx as usize, &mut subgraph_map);
        }

        let len = self.vertices.len();
        let mut index_map = FnvHashMap::default();
        for (idx, num_incoming_edges) in subgraph_map.iter() {
            let obj_idx = *idx as usize;
            assert!(obj_idx < len);
            // duplicate objects with incoming links from outside the subgraph.
            if *num_incoming_edges < self.vertices[obj_idx].incoming_edges() {
                self.duplicate_subgraph(obj_idx, &mut index_map);
            }
        }

        if index_map.is_empty() {
            return Ok(());
        }

        let new_subgraph = subgraph_map
            .keys()
            .map(|idx| {
                index_map
                    .get(&(*idx as usize))
                    .copied()
                    .unwrap_or(*idx as usize)
            })
            .map(|i| i as u32)
            .collect();

        self.remap_obj_indices(&index_map, new_subgraph, false)?;
        self.remap_obj_indices(&index_map, parents, true)?;

        for (old, new) in index_map.iter() {
            if roots.remove(*old as u32) {
                roots.insert(*new as u32);
            }
        }
        Ok(())
    }

    fn remap_obj_indices(
        &mut self,
        index_map: &FnvHashMap<usize, usize>,
        it: IntSet<u32>,
        only_wide: bool,
    ) -> Result<(), RepackErrorFlags> {
        let mut old_to_new_idx_parents = Vec::new();
        for i in it.iter() {
            let parent_idx = i as usize;
            let Some(obj) = self.vertices.get_mut(i as usize) else {
                return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX));
            };
            for l in obj.real_links.values_mut() {
                let old_idx = l.obj_idx();
                let Some(new_idx) = index_map.get(&old_idx) else {
                    continue;
                };

                if only_wide
                    && (l.is_signed()
                        || (l.link_width() != LinkWidth::Four
                            && l.link_width() != LinkWidth::Three))
                {
                    continue;
                }
                l.update_obj_idx(*new_idx);
                old_to_new_idx_parents.push((old_idx, *new_idx, parent_idx, false));
            }

            for l in &mut obj.virtual_links {
                let old_idx = l.obj_idx();
                let Some(new_idx) = index_map.get(&old_idx) else {
                    continue;
                };

                if only_wide
                    && (l.is_signed()
                        || (l.link_width() != LinkWidth::Four
                            && l.link_width() != LinkWidth::Three))
                {
                    continue;
                }
                l.update_obj_idx(*new_idx);
                old_to_new_idx_parents.push((old_idx, *new_idx, parent_idx, true));
            }
        }
        self.reassign_parents(&old_to_new_idx_parents)
    }

    // Also Corrects the parents map on the previous and new child nodes.
    fn reassign_parents(
        &mut self,
        old_to_new_idx_parents: &[(usize, usize, usize, bool)],
    ) -> Result<(), RepackErrorFlags> {
        let vertices = &mut self.vertices;
        for (old_idx, new_idx, parent_idx, is_virtual) in old_to_new_idx_parents {
            let Some(old_v) = vertices.get_mut(*old_idx) else {
                return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX));
            };
            old_v.remove_parent(*parent_idx);

            let Some(new_v) = vertices.get_mut(*new_idx) else {
                return Err(self.set_err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX));
            };
            new_v.add_parent(*parent_idx, *is_virtual);
        }
        Ok(())
    }

    // duplicates all nodes in the subgraph reachable from start_idx. Does not re-assign
    // links. index_map is updated with mappings from old idx to new idx.
    // If a duplication has already been performed for a given index, then it will be skipped.
    fn duplicate_subgraph(&mut self, start_idx: ObjIdx, index_map: &mut FnvHashMap<usize, usize>) {
        if index_map.contains_key(&start_idx) {
            return;
        }

        let clone_idx = self.duplicate_obj(start_idx);
        index_map.insert(start_idx, clone_idx);

        let start_v = &self.vertices[start_idx];
        let child_idxes: Vec<ObjIdx> = start_v
            .real_links
            .values()
            .chain(start_v.virtual_links.iter())
            .map(|l| l.obj_idx())
            .collect();
        for idx in child_idxes {
            self.duplicate_subgraph(idx, index_map);
        }
    }

    // Creates a copy of the specified obj and returns the new obj_idx.
    fn duplicate_obj(&mut self, obj_idx: ObjIdx) -> ObjIdx {
        self.positions_invalid = true;
        self.distance_invalid = true;

        let clone_idx = self.vertices.len();
        self.ordering.push(clone_idx);

        let new_v = self.vertices[obj_idx].clone();
        let vertices = &mut self.vertices;

        for l in new_v.real_links.values() {
            let child_idx = l.obj_idx();
            vertices[child_idx].add_parent(clone_idx, false);
        }

        for l in &new_v.virtual_links {
            let child_idx = l.obj_idx();
            vertices[child_idx].add_parent(clone_idx, true);
        }

        self.vertices.push(new_v);
        clone_idx
    }

    // returns the number of incoming edges that are 24 or 32 bits wide
    // and parent obj_idx will be added into the parents set
    fn wide_parents(&self, obj_idx: ObjIdx, parents_set: &mut IntSet<u32>) -> usize {
        let vertices = &self.vertices;
        let v = &vertices[obj_idx];
        let mut count = 0;
        for p in v.parents.keys() {
            let parent_v = &vertices[*p];
            for l in parent_v.real_links.values() {
                let width = l.link_width() as u8;
                if l.obj_idx() == obj_idx && (width == 3 || width == 4) && !l.is_signed() {
                    count += 1;
                    parents_set.insert(*p as u32);
                }
            }
        }
        count
    }

    fn next_space(&self) -> usize {
        self.num_roots_for_space.len()
    }
}

fn serialize_link(
    s: &mut Serializer,
    link: &Link,
    link_pos: usize,
    start: usize,
    obj_size: usize,
    id_map: &[usize],
) -> Result<(), SerializeErrorFlags> {
    let link_width = link.link_width() as usize;
    if link_width == 0 {
        return Ok(());
    }
    assert!(link_pos + link_width <= obj_size);

    let offset_pos = start + link_pos;
    let Some(offset_data) = s.get_mut_data(offset_pos..offset_pos + link_width) else {
        return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER));
    };

    offset_data.fill(0);
    let new_obj_idx = id_map
        .get(link.obj_idx())
        .ok_or_else(|| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER))?;

    s.add_link(
        offset_pos..offset_pos + link_width,
        *new_obj_idx,
        link.whence(),
        link.bias(),
        link.is_signed(),
    )
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;
    use crate::serialize::OffsetWhence;
    use write_fonts::types::{FixedSize, Offset16, Offset32, Scalar};

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
        pub(crate) fn normalize(&mut self) {
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
        s.embed_bytes(&bytes[0..len]).unwrap();
    }

    fn start_object(s: &mut Serializer, bytes: &[u8], len: usize) {
        s.push().unwrap();
        extend(s, bytes, len);
    }

    fn add_object(s: &mut Serializer, bytes: &[u8], len: usize) -> ObjIdx {
        start_object(s, bytes, len);
        s.pop_pack(false).unwrap()
    }

    fn add_typed_offset<T: Scalar>(s: &mut Serializer, obj_idx: ObjIdx) {
        let offset_pos = s.allocate_size(T::RAW_BYTE_LEN, true).unwrap();
        s.add_link(
            offset_pos..offset_pos + T::RAW_BYTE_LEN,
            obj_idx,
            OffsetWhence::Head,
            0,
            false,
        )
        .unwrap();
    }

    fn add_offset(s: &mut Serializer, obj_idx: ObjIdx) {
        add_typed_offset::<Offset16>(s, obj_idx);
    }

    fn add_wide_offset(s: &mut Serializer, obj_idx: ObjIdx) {
        add_typed_offset::<Offset32>(s, obj_idx);
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

    fn populate_serializer_simple(s: &mut Serializer) {
        let _ = s.start_serialize();
        let obj_1 = add_object(s, b"ghi", 3);
        let obj_2 = add_object(s, b"def", 3);
        start_object(s, b"abc", 3);
        add_offset(s, obj_2);
        add_offset(s, obj_1);
        s.pop_pack(false);
        s.end_serialize();
    }

    pub(crate) fn populate_serializer_with_overflow(s: &mut Serializer) {
        let large_bytes = [b'a'; 50000];
        let _ = s.start_serialize();
        let obj_1 = add_object(s, &large_bytes, 10000);
        let obj_2 = add_object(s, &large_bytes, 20000);
        let obj_3 = add_object(s, &large_bytes, 50000);

        start_object(s, b"abc", 3);
        add_offset(s, obj_3);
        add_offset(s, obj_2);
        add_offset(s, obj_1);
        s.pop_pack(false);

        s.end_serialize();
    }

    fn populate_serializer_with_dedup_overflow(s: &mut Serializer) {
        let large_bytes = [b'a'; 70000];
        let _ = s.start_serialize();
        let obj_1 = add_object(s, b"def", 3);

        start_object(s, &large_bytes, 60000);
        add_offset(s, obj_1);
        let obj_2 = s.pop_pack(false).unwrap();

        start_object(s, &large_bytes, 10000);
        add_offset(s, obj_2);
        add_offset(s, obj_1);
        s.pop_pack(false);

        s.end_serialize();
    }

    pub(crate) fn populate_serializer_spaces(s: &mut Serializer, with_overflow: bool) {
        let large_string = [b'a'; 70000];
        s.start_serialize().unwrap();

        let obj_i = if with_overflow {
            add_object(s, b"i", 1)
        } else {
            0
        };

        // space 2
        let obj_h = add_object(s, b"h", 1);
        start_object(s, &large_string, 30000);
        add_offset(s, obj_h);

        let obj_e = s.pop_pack(false).unwrap();
        start_object(s, b"b", 1);
        add_offset(s, obj_e);
        let obj_b = s.pop_pack(false).unwrap();

        // space 1
        let obj_i = if !with_overflow {
            add_object(s, b"i", 1)
        } else {
            obj_i
        };

        start_object(s, &large_string, 30000);
        add_offset(s, obj_i);
        let obj_g = s.pop_pack(false).unwrap();

        start_object(s, &large_string, 30000);
        add_offset(s, obj_i);
        let obj_f = s.pop_pack(false).unwrap();

        start_object(s, b"d", 1);
        add_offset(s, obj_g);
        let obj_d = s.pop_pack(false).unwrap();

        start_object(s, b"c", 1);
        add_offset(s, obj_f);
        let obj_c = s.pop_pack(false).unwrap();

        start_object(s, b"a", 1);
        add_wide_offset(s, obj_b);
        add_wide_offset(s, obj_c);
        add_wide_offset(s, obj_d);
        s.pop_pack(false).unwrap();
        s.end_serialize();
    }

    #[test]
    fn test_graph_serialize() {
        let buf_size = 100;
        let mut s1 = Serializer::new(buf_size);
        populate_serializer_simple(&mut s1);
        let graph = Graph::from_serializer(&s1).unwrap();

        let expected_bytes = s1.copy_bytes();
        let out = graph.serialize().unwrap();
        assert_eq!(out, expected_bytes);
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

    #[test]
    fn test_has_overflows_1() {
        let buf_size = 100;
        let mut s = Serializer::new(buf_size);
        populate_serializer_complex_2(&mut s);
        let mut graph = Graph::from_serializer(&s).unwrap();

        assert!(!graph.has_overflows());
    }

    #[test]
    fn test_has_overflows_2() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_overflow(&mut s);
        let mut graph = Graph::from_serializer(&s).unwrap();

        assert!(graph.has_overflows());
    }

    #[test]
    fn test_has_overflows_3() {
        let buf_size = 160000;
        let mut s = Serializer::new(buf_size);
        populate_serializer_with_dedup_overflow(&mut s);
        let mut graph = Graph::from_serializer(&s).unwrap();

        assert!(graph.has_overflows());
    }
}
