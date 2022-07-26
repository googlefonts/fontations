//! compiling font tables

use std::collections::HashMap;

use font_types::{
    BigEndian, DynSizedArray, FontReadWithArgs, Offset as AnyOffset, OffsetLen, Scalar, Uint24,
};

//mod cmap;
mod font_builder;
mod graph;
mod offsets;

pub use font_builder::FontBuilder;
use graph::{ObjectId, ObjectStore};
pub use offsets::{NullableOffsetMarker, OffsetMarker};

use self::graph::Graph;

/// A type that that can be written out as part of a font file.
///
/// This both handles writing big-endian bytes as well as describing the
/// relationship between tables and their subtables.
pub trait FontWrite {
    /// Write our data and information about offsets into this [TableWriter].
    fn write_into(&self, writer: &mut TableWriter);
}

#[derive(Debug)]
pub struct TableWriter {
    /// Finished tables, associated with an ObjectId; duplicate tables share an id.
    tables: ObjectStore,
    /// Tables currently being written.
    ///
    /// Tables are processed as they are encountered (as subtables)
    stack: Vec<TableData>,
}

/// A trait for types that can fully resolve themselves.
///
/// This means that any offsets held in this type are resolved relative to the
/// start of the table itself (and not some parent table)
pub trait ToOwnedTable: ToOwnedObj {
    fn to_owned_table(&self) -> Option<Self::Owned> {
        self.to_owned_obj(&[])
    }
}

/// A trait for types that can resolve themselves when provided data to resolve offsets.
///
/// This is implemented for the majority of parse types. Those that are the base
/// for offset data the provided data and use their own.
pub trait ToOwnedObj {
    type Owned;
    fn to_owned_obj(&self, offset_data: &[u8]) -> Option<Self::Owned>;
}

pub fn dump_table<T: FontWrite>(table: &T) -> Vec<u8> {
    let mut writer = TableWriter::default();
    table.write_into(&mut writer);
    let mut graph = writer.finish();
    graph.topological_sort();
    dump_impl(&graph.order, &graph.objects)
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
    let mut off = 0;
    for id in order {
        let node = nodes.get(id).unwrap();
        for offset in &node.offsets {
            let abs_off = *offsets.get(&offset.object).unwrap();
            let rel_off = abs_off - off as u32;
            let buffer_pos = off + offset.pos as usize;
            let write_over = out.get_mut(buffer_pos..).unwrap();
            write_offset(write_over, offset.len, rel_off).unwrap();
        }
        off += node.bytes.len();
    }
    out
}

//TODO: some kind of error if an offset is OOB?
fn write_offset(at: &mut [u8], len: OffsetLen, resolved: u32) -> Result<(), ()> {
    let at = &mut at[..len as u8 as usize];
    match len {
        OffsetLen::Offset16 => at.copy_from_slice(
            u16::try_from(resolved)
                .map_err(|_| ())?
                .to_be_bytes()
                .as_slice(),
        ),
        OffsetLen::Offset24 => at.copy_from_slice(
            Uint24::checked_new(resolved)
                .ok_or(())?
                .to_be_bytes()
                .as_slice(),
        ),
        OffsetLen::Offset32 => at.copy_from_slice(resolved.to_be_bytes().as_slice()),
    }
    Ok(())
}

impl TableWriter {
    fn add_table(&mut self, table: &dyn FontWrite) -> ObjectId {
        self.stack.push(TableData::default());
        table.write_into(self);
        self.tables.add(self.stack.pop().unwrap())
    }

    /// Finish this table, returning the root Id and the object graph.
    fn finish(mut self) -> Graph {
        let id = self.tables.add(self.stack.pop().unwrap());
        Graph::from_obj_store(self.tables, id)
    }

    #[inline]
    pub fn write_slice(&mut self, bytes: &[u8]) {
        self.stack
            .last_mut()
            .unwrap()
            .bytes
            .extend_from_slice(bytes)
    }

    pub fn write_offset<T: AnyOffset>(&mut self, obj: &dyn FontWrite) {
        let obj_id = self.add_table(obj);
        let data = self.stack.last_mut().unwrap();
        data.add_offset::<T>(obj_id);
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
        }
    }
}

impl FontWrite for Box<dyn FontWrite> {
    fn write_into(&self, writer: &mut TableWriter) {
        (**self).write_into(writer)
    }
}

/// The encoded data for a given table, along with info on included offsets
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub(crate) struct TableData {
    bytes: Vec<u8>,
    offsets: Vec<OffsetRecord>,
}

/// The position and type of an offset, along with the id of the pointed-to entity
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct OffsetRecord {
    /// the position of the offset within the parent table
    pos: u32,
    /// the offset type (16/24/32 bit)
    len: OffsetLen,
    /// The object pointed to by the offset
    object: ObjectId,
}

impl TableData {
    fn add_offset<T: AnyOffset>(&mut self, object: ObjectId) {
        self.offsets.push(OffsetRecord {
            pos: self.bytes.len() as u32,
            len: T::SIZE,
            object,
        });

        self.write(T::SIZE.null_bytes());
    }

    fn write(&mut self, bytes: &[u8]) {
        self.bytes.extend(bytes)
    }

