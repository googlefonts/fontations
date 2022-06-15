//! A graph for resolving table offsets

use super::TableData;
use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::AtomicUsize,
};

static OBJECT_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialOrd, Ord, Hash, PartialEq, Eq)]
pub(crate) struct ObjectId(usize);

impl ObjectId {
    pub fn next() -> Self {
        ObjectId(OBJECT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

#[derive(Debug, Default)]
pub(crate) struct ObjectStore {
    pub(crate) objects: HashMap<TableData, ObjectId>,
}

pub(crate) struct Graph {
    pub(crate) nodes: HashMap<ObjectId, TableData>,
}

impl Graph {
    fn make_parents(&self) -> HashMap<ObjectId, Vec<ObjectId>> {
        let mut edges = HashMap::<_, Vec<_>>::new();
        for (id, node) in &self.nodes {
            for offset in &node.offsets {
                edges.entry(offset.object).or_default().push(*id);
            }
        }
        edges
    }

    pub(super) fn kahn_sort(&self, root: ObjectId) -> Vec<ObjectId> {
        let mut parents = self.make_parents();
        let mut queue = VecDeque::new();

        queue.push_back(root);
        let mut sorted = Vec::new();

        while let Some(id) = queue.pop_front() {
            sorted.push(id);
            let node = &self.nodes[&id];
            for offset in &node.offsets {
                let parent_links = parents.get_mut(&offset.object).unwrap();
                let idx = parent_links.iter().position(|p| p == &id).unwrap();
                parent_links.remove(idx);
                if parent_links.is_empty() {
                    queue.push_back(offset.object);
                }
            }
        }

        if parents.values().any(|val| !val.is_empty()) {
            panic!("cycle?")
        }

        sorted
    }
}

impl ObjectStore {
    pub(crate) fn add(&mut self, data: TableData) -> ObjectId {
        *self.objects.entry(data).or_insert_with(ObjectId::next)
    }

    pub(crate) fn into_graph(self) -> Graph {
        let nodes = self.objects.into_iter().map(|(k, v)| (v, k)).collect();
        Graph { nodes }
    }
}
