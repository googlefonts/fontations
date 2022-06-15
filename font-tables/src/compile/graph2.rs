use std::collections::{BinaryHeap, HashMap, HashSet};

use font_types::OffsetLen;

use super::{
    graph::{ObjectId, ObjectStore},
    TableData,
};

pub struct Graph {
    pub(crate) objects: HashMap<ObjectId, TableData>,
    nodes: HashMap<ObjectId, Node>,
    pub(crate) order: Vec<ObjectId>,
    root: ObjectId,
    parents_invalid: bool,
    distance_invalid: bool,
    positions_invalid: bool,
    successful: bool,
    num_roots_for_space: Vec<usize>,
}

struct Node {
    size: u32,
    distance: Distance,
    space: i64,
    parents: Vec<ObjectId>,
    priority: Priority,
}

/// Scored used when computing shortest distance
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Distance {
    distance: u64,
    order: u32,
}

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
    const MIN: Distance = Distance::new(u64::MIN, u32::MIN);
    const MAX: Distance = Distance::new(u64::MAX, u32::MAX);

    const fn new(distance: u64, order: u32) -> Self {
        Distance { distance, order }
    }

    fn from_offset_and_size(width: OffsetLen, size: u32) -> Self {
        let width_bits = width as u8 * 8;
        let distance = size as u64 + 1_u64 << width_bits;
        Distance { distance, order: 0 }.into()
    }

    fn rev(self) -> std::cmp::Reverse<Distance> {
        std::cmp::Reverse(self)
    }
}

impl Node {
    pub fn new(size: u32) -> Self {
        Node {
            //obj,
            size,
            distance: Default::default(),
            space: 0,
            parents: Default::default(),
            priority: Default::default(),
        }
    }

    fn is_shared(&self) -> bool {
        self.parents.len() > 1
    }

    fn incoming_edges(&self) -> usize {
        self.parents.len()
    }

    fn remove_parent(&mut self, obj: ObjectId) {
        if let Some(idx) = self.parents.iter().position(|x| x == &obj) {
            self.parents.swap_remove(idx);
        }
    }

    fn raise_priority(&mut self) -> bool {
        self.priority.increase()
    }

    fn modified_distance(&self, order: u32) -> Distance {
        let dist = self.distance.distance as i64;
        let modified_distance = match self.priority {
            Priority::ZERO => dist,
            Priority::ONE => dist - self.size as i64 / 2,
            Priority::TWO => dist - self.size as i64,
            _ => 0,
        }
        .max(0);
        Distance::new(modified_distance as u64, order)
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
            successful: true,
            num_roots_for_space: vec![1],
        }
    }

    fn topological_sort(&mut self) {}

    fn update_parents(&mut self) {
        if !self.parents_invalid {
            return;
        }
        for (_, node) in &mut self.nodes {
            node.parents.clear();
        }

        for (id, obj) in &self.objects {
            for child in &obj.offsets {
                self.nodes.get_mut(&child.object).unwrap().parents.push(*id);
            }
        }
        self.parents_invalid = false;
    }

    pub(crate) fn sort_kahn(&mut self) {
        self.positions_invalid = true;
        if self.nodes.len() <= 1 {
            return;
        }

        let mut queue = BinaryHeap::new();
        let mut removed_edges = HashMap::new();
        self.order.clear();

        self.update_parents();
        queue.push(std::cmp::Reverse(self.root));

        while let Some(id) = queue.pop().map(|x| x.0) {
            let next = &self.objects[&id];
            self.order.push(id);
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
            if *seen_len != self.nodes[&id].parents.len() {
                panic!("cycle or something?");
            }
        }
    }

    pub(crate) fn sort_shortest_distance(&mut self) {
        self.positions_invalid = true;
        self.update_parents();
        self.update_distances();

        let mut queue = BinaryHeap::new();
        let mut removed_edges = HashMap::with_capacity(self.nodes.len());
        self.order.clear();

        queue.push((Distance::MIN.rev(), self.root));
        let mut obj_order = 1u32;
        while let Some((_, id)) = queue.pop() {
            let next = &self.objects[&id];
            self.order.push(id);
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
            if *seen_len != self.nodes[&id].parents.len() {
                panic!("cycle or something?");
            }
        }
    }

    fn update_distances(&mut self) {
        for (id, node) in &mut self.nodes {
            if *id == self.root {
                node.distance = Distance::MIN;
            } else {
                node.distance = Distance::MAX;
            }
        }

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
                let distance = Distance::from_offset_and_size(link.len, child.size);
                let child_distance = next_distance + distance;

                if child_distance < child.distance {
                    child.distance = child_distance;
                    queue.push((child_distance, link.object));
                }
            }
        }

        self.distance_invalid = false;
    }
}

impl Default for Priority {
    fn default() -> Self {
        Priority::ZERO
    }
}

impl std::ops::Add for Distance {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let distance = self.distance + rhs.distance;
        Distance::new(distance, self.order)
    }
}

impl std::ops::AddAssign for Distance {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
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

    #[test]
    fn difference_smoke_test() {
        assert!(Distance::MIN < Distance::MAX);
        assert!(
            Distance::from_offset_and_size(OffsetLen::Offset16, 10)
                < Distance::from_offset_and_size(OffsetLen::Offset16, 20)
        );
        assert!(
            Distance::from_offset_and_size(OffsetLen::Offset32, 10)
                > Distance::from_offset_and_size(OffsetLen::Offset16, 20)
        );
        assert!(Distance::new(10, 3) > Distance::new(10, 1));
    }

    #[test]
    fn priority_smoke_test() {
        let mut node = Node::new(20);
        node.distance = Distance::new(100, 0);
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
}
