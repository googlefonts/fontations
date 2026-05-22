use std::{collections::HashMap, sync::atomic::AtomicU64};

use crate::write::TableData;

static OBJECT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// An identifier for an object in the compilation graph.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, Hash, PartialEq, Eq)]
pub struct ObjectId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum OffsetLen {
    Offset16 = 2,
    Offset24 = 3,
    Offset32 = 4,
}

impl OffsetLen {
    /// The maximum value for an offset of this length.
    #[cfg(feature = "tables")]
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
