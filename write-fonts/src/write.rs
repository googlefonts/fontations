use std::collections::{BTreeSet, HashMap};

use crate::error::{Error, PackingError};
use crate::graph::{Graph, ObjectId, ObjectStore, OffsetLen};
use crate::validate::Validate;
use font_types::Scalar;
use types::Uint24;

/// A type that that can be written out as part of a font file.
///
/// This both handles writing big-endian bytes as well as describing the
/// relationship between tables and their subtables.
pub trait FontWrite {
    /// Write our data and information about offsets into this [TableWriter].
    fn write_into(&self, writer: &mut TableWriter);
    fn name(&self) -> &'static str {
        "Unknown"
    }
}

/// An object that manages a collection of serialized tables.
///
/// This handles deduplicating objects and tracking offsets.
#[derive(Debug)]
pub struct TableWriter {
    /// Finished tables, associated with an ObjectId; duplicate tables share an id.
    tables: ObjectStore,
    /// Tables currently being written.
    ///
    /// Tables are processed as they are encountered (as subtables)
    stack: Vec<TableData>,
    /// An adjustment factor subtracted from written offsets.
    ///
    /// This is '0', unless a particular offset is expected to be relative some
    /// position *other* than the start of the table.
    ///
    /// This should only ever be non-zero in the body of a closure passed to
    /// [adjust_offsets](Self::adjust_offsets)
    offset_adjustment: u32,
}

/// Attempt to serialize a table.
///
/// Returns an error if the table is malformed or cannot otherwise be serialized,
/// otherwise it will return the bytes encoding the table.
pub fn dump_table<T: FontWrite + Validate>(table: &T) -> Result<Vec<u8>, Error> {
    table.validate().map_err(Error::ValidationFailed)?;
    let mut graph = TableWriter::make_graph(table);

    if !graph.pack_objects() {
        return Err(Error::PackingFailed(PackingError {
            graph: graph.into(),
        }));
    }
    Ok(dump_impl(&graph.order, &graph.objects))
}

fn dump_impl(order: &[ObjectId], nodes: &HashMap<ObjectId, TableData>) -> Vec<u8> {
    let mut offsets = HashMap::new();
    let mut out = Vec::new();
    let mut off = 0;

    // first pass: write out bytes, record positions of offsets
    for id in order {
        let node = nodes.get(id).unwrap();
        offsets.insert(*id, off);
        off += node.bytes.len() as u32;
        out.extend_from_slice(&node.bytes);
    }

    // second pass: write offsets
    let mut table_head = 0;
    for id in order {
        let node = nodes.get(id).unwrap();
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

impl TableWriter {
    /// A convenience method for generating a graph with the provided root object.
    fn make_graph(root: &impl FontWrite) -> Graph {
        let mut writer = TableWriter::default();
        let root_id = writer.add_table(root);
        Graph::from_obj_store(writer.tables, root_id)
    }

    fn add_table(&mut self, table: &dyn FontWrite) -> ObjectId {
        self.stack.push(TableData::default());
        table.write_into(self);
        let mut table_data = self.stack.pop().unwrap();
        table_data.name = table.name();
        self.tables.add(table_data)
    }

    /// Call the provided closure, adjusting any written offsets by `adjustment`.
    pub(crate) fn adjust_offsets(&mut self, adjustment: u32, f: impl FnOnce(&mut TableWriter)) {
        self.offset_adjustment = adjustment;
        f(self);
        self.offset_adjustment = 0;
    }

    /// Write raw bytes into this table.
    ///
    /// The caller is responsible for ensuring bytes are in big-endian order.
    #[inline]
    pub fn write_slice(&mut self, bytes: &[u8]) {
        self.stack.last_mut().unwrap().write_bytes(bytes)
    }

    /// Create an offset to another table.
    ///
    /// The `width` argument is the size in bytes of the offset, e.g. 2 for
    /// an `Offset16`, and 4 for an `Offset32`.
    ///
    /// The provided table will be serialized immediately, and the position
    /// of the offset within the current table will be recorded. Offsets
    /// are resolved when the root table object is serialized, at which point
    /// we overwrite each recorded offset position with the final offset of the
    /// appropriate table.
    pub fn write_offset(&mut self, obj: &dyn FontWrite, width: usize) {
        let obj_id = self.add_table(obj);
        let data = self.stack.last_mut().unwrap();
        data.add_offset(obj_id, width, self.offset_adjustment);
    }

    /// Add a padding byte of necessary to ensure the table length is an even number.
    ///
    /// This is necessary for things like the glyph table, which require offsets
    /// to be aligned on 2-byte boundaries.
    pub fn pad_to_2byte_aligned(&mut self) {
        if self.stack.last().unwrap().bytes.len() % 2 != 0 {
            self.write_slice(&[0]);
        }
    }

    /// used when writing top-level font objects, which are done more manually.
    pub(crate) fn into_data(mut self) -> Vec<u8> {
        assert_eq!(self.stack.len(), 1);
        let result = self.stack.pop().unwrap();
        assert!(result.offsets.is_empty());
        result.bytes
    }
}

impl Default for TableWriter {
    fn default() -> Self {
        TableWriter {
            tables: ObjectStore::default(),
            stack: vec![TableData::default()],
            offset_adjustment: 0,
        }
    }
}

/// The encoded data for a given table, along with info on included offsets
#[derive(Debug, Default, Clone)] // DO NOT DERIVE MORE TRAITS! we want to ignore name field
pub(crate) struct TableData {
    pub(crate) name: &'static str,
    pub(crate) bytes: Vec<u8>,
    pub(crate) offsets: Vec<OffsetRecord>,
}

impl std::hash::Hash for TableData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
        self.offsets.hash(state);
    }
}

