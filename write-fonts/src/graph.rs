//! A graph for resolving table offsets

use font_types::Uint24;

use super::write::TableData;
use std::{
    collections::{BinaryHeap, HashMap, HashSet, VecDeque},
    sync::atomic::AtomicU64,
};

#[cfg(feature = "dot2")]
mod graphviz;

static OBJECT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// An identifier for an object in the compilation graph.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, Hash, PartialEq, Eq)]
pub struct ObjectId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum OffsetLen {
    Offset16 = 2,
    Offset24 = 3,
    Offset32 = 4,
}

impl OffsetLen {
    /// The maximum value for an offset of this length.
    pub const fn max_value(self) -> u32 {
        match self {
            Self::Offset16 => u16::MAX as u32,
            Self::Offset24 => (1 << 24) - 1,
            Self::Offset32 => u32::MAX,
        }
    }
}

impl std::fmt::Display for OffsetLen {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Offset16 => write!(f, "Offset16"),
            Self::Offset24 => write!(f, "Offset24"),
            Self::Offset32 => write!(f, "Offset32"),
        }
    }
}
/// A ranking used for sorting the graph.
///
/// Nodes are assigned a space, and nodes in lower spaces are always
/// packed before nodes in higher spaces.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, Hash, PartialEq, Eq)]
pub struct Space(u32);

impl Space {
    /// A generic space for nodes reachable via 16-bit offsets.
    const SHORT_REACHABLE: Space = Space(0);
    /// A generic space for nodes that are reachable via any offset.
    const REACHABLE: Space = Space(1);
    /// The first space used for assignment to specific subgraphs.
    const INIT: Space = Space(2);

    const fn is_custom(self) -> bool {
        self.0 >= Space::INIT.0
    }
}

