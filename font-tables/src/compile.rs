//! compiling font tables

use std::collections::HashMap;

use font_types::{Offset, Offset16, OffsetLen, Uint24};

mod graph;

use graph::{ObjectId, ObjectStore};

#[cfg(test)]
mod hex_diff;

pub trait Table {
    /// Write our data and information about offsets into this [TableWriter].
    fn describe(&self, writer: &mut TableWriter);
}

#[derive(Debug, Default)]
pub struct TableWriter {
    tables: ObjectStore,
    stack: Vec<TableData>,
}

pub fn dump_table<T: Table>(table: &T) -> Vec<u8> {
    let mut writer = TableWriter::default();
    let root = writer.add_table(table);
    assert!(writer.stack.is_empty());
    let graph = writer.tables.into_graph();
    let sorted = graph.kahn_sort(root);

    let mut offsets = HashMap::new();
    let mut out = Vec::new();
    let mut off = 0;

    // first pass: write out bytes, record positions of offsets
    for id in &sorted {
        let node = graph.get_node(*id).unwrap();
        offsets.insert(*id, off);
        off += node.bytes.len() as u32;
        out.extend_from_slice(&node.bytes);
    }

    // second pass: write offsets
    let mut off = 0;
    for id in &sorted {
        let node = graph.get_node(*id).unwrap();
        for offset in &node.offsets {
            let resolved = *offsets.get(&offset.object).unwrap();
            let pos = off + offset.pos as usize;
            let write_over = out.get_mut(pos..).unwrap();
            write_offset(write_over, offset.len, resolved).unwrap();
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
    //fn add_table<T: Table + ?Sized>(&mut self, table: &T) -> ObjectId {
    fn add_table(&mut self, table: &dyn Table) -> ObjectId {
        self.stack.push(TableData::default());
        table.describe(self);
        let data = self.stack.pop().unwrap();
        self.tables.add(data)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        self.stack.last_mut().unwrap().write(bytes)
    }

    pub fn write_offset16(&mut self, obj: &dyn Table) {
        let obj_id = self.add_table(obj);
        let data = self.stack.last_mut().unwrap();
        data.add_offset::<Offset16>(obj_id);
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
    fn add_offset<T: Offset>(&mut self, object: ObjectId) {
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
}

#[cfg(test)]
#[rustfmt::skip::macros(assert_hex_eq)]
mod tests {
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

    struct Table2 {
        version: u16,
        bigness: u16,
    }

    impl Table for Table2 {
        fn describe(&self, writer: &mut TableWriter) {
            writer.write(&self.version.to_be_bytes());
            writer.write(&self.bigness.to_be_bytes());
        }
    }

    impl Table for Table1 {
        fn describe(&self, writer: &mut TableWriter) {
            writer.write(&self.version.to_be_bytes());
            for record in &self.records {
                writer.write(&record.value.to_be_bytes());
                writer.write_offset16(&record.offset);
            }
        }
    }

    #[test]
    fn weeeeee() {
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
            0x00, 0x12, //18

            0x40, 0x40,
            0x00, 0x0e, //14

            0x69, 0x69,
            0x00, 0x12, //18

            0x50, 0x50,
            0x60, 0x60,

            0x20, 0x20,
            0x30, 0x30,
        ]);
    }
}