impl PartialEq for TableData {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes && self.offsets == other.offsets
    }
}

impl Eq for TableData {}

/// The position and type of an offset, along with the id of the pointed-to entity
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct OffsetRecord {
    /// the position of the offset within the parent table
    pos: u32,
    /// the offset length in bytes
    pub(crate) len: OffsetLen,
    /// The object pointed to by the offset
    pub(crate) object: ObjectId,
    /// a value subtracted from the resolved offset before writing.
    ///
    /// In general we assume that offsets are relative to the start of the parent
    /// table, but in some cases this is not true (for instance, offsets to
    /// strings in the name table are relative to the end of the table.)
    pub(crate) adjustment: u32,
}

impl TableData {
    /// the 'adjustment' param is used to modify the written position.
    fn add_offset(&mut self, object: ObjectId, width: usize, adjustment: u32) {
        self.offsets.push(OffsetRecord {
            pos: self.bytes.len() as u32,
            len: match width {
                2 => OffsetLen::Offset16,
                3 => OffsetLen::Offset24,
                _ => OffsetLen::Offset32,
            },
            object,
            adjustment,
        });
        let null_bytes = &[0u8, 0, 0, 0].get(..width.min(4)).unwrap();

        self.write_bytes(null_bytes);
    }

    #[allow(dead_code)] // this will be used when we do promotion/splitting
    pub(crate) fn write<T: Scalar>(&mut self, value: T) {
        self.write_bytes(value.to_raw().as_ref())
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes)
    }

    #[cfg(test)]
    pub fn make_mock(size: usize) -> Self {
        TableData {
            name: "",
            bytes: vec![0xca; size], // has no special meaning
            offsets: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn add_mock_offset(&mut self, object: ObjectId, len: OffsetLen) {
        let pos = self.offsets.iter().map(|off| off.len as u8 as u32).sum();
        self.offsets.push(OffsetRecord {
            pos,
            len,
            object,
            adjustment: 0,
        });
    }
}

macro_rules! write_be_bytes {
    ($ty:ty) => {
        impl FontWrite for $ty {
            #[inline]
            fn write_into(&self, writer: &mut TableWriter) {
                writer.write_slice(&self.to_be_bytes())
            }
        }
    };
}

//NOTE: not implemented for offsets! it would be too easy to accidentally write them.
write_be_bytes!(u8);
write_be_bytes!(i8);
write_be_bytes!(u16);
write_be_bytes!(i16);
write_be_bytes!(u32);
write_be_bytes!(i32);
write_be_bytes!(i64);
write_be_bytes!(types::Uint24);
write_be_bytes!(types::F2Dot14);
write_be_bytes!(types::Fixed);
write_be_bytes!(types::FWord);
write_be_bytes!(types::UfWord);
write_be_bytes!(types::LongDateTime);
write_be_bytes!(types::Tag);
write_be_bytes!(types::Version16Dot16);
write_be_bytes!(types::MajorMinor);
write_be_bytes!(types::GlyphId);
write_be_bytes!(types::NameId);

impl<T: FontWrite> FontWrite for [T] {
    fn write_into(&self, writer: &mut TableWriter) {
        self.iter().for_each(|item| item.write_into(writer))
    }
}

impl<T: FontWrite> FontWrite for BTreeSet<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        self.iter().for_each(|item| item.write_into(writer))
    }
}

impl<T: FontWrite> FontWrite for Vec<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        self.iter().for_each(|item| item.write_into(writer))
    }
}

impl<T: FontWrite> FontWrite for Option<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        match self {
            Some(obj) => obj.write_into(writer),
            None => (),
        }
    }
}