impl ObjectId {
    pub fn next() -> Self {
        ObjectId(OBJECT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

#[derive(Debug, Default)]
pub(crate) struct ObjectStore {
    pub(crate) objects: HashMap<TableData, ObjectId>,
}

impl ObjectStore {
    pub(crate) fn add(&mut self, data: TableData) -> ObjectId {
        *self.objects.entry(data).or_insert_with(ObjectId::next)
    }
}

/// A graph of subtables, starting at a single root.
///
/// This type is used during compilation, to determine the final write order
/// for the various subtables.
//NOTE: we don't derive Debug because it's way too verbose to be useful
pub struct Graph {
    /// the actual data for each table
    objects: HashMap<ObjectId, TableData>,
    /// graph-specific state used for sorting
    nodes: HashMap<ObjectId, Node>,
    order: Vec<ObjectId>,
    root: ObjectId,
    parents_invalid: bool,
    distance_invalid: bool,
    positions_invalid: bool,
    next_space: Space,
    num_roots_per_space: HashMap<Space, usize>,
}

#[derive(Debug)]
struct Node {
    size: u32,
    distance: u32,
    /// overall position after sorting
    position: u32,
    space: Space,
    parents: Vec<(ObjectId, OffsetLen)>,
    priority: Priority,
}

/// Scored used when computing shortest distance
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Distance {
    // a space ranking; like rankings are packed together,
    // and larger rankings are packed after smaller ones.
    space: Space,
    distance: u64,
    // a tie-breaker, based on order within a parent
    order: u32,
}

//TODO: remove me? maybe? not really used right now...
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Priority(u8);

/// A record of an overflowing offset
pub(crate) struct Overflow {
    parent: ObjectId,
    child: ObjectId,
    distance: u32,
    offset_type: OffsetLen,
}

impl Priority {
    const ZERO: Priority = Priority(0);
    const ONE: Priority = Priority(1);
    const TWO: Priority = Priority(2);
    const THREE: Priority = Priority(3);

    #[cfg(test)]
    fn increase(&mut self) -> bool {
        let result = *self != Priority::THREE;
        self.0 = (self.0 + 1).min(3);
        result
    }
}

impl Distance {
    const ROOT: Distance = Distance {
        space: Space::SHORT_REACHABLE,
        distance: 0,
        order: 0,
    };

    fn rev(self) -> std::cmp::Reverse<Distance> {
        std::cmp::Reverse(self)
    }
}

impl Node {
    pub fn new(size: u32) -> Self {
        Node {
            //obj,
            size,
            position: Default::default(),
            distance: Default::default(),
            space: Space::REACHABLE,
            parents: Default::default(),
            priority: Default::default(),
        }
    }

    #[cfg(test)]
    fn raise_priority(&mut self) -> bool {
        self.priority.increase()
    }

    fn modified_distance(&self, order: u32) -> Distance {
        let prev_dist = self.distance as i64;
        let distance = match self.priority {
            Priority::ZERO => prev_dist,
            Priority::ONE => prev_dist - self.size as i64 / 2,
            Priority::TWO => prev_dist - self.size as i64,
            Priority::THREE => 0,
            _ => 0,
        }
        .max(0) as u64;

        Distance {
            space: self.space,
            distance,
            order,
        }
    }
}

impl Graph {
    pub(crate) fn from_obj_store(store: ObjectStore, root: ObjectId) -> Self {
        let objects = store
            .objects
            .into_iter()
            .map(|(k, v)| (v, k))
            .collect::<HashMap<_, _>>();
        Self::from_objects(objects, root)
    }

    fn from_objects(objects: HashMap<ObjectId, TableData>, root: ObjectId) -> Self {
        let nodes = objects
            .iter()
            //TODO: ensure table sizes elsewhere?
            .map(|(key, obj)| (*key, Node::new(obj.bytes.len().try_into().unwrap())))
            .collect::<HashMap<_, _>>();
        Graph {
            objects,
            nodes,
            order: Default::default(),
            root,
            parents_invalid: true,
            distance_invalid: true,
            positions_invalid: true,
            next_space: Space::INIT,
            num_roots_per_space: Default::default(),
        }
    }

    /// Write out the serialized graph.
    ///
    /// This is not public API, and you are responsible for ensuring that
    /// the graph is sorted before calling (by calling `pack_objects`, and
    /// checking that it has succeded).
    pub(crate) fn serialize(&self) -> Vec<u8> {
        fn write_offset(at: &mut [u8], len: OffsetLen, resolved: u32) {
            let at = &mut at[..len as u8 as usize];
            match len {
                OffsetLen::Offset16 => at.copy_from_slice(
                    u16::try_from(resolved)
                        .expect("offset overflow should be checked before now")
                        .to_be_bytes()
                        .as_slice(),
                ),
                OffsetLen::Offset24 => at.copy_from_slice(
                    Uint24::checked_new(resolved)
                        .expect("offset overflow should be checked before now")
                        .to_be_bytes()
                        .as_slice(),
                ),
                OffsetLen::Offset32 => at.copy_from_slice(resolved.to_be_bytes().as_slice()),
            }
        }

        assert!(
            !self.order.is_empty(),
            "graph must be sorted before serialization"
        );
        let mut offsets = HashMap::new();
        let mut out = Vec::new();
        let mut off = 0;

        // first pass: write out bytes, record positions of offsets
        for id in &self.order {
            let node = self.objects.get(id).unwrap();
            offsets.insert(*id, off);
            off += node.bytes.len() as u32;
            out.extend_from_slice(&node.bytes);
        }

        // second pass: write offsets
        let mut table_head = 0;
        for id in &self.order {
            let node = self.objects.get(id).unwrap();
            for offset in &node.offsets {
                let abs_off = *offsets
                    .get(&offset.object)
                    .expect("all offsets visited in first pass");
                let rel_off = abs_off - (table_head + offset.adjustment);
                let buffer_pos = table_head + offset.pos;
                let write_over = out.get_mut(buffer_pos as usize..).unwrap();
                write_offset(write_over, offset.len, rel_off);
            }
            table_head += node.bytes.len() as u32;
        }
        out
    }

    /// Attempt to pack the graph.
    ///
    /// This involves finding an order for objects such that all offsets are
    /// resolveable.
    ///
    /// In the simple case, this just means finding a topological ordering.
    /// In exceptional cases, however, this may require us to significantly
    /// modify the graph.
    ///
    /// Our implementation is closely modeled on the implementation in the
    /// HarfBuzz repacker; see the [repacker docs] for further detail.
    ///
    /// returns `true` if a solution is found, `false` otherwise
    ///
    /// [repacker docs]: https://github.com/harfbuzz/harfbuzz/blob/main/docs/repacker.md
    pub(crate) fn pack_objects(&mut self) -> bool {
        if self.basic_sort() {
            return true;
        }

        self.assign_spaces_hb();
        self.sort_shortest_distance();

        if !self.has_overflows() {
            return true;
        }

        // now isolate spaces in a loop, until there are no more left:
        let overflows = loop {
            let overflows = self.find_overflows();
            if overflows.is_empty() {
                // we're done
                return true;
            }
            log::info!(
                "failed with {} overflows, current size {}",
                overflows.len(),
                self.debug_len()
            );
            if !self.try_splitting_subgraphs(&overflows) {
                log::info!("finished isolating all subgraphs without solution");
                break overflows;
            }
            self.sort_shortest_distance();
        };

        assert!(!overflows.is_empty());
        self.debug_overflows(&overflows);
        false
    }

    /// Initial sorting operation. Attempt Kahn, falling back to shortest distance.
    ///
    /// This has to be called first, since it establishes an initial order.
    /// subsequent operations on the graph require this order.
    ///
    /// returns `true` if sort succeeds with no overflows
    fn basic_sort(&mut self) -> bool {
        log::info!("sorting {} objects", self.objects.len());

        self.sort_kahn();
        if !self.has_overflows() {
            return true;
        }
        log::info!("kahn failed, trying shortest distance");
        self.sort_shortest_distance();
        !self.has_overflows()
    }

    fn has_overflows(&self) -> bool {
        for (parent_id, data) in &self.objects {
            let parent = &self.nodes[parent_id];
            for link in &data.offsets {
                let child = &self.nodes[&link.object];
                //TODO: account for 'whence'
                let rel_off = child.position - parent.position;
                if link.len.max_value() < rel_off {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn find_overflows(&self) -> Vec<Overflow> {
        let mut result = Vec::new();
        for (parent_id, data) in &self.objects {
            let parent = &self.nodes[parent_id];
            for link in &data.offsets {
                let child = &self.nodes[&link.object];
                //TODO: account for 'whence'
                let rel_off = child.position - parent.position;
                if link.len.max_value() < rel_off {
                    result.push(Overflow {
                        parent: *parent_id,
                        child: link.object,
                        distance: rel_off,
                        offset_type: link.len,
                    });
                }
            }
        }
        result
    }

    fn debug_overflows(&self, overflows: &[Overflow]) {
        let (parents, children): (HashSet<_>, HashSet<_>) =
            overflows.iter().map(|x| (x.parent, x.child)).unzip();
        log::debug!(
            "found {} overflows from {} parents to {} children",
            overflows.len(),
            parents.len(),
            children.len()
        );

        for overflow in overflows {
            log::debug!(
                "{:?} -> {:?} type {} dist {}",
                overflow.parent,
                overflow.child,
                overflow.offset_type,
                overflow.distance
            );
        }
    }

    // only valid if order is up to date. Returns total byte len of graph.
    fn debug_len(&self) -> usize {
        self.order
            .iter()
            .map(|id| self.objects.get(id).unwrap().bytes.len())
            .sum()
    }

    fn update_parents(&mut self) {
        if !self.parents_invalid {
            return;
        }
        for node in self.nodes.values_mut() {
            node.parents.clear();
        }

        for (id, obj) in &self.objects {
            for link in &obj.offsets {
                self.nodes
                    .get_mut(&link.object)
                    .unwrap()
                    .parents
                    .push((*id, link.len));
            }
        }
        self.parents_invalid = false;
    }

    fn sort_kahn(&mut self) {
        self.positions_invalid = true;
        if self.nodes.len() <= 1 {
            self.order.extend(self.nodes.keys().copied());
            return;
        }

        let mut queue = BinaryHeap::new();
        let mut removed_edges = HashMap::new();
        let mut current_pos: u32 = 0;
        self.order.clear();

        self.update_parents();
        queue.push(std::cmp::Reverse(self.root));

        while let Some(id) = queue.pop().map(|x| x.0) {
            let next = &self.objects[&id];
            self.order.push(id);
            self.nodes.get_mut(&id).unwrap().position = current_pos;
            current_pos += next.bytes.len() as u32;
            for link in &next.offsets {
                let seen_edges = removed_edges.entry(link.object).or_insert(0usize);
                *seen_edges += 1;
                // if the target of this link has no other incoming links, add
                // to the queue
                if *seen_edges == self.nodes[&link.object].parents.len() {
                    queue.push(std::cmp::Reverse(link.object));
                }
            }
        }
        //TODO: check for orphans & cycles?
        for (id, seen_len) in &removed_edges {
            if *seen_len != self.nodes[id].parents.len() {
                panic!("cycle or something?");
            }
        }
    }

    pub(crate) fn sort_shortest_distance(&mut self) {
        self.positions_invalid = true;
        self.update_parents();
        self.update_distances();
        self.assign_space_0();

        let mut queue = BinaryHeap::new();
        let mut removed_edges = HashMap::with_capacity(self.nodes.len());
        let mut current_pos = 0;
        self.order.clear();

        queue.push((Distance::ROOT.rev(), self.root));
        let mut obj_order = 1u32;
        while let Some((_, id)) = queue.pop() {
            let next = &self.objects[&id];
            self.order.push(id);
            self.nodes.get_mut(&id).unwrap().position = current_pos;
            current_pos += next.bytes.len() as u32;
            for link in &next.offsets {
                let seen_edges = removed_edges.entry(link.object).or_insert(0usize);
                *seen_edges += 1;
                // if the target of this link has no other incoming links, add
                // to the queue
                if *seen_edges == self.nodes[&link.object].parents.len() {
                    let distance = self.nodes[&link.object].modified_distance(obj_order);
                    obj_order += 1;
                    queue.push((distance.rev(), link.object));
                }
            }
        }

        //TODO: check for orphans & cycles?
        for (id, seen_len) in &removed_edges {
            if *seen_len != self.nodes[id].parents.len() {
                panic!("cycle or something?");
            }
        }
    }

    fn update_distances(&mut self) {
        self.nodes
            .values_mut()
            .for_each(|node| node.distance = u32::MAX);
        self.nodes.get_mut(&self.root).unwrap().distance = u32::MIN;

        let mut queue = BinaryHeap::new();
        let mut visited = HashSet::new();
        queue.push((Default::default(), self.root));

        while let Some((_, next_id)) = queue.pop() {
            if !visited.insert(next_id) {
                continue;
            }
            let next_distance = self.nodes[&next_id].distance;
            let next_obj = &self.objects[&next_id];
            for link in &next_obj.offsets {
                if visited.contains(&link.object) {
                    continue;
                }

                let child = self.nodes.get_mut(&link.object).unwrap();
                let child_distance = next_distance + child.size;

                if child_distance < child.distance {
                    child.distance = child_distance;
                    queue.push((child_distance, link.object));
                }
            }
        }

        self.distance_invalid = false;
    }

    /// isolate and assign spaces to subgraphs reachable via long offsets.
    ///
    /// This finds all subgraphs that are reachable via long offsets, and
    /// isolates them (ensuring they are *only* reachable via long offsets),
    /// assigning each unique space an identifier.
    ///
    /// Each space may have multiple roots; this works by finding the connected
    /// components from each root (counting only nodes reachable via long offsets).
    ///
    /// This is a close port of the [assign_spaces] method used by the HarfBuzz
    /// repacker.
    ///
    /// [assign_spaces]: https://github.com/harfbuzz/harfbuzz/blob/main/src/graph/graph.hh#L624
    fn assign_spaces_hb(&mut self) -> bool {
        self.update_parents();
        let (visited, mut roots) = self.find_space_roots_hb();

        if roots.is_empty() {
            return false;
        }

        log::debug!("found {} space roots to isolate", roots.len());

        // we want to *invert* the visited set, but we don't have a fancy hb_set_t
        let mut visited = self
            .order
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .difference(&visited)
            .copied()
            .collect::<HashSet<_>>();

        let mut connected_roots = HashSet::new(); // we can reuse this
        while let Some(next) = roots.iter().copied().next() {
            connected_roots.clear();
            self.find_connected_nodes_hb(next, &mut roots, &mut visited, &mut connected_roots);
            self.isolate_subgraph_hb(&mut connected_roots);

            self.distance_invalid = true;
            self.positions_invalid = true;
        }
        true
    }

    /// Find the root nodes of 32 (and later 24?)-bit space.
    ///
    /// These are the set of nodes that have incoming long offsets, for which
    /// no ancestor has an incoming long offset.
    ///
    /// Ported from the [find_space_roots] method in HarfBuzz.
    ///
    /// [find_space_roots]: https://github.com/harfbuzz/harfbuzz/blob/main/src/graph/graph.hh#L508
    fn find_space_roots_hb(&self) -> (HashSet<ObjectId>, HashSet<ObjectId>) {
        let mut visited = HashSet::new();
        let mut roots = HashSet::new();

        let mut queue = VecDeque::from([self.root]);

        while let Some(id) = queue.pop_front() {
            if visited.contains(&id) {
                continue;
            }
            let obj = self.objects.get(&id).unwrap();
            for link in &obj.offsets {
                //FIXME: harfbuzz has a bunch of logic here for 24-bit offsets
                if link.len == OffsetLen::Offset32 {
                    roots.insert(link.object);
                    self.find_subgraph_hb(link.object, &mut visited);
                } else {
                    queue.push_back(link.object);
                }
            }
        }
        (visited, roots)
    }

    fn find_subgraph_hb(&self, idx: ObjectId, nodes: &mut HashSet<ObjectId>) {
        if !nodes.insert(idx) {
            return;
        }
        for link in self.objects.get(&idx).unwrap().offsets.iter() {
            self.find_subgraph_hb(link.object, nodes);
        }
    }

    fn find_subgraph_map_hb(&self, idx: ObjectId, graph: &mut HashMap<ObjectId, usize>) {
        for link in &self.objects[&idx].offsets {
            *graph.entry(link.object).or_default() += 1;
            self.find_subgraph_map_hb(link.object, graph);
        }
    }
    /// find all of the members of 'targets' that are reachable, skipping nodes in `visited`.
    fn find_connected_nodes_hb(
        &self,
        id: ObjectId,
        targets: &mut HashSet<ObjectId>,
        visited: &mut HashSet<ObjectId>,
        connected: &mut HashSet<ObjectId>,
    ) {
        if !visited.insert(id) {
            return;
        }
        if targets.remove(&id) {
            connected.insert(id);
        }
        // recurse to all children and parents
        for (obj, _) in &self.nodes.get(&id).unwrap().parents {
            self.find_connected_nodes_hb(*obj, targets, visited, connected);
        }
        for link in &self.objects.get(&id).unwrap().offsets {
            self.find_connected_nodes_hb(link.object, targets, visited, connected);
        }
    }

    /// Isolate the subgraph with the provided roots, moving it to a new space.
    ///
    /// This duplicates any nodes in this subgraph that are shared with
    /// any other nodes in the graph.
    ///
    /// Based on the [isolate_subgraph] method in HarfBuzz.
    ///
    /// [isolate_subgraph]: https://github.com/harfbuzz/harfbuzz/blob/main/src/graph/graph.hh#L508
    fn isolate_subgraph_hb(&mut self, roots: &mut HashSet<ObjectId>) -> bool {
        self.update_parents();
        log::debug!("isolating subgraph with {} roots", roots.len());

        // map of object id -> number of incoming edges
        let mut subgraph = HashMap::new();

        for root in roots.iter() {
            // for the roots, we set the edge count to the number of long
            // incoming offsets; if this differs from the total number off
            // incoming offsets it means we need to dupe the root as well.
            let inbound_wide_offsets = self.nodes[root]
                .parents
                .iter()
                .filter(|(_, len)| !matches!(len, OffsetLen::Offset16))
                .count();
            subgraph.insert(*root, inbound_wide_offsets);
            self.find_subgraph_map_hb(*root, &mut subgraph);
        }

        let next_space = self.next_space();
        self.num_roots_per_space.insert(next_space, roots.len());
        let mut id_map = HashMap::new();
        //let mut made_changes = false;
        for (id, incoming_edges_in_subgraph) in &subgraph {
            // there are edges to this object from outside the subgraph; dupe it.
            if *incoming_edges_in_subgraph < self.nodes[id].parents.len() {
                self.duplicate_subgraph(*id, &mut id_map, next_space);
            }
        }

        // now remap any links in the subgraph from nodes that were not
        // themselves duplicated (since they were not reachable from outside)
        for id in subgraph.keys().filter(|k| !id_map.contains_key(k)) {
            self.nodes.get_mut(id).unwrap().space = next_space;
            let obj = self.objects.get_mut(id).unwrap();
            for link in &mut obj.offsets {
                if let Some(new_id) = id_map.get(&link.object) {
                    link.object = *new_id;
                }
            }
        }

        if id_map.is_empty() {
            return false;
        }

        // now everything but the links to the roots roots has been remapped;
        // remap those, if needed
        for root in roots.iter() {
            let Some(new_id) = id_map.get(root) else { continue };
            self.parents_invalid = true;
            self.positions_invalid = true;
            for (parent_id, len) in &self.nodes[new_id].parents {
                if !matches!(len, OffsetLen::Offset16) {
                    for link in &mut self.objects.get_mut(parent_id).unwrap().offsets {
                        if link.object == *root {
                            link.object = *new_id;
                        }
                    }
                }
            }
        }

        // if any roots changed, we also rename them in the input set:
        for (old, new) in id_map {
            if roots.remove(&old) {
                roots.insert(new);
            }
        }

        true
    }

    /// for each space that has overflows and > 1 roots, select half the roots
    /// and move them to a separate subgraph.
    //
    /// return `true` if any change was made.
    ///
    /// This is a port of the [_try_isolating_subgraphs] method in hb-repacker.
    ///
    /// [_try_isolating_subgraphs]: https://github.com/harfbuzz/harfbuzz/blob/main/src/hb-repacker.hh#L182
    fn try_splitting_subgraphs(&mut self, overflows: &[Overflow]) -> bool {
        let mut to_isolate = HashMap::new();
        for overflow in overflows {
            let child_space = self.nodes[&overflow.child].space;
            // we only isolate subgraphs in wide-space
            if !child_space.is_custom() || self.num_roots_per_space[&child_space] <= 1 {
                continue;
            }
            let root = self.find_root_of_space(overflow.child);
            debug_assert_eq!(self.nodes[&root].space, child_space);
            to_isolate
                .entry(child_space)
                .or_insert_with(HashSet::new)
                .insert(root);
        }

        if to_isolate.is_empty() {
            return false;
        }

        for (space, mut roots) in to_isolate {
            let max_to_move = self.num_roots_per_space[&space] / 2;
            log::debug!(
                "moving {} of {} candidate roots from {space:?} to new space",
                max_to_move.min(roots.len()),
                roots.len()
            );
            while roots.len() > max_to_move {
                let next = *roots.iter().next().unwrap();
                roots.remove(&next);
            }
            self.isolate_subgraph_hb(&mut roots);
            *self.num_roots_per_space.get_mut(&space).unwrap() -= roots.len();
        }

        true
    }

    // invariant: obj must not be in space 0
    fn find_root_of_space(&self, obj: ObjectId) -> ObjectId {
        let space = self.nodes[&obj].space;
        debug_assert!(space.is_custom());
        let parent = self.nodes[&obj].parents[0].0;
        if self.nodes[&parent].space != space {
            return obj;
        }
        self.find_root_of_space(parent)
    }

    fn next_space(&mut self) -> Space {
        self.next_space = Space(self.next_space.0 + 1);
        self.next_space
    }

    fn duplicate_subgraph(
        &mut self,
        root: ObjectId,
        dupes: &mut HashMap<ObjectId, ObjectId>,
        space: Space,
    ) -> ObjectId {
        if let Some(existing) = dupes.get(&root) {
            return *existing;
        }
        self.parents_invalid = true;
        self.distance_invalid = true;
        let new_root = ObjectId::next();
        log::debug!("duplicating node {root:?} to {new_root:?}");

        let mut obj = self.objects.get(&root).cloned().unwrap();
        let mut node = Node::new(obj.bytes.len() as u32);
        node.space = space;

        for link in &mut obj.offsets {
            // recursively duplicate the object
            link.object = self.duplicate_subgraph(link.object, dupes, space);
        }
        dupes.insert(root, new_root);
        self.objects.insert(new_root, obj);
        self.nodes.insert(new_root, node);
        new_root
    }

    /// Find the set of nodes that are reachable from root only following
    /// 16 & 24 bit offsets, and assign them to space 0.
    fn assign_space_0(&mut self) {
        let mut stack = VecDeque::from([self.root]);

        while let Some(next) = stack.pop_front() {
            match self.nodes.get_mut(&next) {
                Some(node) if node.space != Space::SHORT_REACHABLE => {
                    node.space = Space::SHORT_REACHABLE
                }
                _ => continue,
            }
            for link in self
                .objects
                .get(&next)
                .iter()
                .flat_map(|obj| obj.offsets.iter())
            {
                if link.len != OffsetLen::Offset32 {
                    stack.push_back(link.object);
                }
            }
        }
    }

    #[cfg(test)]
    fn find_descendents(&self, root: ObjectId) -> HashSet<ObjectId> {
        let mut result = HashSet::new();
        let mut stack = VecDeque::from([root]);
        while let Some(id) = stack.pop_front() {
            if result.insert(id) {
                for link in self
                    .objects
                    .get(&id)
                    .iter()
                    .flat_map(|obj| obj.offsets.iter())
                {
                    stack.push_back(link.object);
                }
            }
        }
        result
    }

    #[cfg(feature = "dot2")]
    pub(crate) fn write_graph_viz(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        graphviz::GraphVizGraph::from_graph(self).write_to_file(path)
    }
}

impl Default for Priority {
    fn default() -> Self {
        Priority::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ids<const N: usize>() -> [ObjectId; N] {
        let mut ids = [ObjectId::next(); N];
        for id in ids.iter_mut().skip(1) {
            *id = ObjectId::next();
        }
        ids
    }

    struct Link {
        from: ObjectId,
        to: ObjectId,
        width: OffsetLen,
    }

    struct TestGraphBuilder {
        objects: Vec<(ObjectId, usize)>,
        links: Vec<Link>,
    }

    impl TestGraphBuilder {
        fn new<const N: usize>(ids: [ObjectId; N], sizes: [usize; N]) -> Self {
            TestGraphBuilder {
                objects: ids.into_iter().zip(sizes).collect(),
                links: Default::default(),
            }
        }

        fn add_link(&mut self, from: ObjectId, to: ObjectId, width: OffsetLen) -> &mut Self {
            self.links.push(Link { from, to, width });
            self
        }

        fn build(&self) -> Graph {
            let mut objects = self
                .objects
                .iter()
                .map(|(id, size)| {
                    let table = TableData::make_mock(*size);
                    (*id, table)
                })
                .collect::<HashMap<_, _>>();

            for link in &self.links {
                objects
                    .get_mut(&link.from)
                    .unwrap()
                    .add_mock_offset(link.to, link.width);
            }
            let root = self.objects.first().unwrap().0;
            Graph::from_objects(objects, root)
        }
    }

    //#[test]
    //fn difference_smoke_test() {
    //assert!(Distance::MIN < Distance::MAX);
    //assert!(
    //Distance::from_offset_and_size(OffsetLen::Offset16, 10)
    //< Distance::from_offset_and_size(OffsetLen::Offset16, 20)
    //);
    //assert!(
    //Distance::from_offset_and_size(OffsetLen::Offset32, 10)
    //> Distance::from_offset_and_size(OffsetLen::Offset16, 20)
    //);
    //assert!(Distance::new(10, 3) > Distance::new(10, 1));
    //}

    #[test]
    fn priority_smoke_test() {
        let mut node = Node::new(20);
        node.distance = 100;
        let mod0 = node.modified_distance(1);
        node.raise_priority();
        let mod1 = node.modified_distance(1);
        assert!(mod0 > mod1);
        node.raise_priority();
        let mod2 = node.modified_distance(1);
        assert!(mod1 > mod2);
        node.raise_priority();
        let mod3 = node.modified_distance(1);
        assert!(mod2 > mod3, "{mod2:?} {mod3:?}");

        // max priority is 3
        node.raise_priority();
        let mod4 = node.modified_distance(1);
        assert_eq!(mod3, mod4);
    }

    #[test]
    fn kahn_basic() {
        let ids = make_ids::<4>();
        let sizes = [10, 10, 20, 10];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset16)
            .add_link(ids[0], ids[2], OffsetLen::Offset16)
            .add_link(ids[0], ids[3], OffsetLen::Offset16)
            .add_link(ids[3], ids[1], OffsetLen::Offset16)
            .build();

        graph.sort_kahn();
        // 3 links 1, so 1 must be last
        assert_eq!(&graph.order, &[ids[0], ids[2], ids[3], ids[1]]);
    }

    #[test]
    fn shortest_basic() {
        let ids = make_ids::<4>();
        let sizes = [10, 10, 20, 10];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset16)
            .add_link(ids[0], ids[2], OffsetLen::Offset16)
            .add_link(ids[0], ids[3], OffsetLen::Offset16)
            .build();

        graph.sort_shortest_distance();
        // but 2 is larger than 3, so should be ordered after
        assert_eq!(&graph.order, &[ids[0], ids[1], ids[3], ids[2]]);
    }

    #[test]
    fn overflow_basic() {
        let ids = make_ids::<3>();
        let sizes = [10, u16::MAX as usize - 5, 100];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset16)
            .add_link(ids[0], ids[2], OffsetLen::Offset16)
            .add_link(ids[1], ids[2], OffsetLen::Offset16)
            .build();
        graph.sort_kahn();
        assert_eq!(graph.find_overflows().len(), 1);
        assert_eq!(graph.find_overflows()[0].parent, ids[0]);
        assert_eq!(graph.find_overflows()[0].child, ids[2]);
    }

    #[test]
    fn duplicate_subgraph() {
        let _ = env_logger::builder().is_test(true).try_init();
        let ids = make_ids::<10>();
        let sizes = [10; 10];

        // root has two children, one 16 and one 32-bit offset.
        // those subgraphs share three nodes, which must be deduped.

        //
        //     before          after
        //      0                 0
        //     / ⑊            ┌───┘⑊
        //    1   2 ---+      1     2 ---+
        //    |\ / \   |     / \   / \   |
        //    | 3   4  5    9   3 3'  4  5
        //    |  \ / \          |  \ / \
        //    |   6   7         6   6'  7
        //    |       |                 |
        //    |    8──┘              8──┘
        //    |    │                /
        //    9 ───┘               9'

        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset16)
            .add_link(ids[0], ids[2], OffsetLen::Offset32)
            .add_link(ids[1], ids[3], OffsetLen::Offset16)
            .add_link(ids[1], ids[9], OffsetLen::Offset16)
            .add_link(ids[2], ids[3], OffsetLen::Offset16)
            .add_link(ids[2], ids[4], OffsetLen::Offset16)
            .add_link(ids[2], ids[5], OffsetLen::Offset16)
            .add_link(ids[3], ids[6], OffsetLen::Offset16)
            .add_link(ids[4], ids[6], OffsetLen::Offset16)
            .add_link(ids[4], ids[7], OffsetLen::Offset16)
            .add_link(ids[7], ids[8], OffsetLen::Offset16)
            .add_link(ids[8], ids[9], OffsetLen::Offset16)
            .build();

        assert_eq!(graph.nodes.len(), 10);
        let one = graph.find_descendents(ids[1]);
        let two = graph.find_descendents(ids[2]);
        assert_eq!(one.intersection(&two).count(), 3);

        graph.assign_spaces_hb();

        // 3, 6, and 9 should be duplicated
        assert_eq!(graph.nodes.len(), 13);
        let one = graph.find_descendents(ids[1]);
        let two = graph.find_descendents(ids[2]);
        assert_eq!(one.intersection(&two).count(), 0);

        for id in &one {
            assert!(!graph.nodes.get(id).unwrap().space.is_custom());
        }

        for id in &two {
            assert!(graph.nodes.get(id).unwrap().space.is_custom());
        }
    }

    #[test]
    fn split_overflowing_spaces() {
        // this attempts to show a simplified version of a gsub table with extension
        // subtables, before any isolation/deduplication has happened.
        //
        //    before                         after
        //      0           (GSUB)             0
        //      |                              |
        //      1        (lookup List)         1
        //      |                              |
        //      2          (Lookup)            2
        //     / \                            / \
        //  ╔═3   4═╗   (ext subtables)    ╔═3   4═╗
        //  ║       ║                      ║       ║   (long offsets)
        //  5─┐   ┌─6    (subtables)       5       6
        //  │ └─8─┘ │                     / \     / \
        //  │       │    (cov tables)    7'  8'  7   8
        //  └───7───┘
        //

        let _ = env_logger::builder().is_test(true).try_init();
        let ids = make_ids::<9>();
        // make the coverage tables big enough that overflow is unavoidable
        let sizes = [10, 4, 12, 8, 8, 14, 14, 65520, 65520];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset16)
            .add_link(ids[1], ids[2], OffsetLen::Offset16)
            .add_link(ids[2], ids[3], OffsetLen::Offset16)
            .add_link(ids[2], ids[4], OffsetLen::Offset16)
            .add_link(ids[3], ids[5], OffsetLen::Offset32)
            .add_link(ids[4], ids[6], OffsetLen::Offset32)
            .add_link(ids[5], ids[7], OffsetLen::Offset16)
            .add_link(ids[5], ids[8], OffsetLen::Offset16)
            .add_link(ids[6], ids[7], OffsetLen::Offset16)
            .add_link(ids[6], ids[8], OffsetLen::Offset16)
            .build();
        graph.sort_shortest_distance();

        assert!(graph.has_overflows());
        assert_eq!(graph.nodes.len(), 9);

        graph.assign_spaces_hb();
        graph.sort_shortest_distance();

        // now spaces are assigned, but not isolated
        assert_eq!(graph.nodes[&ids[5]].space, graph.nodes[&ids[6]].space);
        assert_eq!(graph.nodes.len(), 9);

        // now isolate space that overflows
        let overflows = graph.find_overflows();
        graph.try_splitting_subgraphs(&overflows);
        graph.sort_shortest_distance();

        assert_eq!(graph.nodes.len(), 11);
        assert!(graph.find_overflows().is_empty());
        // ensure we are correctly update the roots_per_space thing
        assert_eq!(graph.num_roots_per_space[&graph.nodes[&ids[6]].space], 1);
        assert_eq!(graph.num_roots_per_space[&graph.nodes[&ids[5]].space], 1);
    }

    #[test]
    fn two_roots_one_space() {
        // If a subgraph is reachable from multiple long offsets, they are all
        // initially placed in the same space.
        //
        //  ┌──0═══╗    ┌──0═══╗
        //  │  ║   ║    │  ║   ║
        //  │  ║   ║    │  ║   ║
        //  1  2   3    1  2   3
        //  │   \ /     │   \ /
        //  └────4      4    4'
        //       │      │    │
        //       5      5    5'

        let ids = make_ids::<6>();
        let sizes = [10; 6];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset16)
            .add_link(ids[0], ids[2], OffsetLen::Offset32)
            .add_link(ids[0], ids[3], OffsetLen::Offset32)
            .add_link(ids[1], ids[4], OffsetLen::Offset16)
            .add_link(ids[2], ids[4], OffsetLen::Offset16)
            .add_link(ids[3], ids[4], OffsetLen::Offset16)
            .add_link(ids[4], ids[5], OffsetLen::Offset16)
            .build();

        assert_eq!(graph.nodes.len(), 6);
        graph.assign_spaces_hb();
        assert_eq!(graph.nodes.len(), 8);
        let one = graph.find_descendents(ids[1]);
        assert!(one.iter().all(|id| !graph.nodes[id].space.is_custom()));

        let two = graph.find_descendents(ids[2]);
        let three = graph.find_descendents(ids[3]);
        assert_eq!(two.intersection(&three).count(), 2);
        assert_eq!(two.union(&three).count(), 4);

        assert_eq!(
            two.union(&three)
                .map(|id| graph.nodes[id].space)
                .collect::<HashSet<_>>()
                .len(),
            1
        );
    }

    #[test]
    fn duplicate_shared_root_subgraph() {
        // if a node is linked from both 16 & 32-bit space, and has no parents
        // in 32 bit space, it should always still be deduped.
        //
        //    before    after
        //     0          0
        //    / ⑊        / ⑊
        //   1   ⑊      1   2
        //   └───╴2     │
        //              2'

        let ids = make_ids::<3>();
        let sizes = [10; 3];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset16)
            .add_link(ids[0], ids[2], OffsetLen::Offset32)
            .add_link(ids[1], ids[2], OffsetLen::Offset16)
            .build();
        graph.assign_spaces_hb();
        assert_eq!(graph.nodes.len(), 4);
    }

    #[test]
    fn assign_space_even_without_any_duplication() {
        // the subgraph of the long offset (0->2) is already isolated, and
        // so requires no duplication; but we should still correctly assign a
        // space to the children.
        //
        //     0
        //    / ⑊
        //   1   2
        //      /
        //     3

        let ids = make_ids::<4>();
        let sizes = [10; 4];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset16)
            .add_link(ids[0], ids[2], OffsetLen::Offset32)
            .add_link(ids[2], ids[3], OffsetLen::Offset16)
            .build();
        graph.assign_spaces_hb();
        let two = graph.find_descendents(ids[2]);
        assert!(two.iter().all(|id| graph.nodes[id].space.is_custom()));
    }

    #[test]
    fn sort_respects_spaces() {
        let ids = make_ids::<4>();
        let sizes = [10; 4];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset32)
            .add_link(ids[0], ids[2], OffsetLen::Offset32)
            .add_link(ids[0], ids[3], OffsetLen::Offset16)
            .build();
        graph.sort_shortest_distance();
        assert_eq!(&graph.order, &[ids[0], ids[3], ids[1], ids[2]]);
    }

    #[test]
    fn assign_32bit_spaces_if_needed() {
        let ids = make_ids::<3>();
        let sizes = [10, u16::MAX as usize, 10];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset32)
            .add_link(ids[0], ids[2], OffsetLen::Offset16)
            .add_link(ids[1], ids[2], OffsetLen::Offset16)
            .build();
        graph.basic_sort();
        // this will overflow unless the 32-bit offset is put last.
        assert!(graph.has_overflows());
        graph.pack_objects();
        assert!(!graph.has_overflows());
    }
}