    #[cfg(test)]
    pub fn make_mock(size: usize) -> Self {
        TableData {
            bytes: vec![0xca; size], // has no special meaning
            offsets: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn add_mock_offset(&mut self, object: ObjectId, len: OffsetLen) {
        let pos = self.offsets.iter().map(|off| off.len as u8 as u32).sum();
        self.offsets.push(OffsetRecord { pos, len, object });
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
write_be_bytes!(font_types::Uint24);
write_be_bytes!(font_types::F2Dot14);
write_be_bytes!(font_types::Fixed);
write_be_bytes!(font_types::LongDateTime);
write_be_bytes!(font_types::Tag);
write_be_bytes!(font_types::Version16Dot16);
write_be_bytes!(font_types::MajorMinor);

impl<T: Scalar + FontWrite> FontWrite for BigEndian<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        writer.write_slice(self.be_bytes())
    }
}

//impl<T: FontWrite> FontWrite for &'_ [T] {
//fn write_into(&self, writer: &mut TableWriter) {
//self.iter().for_each(|item| item.write_into(writer))
//}
//}

impl<T: FontWrite> FontWrite for [T] {
    fn write_into(&self, writer: &mut TableWriter) {
        self.iter().for_each(|item| item.write_into(writer))
    }
}

impl<'a, Args, T> ToOwnedObj for DynSizedArray<'a, Args, T>
where
    Args: Copy + 'static,
    T: FontReadWithArgs<'a, Args> + ToOwnedObj + 'a,
{
    type Owned = Vec<T::Owned>;

    fn to_owned_obj(&self, offset_data: &[u8]) -> Option<Self::Owned> {
        self.iter()
            .map(|item| item.to_owned_obj(offset_data))
            .collect::<Option<_>>()
    }
}

// expensive fallback eq implementation
impl PartialEq for Box<dyn FontWrite> {
    fn eq(&self, other: &Self) -> bool {
        let mut writer = TableWriter::default();
        let one = writer.add_table(self);
        let two = writer.add_table(other);
        one == two
    }
}

impl std::fmt::Debug for Box<dyn FontWrite> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Box<dyn FontWrite>")
    }
}

#[cfg(test)]
#[rustfmt::skip::macros(assert_hex_eq)]
mod tests {
    use font_types::Offset16;

    use crate::assert_hex_eq;

    use super::*;

    struct Table1 {
        version: u16,
        records: Vec<SomeRecord>,
    }

    struct SomeRecord {
        value: u16,
        offset: Table2,
    }

    struct Table0 {
        version: u16,
        offsets: Vec<Table0a>,
    }

    struct Table0a {
        version: u16,
        offset: Table2,
    }

    struct Table2 {
        version: u16,
        bigness: u16,
    }

    impl FontWrite for Table2 {
        fn write_into(&self, writer: &mut TableWriter) {
            (self.version).write_into(writer);
            (self.bigness).write_into(writer);
        }
    }

    impl FontWrite for Table1 {
        fn write_into(&self, writer: &mut TableWriter) {
            (self.version).write_into(writer);
            for record in &self.records {
                (record.value).write_into(writer);
                writer.write_offset::<Offset16>(&record.offset);
            }
        }
    }

    impl FontWrite for Table0 {
        fn write_into(&self, writer: &mut TableWriter) {
            (self.version).write_into(writer);
            for offset in &self.offsets {
                writer.write_offset::<Offset16>(offset);
            }
        }
    }

    impl FontWrite for Table0a {
        fn write_into(&self, writer: &mut TableWriter) {
            (self.version).write_into(writer);
            writer.write_offset::<Offset16>(&self.offset);
        }
    }

    #[test]
    fn simple_dedup() {
        let table = Table1 {
            version: 0xffff,
            records: vec![
                SomeRecord {
                    value: 0x1010,
                    offset: Table2 {
                        version: 0x2020,
                        bigness: 0x3030,
                    },
                },
                SomeRecord {
                    value: 0x4040,
                    offset: Table2 {
                        version: 0x5050,
                        bigness: 0x6060,
                    },
                },
                SomeRecord {
                    value: 0x6969,
                    offset: Table2 {
                        version: 0x2020,
                        bigness: 0x3030,
                    },
                },
            ],
        };

        let bytes = super::dump_table(&table);
        assert_hex_eq!(bytes.as_slice(), &[
            0xff, 0xff,

            0x10, 0x10,
            0x00, 0x0e, //14

            0x40, 0x40,
            0x00, 0x12, //18

            0x69, 0x69,
            0x00, 0x0e, //14

            0x20, 0x20,
            0x30, 0x30,

            0x50, 0x50,
            0x60, 0x60,
        ]);
    }

    #[test]
    fn sibling_dedup() {
        let table = Table0 {
            version: 0xffff,
            offsets: vec![
                Table0a {
                    version: 0xa1a1,
                    offset: Table2 {
                        version: 0x2020,
                        bigness: 0x3030,
                    },
                },
                Table0a {
                    version: 0xa2a2,
                    offset: Table2 {
                        version: 0x2020,
                        bigness: 0x3030,
                    },
                },
            ],
        };

        let bytes = super::dump_table(&table);

        assert_hex_eq!(bytes.as_slice(), &[
            0xff, 0xff,

            0x00, 0x06, // offset1: 6
            0x00, 0x0a, // offset2: 10

            0xa1, 0xa1, // 0a #1
            0x00, 0x08, //8

            0xa2, 0xa2, // 0a #2
            0x00, 0x4, //4

            0x20, 0x20,
            0x30, 0x30,
        ]);
    }
}
