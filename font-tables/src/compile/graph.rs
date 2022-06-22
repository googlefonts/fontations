//! A graph for resolving table offsets

use font_types::OffsetLen;

use super::TableData;
use std::{
    collections::{BinaryHeap, HashMap, HashSet, VecDeque},
    sync::atomic::AtomicUsize,
};

static OBJECT_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialOrd, Ord, Hash, PartialEq, Eq)]
pub(crate) struct ObjectId(usize);

/// A ranking used for sorting the graph.
///
/// Nodes are assigned a space, and nodes in lower spaces are always
/// packed before nodes in higher spaces.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, Hash, PartialEq, Eq)]
struct Space(u32);

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

pub struct Graph {
    pub(crate) objects: HashMap<ObjectId, TableData>,
    nodes: HashMap<ObjectId, Node>,
    pub(crate) order: Vec<ObjectId>,
    root: ObjectId,
    parents_invalid: bool,
    distance_invalid: bool,
    positions_invalid: bool,
    next_space: Space,
}

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

impl Priority {
    const ZERO: Priority = Priority(0);
    const ONE: Priority = Priority(1);
    const TWO: Priority = Priority(2);
    const THREE: Priority = Priority(3);

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
        }
    }

    pub(crate) fn topological_sort(&mut self) {
        self.sort_kahn();
        if !self.find_overflows().is_empty() {
            self.sort_shortest_distance();
        }
        if !self.find_overflows().is_empty() {
            self.assign_32bit_spaces();
            self.sort_shortest_distance();
        }
    }

    fn find_overflows(&self) -> Vec<(ObjectId, ObjectId)> {
        let mut result = Vec::new();
        for (parent_id, data) in &self.objects {
            let parent = &self.nodes[&parent_id];
            for link in &data.offsets {
                let child = &self.nodes[&link.object];
                //TODO: account for 'whence'
                let rel_off = child.position - parent.position;
                if link.len.max_value() < rel_off {
                    result.push((*parent_id, link.object));
                }
            }
        }
        result
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

    /// Returns `true` if there were any 32bit subgraphs
    fn assign_32bit_spaces(&mut self) -> bool {
        self.update_parents();
        // find all the nodes that only have 32-bit incoming edges
        let mut roots = HashSet::new();
        for (id, node) in &self.nodes {
            if !node.parents.is_empty()
                && node
                    .parents
                    .iter()
                    .all(|(_, len)| *len == OffsetLen::Offset32)
            {
                roots.insert(*id);
            }
        }

        if roots.is_empty() {
            return false;
        }

        // assign all nodes reachable from 16/24 bit edges to space 0.
        self.assign_space_0();

        while !roots.is_empty() {
            let root = *roots.iter().next().unwrap();
            self.isolate_and_assign_space(root);
            roots.remove(&root);
        }
        self.update_parents();
        true
    }

    /// Isolate the subgraph at root, deduplicating any nodes reachable from
    /// 16-bit space. Assign the subgraph to a new space.
    fn isolate_and_assign_space(&mut self, root: ObjectId) {
        // - if root is already in a space, it means we're part of an existing
        // subgraph, and can return.
        //
        // - do a directed traversal from root
        // - if we encounter a node in space 0, duplicate that node (subgraph?)
        // - if we encounter a node in *another* space:
        //    - we want it ordered after us, somehow :thinking face:
        //    - maybe we reassign all nodes in that space to space_next()?
        if self.nodes.get(&root).unwrap().space.is_custom() {
            return;
        }

        let mut stack = VecDeque::from([root]);
        let space = self.next_space();

        enum Op {
            Reprioritize(Space),
            Duplicate(ObjectId),
            None,
        }

        let mut duplicated = HashMap::new();

        while let Some(next) = stack.pop_front() {
            // we do this with an enum so we can release the borrow
            let op = match self.nodes.get_mut(&next) {
                Some(node) => match node.space {
                    // if this node is already in a space, we want to force that
                    // space to be after the current one.
                    Space::SHORT_REACHABLE => Op::Duplicate(next),
                    Space::REACHABLE => Op::None,
                    prev_space if prev_space == space => continue,
                    prev_space => Op::Reprioritize(prev_space),
                },
                None => unreachable!("ahem"),
            };

            let next = match op {
                Op::Reprioritize(old_space) => {
                    self.reprioritize_space(old_space);
                    // no need to recurse
                    continue;
                }
                Op::Duplicate(obj) => match duplicated.get(&obj) {
                    // if we've already duplicated this node we can continue
                    Some(_id) => continue,
                    None => {
                        let new_obj = self.duplicate_subgraph(obj, &mut duplicated);
                        duplicated.insert(obj, new_obj);
                        new_obj
                    }
                },
                Op::None => next,
            };

            self.nodes.get_mut(&next).unwrap().space = space;
            for link in self
                .objects
                .get(&next)
                .iter()
                .flat_map(|obj| obj.offsets.iter())
            {
                stack.push_back(link.object);
            }
        }

        // if we did any duplicates, do another traversal to update links
        if !duplicated.is_empty() {
            stack.push_back(root);
            while let Some(next) = stack.pop_front() {
                for link in self
                    .objects
                    .get_mut(&next)
                    .iter_mut()
                    .flat_map(|obj| obj.offsets.iter_mut())
                {
                    if let Some(new_id) = duplicated.get(&link.object) {
                        link.object = *new_id;
                    } else {
                        stack.push_back(link.object);
                    }
                }
            }
        }
    }

    fn next_space(&mut self) -> Space {
        self.next_space = Space(self.next_space.0 + 1);
        self.next_space
    }

    /// moves all nodes in the 'old' space to the next space.
    fn reprioritize_space(&mut self, old: Space) {
        let space = self.next_space();
        for node in self.nodes.values_mut() {
            if node.space == old {
                node.space = space;
            }
        }
    }

    fn duplicate_subgraph(
        &mut self,
        root: ObjectId,
        dupes: &mut HashMap<ObjectId, ObjectId>,
    ) -> ObjectId {
        if let Some(existing) = dupes.get(&root) {
            return *existing;
        }
        self.parents_invalid = true;
        self.distance_invalid = true;
        let new_root = ObjectId::next();
        let mut obj = self.objects.get(&root).cloned().unwrap();
        let node = Node::new(obj.bytes.len() as u32);

        for link in &mut obj.offsets {
            // recursively duplicate the object
            link.object = self.duplicate_subgraph(link.object, dupes);
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
        for i in 1..N {
            ids[i] = ObjectId::next();
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
        assert_eq!(graph.find_overflows(), &[(ids[0], ids[2])]);
    }

    #[test]
    fn duplicate_subgraph() {
        let ids = make_ids::<10>();
        let sizes = [10; 10];

        // root has two children, one 16 and one 32-bit offset.
        // those subgraphs share three nodes, which must be deduped.

        //
        //     before          after
        //      0                 0
        //     / \            ┌───┘\
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

        graph.sort_kahn();
        graph.assign_32bit_spaces();

        // 3, 6, and 9 should be duplicated
        assert_eq!(graph.nodes.len(), 13);
        let one = graph.find_descendents(ids[1]);
        let two = graph.find_descendents(ids[2]);
        assert_eq!(one.intersection(&two).count(), 0);

        for id in &one {
            assert_eq!(graph.nodes.get(&id).unwrap().space, Space::SHORT_REACHABLE);
        }

        for id in &two {
            assert!(graph.nodes.get(&id).unwrap().space.is_custom());
        }
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
        let ids = make_ids::<4>();
        let sizes = [10, u16::MAX as usize, 10, 10];
        let mut graph = TestGraphBuilder::new(ids, sizes)
            .add_link(ids[0], ids[1], OffsetLen::Offset32)
            .add_link(ids[0], ids[2], OffsetLen::Offset16)
            .add_link(ids[1], ids[2], OffsetLen::Offset16)
            .build();
        graph.topological_sort();
        assert!(graph.find_overflows().is_empty());
    }
}
