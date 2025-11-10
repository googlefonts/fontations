//! Read layout tables in a graph
//!
use crate::{
    graph::{Graph, RepackErrorFlags},
    serialize::{LinkWidth, ObjIdx},
};
use fnv::FnvHashMap;
use write_fonts::types::Scalar;
struct ExtensionSubtable<'a> {
    bytes: &'a mut [u8],
}

impl<'a> ExtensionSubtable<'a> {
    const FORMAT_BYTE_LEN: usize = 2;
    const LOOKUP_TYPE_LEN: usize = 2;
    fn from_graph(graph: &'a mut Graph, obj_idx: ObjIdx) -> Result<Self, RepackErrorFlags> {
        let ext = graph
            .obj_data(obj_idx)
            .map(move |data| Self { bytes: data })
            .ok_or_else(|| RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX)?;

        if !ext.sanitize() {
            return Err(RepackErrorFlags::REPACK_ERROR_EXT_PROMOTION);
        }
        Ok(ext)
    }

    fn sanitize(&self) -> bool {
        self.bytes.len() >= EXTENSION_TABLE_SIZE
    }

    fn reset(&mut self, lookup_type: u16) {
        assert!(self.bytes.len() == EXTENSION_TABLE_SIZE);
        // format = 1
        self.bytes
            .get_mut(0..Self::FORMAT_BYTE_LEN)
            .unwrap()
            .copy_from_slice(&1_u16.to_be_bytes());

        self.bytes
            .get_mut(Self::FORMAT_BYTE_LEN..Self::FORMAT_BYTE_LEN + Self::LOOKUP_TYPE_LEN)
            .unwrap()
            .copy_from_slice(&lookup_type.to_be_bytes());
    }
}

pub(crate) struct Lookup<'a> {
    bytes: &'a mut [u8],
}

impl<'a> Lookup<'a> {
    const LOOKUP_MIN_SIZE: usize = 6;
    const LOOKUP_TYPE_POS: usize = 0;
    const LOOKUP_FLAG_POS: usize = 2;
    const NUM_SUBTABLE_BYTE_POS: usize = 4;

    pub(crate) fn from_graph(
        graph: &'a mut Graph,
        obj_idx: ObjIdx,
    ) -> Result<Self, RepackErrorFlags> {
        let lookup = graph
            .obj_data(obj_idx)
            .map(move |data| Self { bytes: data })
            .ok_or_else(|| RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX)?;

        if !lookup.sanitize() {
            return Err(RepackErrorFlags::REPACK_ERROR_EXT_PROMOTION);
        }
        Ok(lookup)
    }

    fn sanitize(&self) -> bool {
        if self.bytes.len() < Self::LOOKUP_MIN_SIZE {
            return false;
        }
        let num_subtables = self.num_subtables();

        let size = Self::LOOKUP_MIN_SIZE
            + num_subtables as usize * 2
            + if self.use_mark_filtering_set() { 2 } else { 0 };
        self.bytes.len() >= size
    }

    pub(crate) fn num_subtables(&self) -> u16 {
        let num_subtable_bytes = self
            .bytes
            .get(Self::NUM_SUBTABLE_BYTE_POS..Self::NUM_SUBTABLE_BYTE_POS + 2)
            .unwrap();
        u16::read(num_subtable_bytes).unwrap()
    }

    fn use_mark_filtering_set(&self) -> bool {
        let lookup_flag_bytes = self
            .bytes
            .get(Self::LOOKUP_FLAG_POS..Self::LOOKUP_FLAG_POS + 2)
            .unwrap();
        let lookup_flag = u16::read(lookup_flag_bytes).unwrap();
        (lookup_flag & 0x0010_u16) != 0
    }

    pub(crate) fn lookup_type(&self) -> u16 {
        let num_subtable_bytes = self
            .bytes
            .get(Self::LOOKUP_TYPE_POS..Self::LOOKUP_TYPE_POS + 2)
            .unwrap();
        u16::read(num_subtable_bytes).unwrap()
    }

    fn set_lookup_type(&mut self, lookup_type: u16) {
        let lookup_type_bytes = self
            .bytes
            .get_mut(Self::LOOKUP_TYPE_POS..Self::LOOKUP_TYPE_POS + 2)
            .unwrap();
        lookup_type_bytes.copy_from_slice(&lookup_type.to_be_bytes());
    }
}

// num of bytes for an extension subtable
pub(crate) const EXTENSION_TABLE_SIZE: usize = 8;
impl Graph {
    fn create_extension_subtable(
        &mut self,
        subtable_idx: ObjIdx,
        lookup_type: u16,
    ) -> Result<ObjIdx, RepackErrorFlags> {
        let ext_idx = self.new_vertex(EXTENSION_TABLE_SIZE);
        let mut ext_subtable = ExtensionSubtable::from_graph(self, ext_idx)?;

        ext_subtable.reset(lookup_type);

        // Make extension point at the subtable
        self.vertices[ext_idx].add_link(LinkWidth::Four, subtable_idx, 4);
        Ok(ext_idx)
    }

    fn obj_data(&mut self, obj_idx: ObjIdx) -> Option<&mut [u8]> {
        let v = &self.vertices[obj_idx];
        self.data.get_mut(v.head..v.tail)
    }

    fn make_subtable_extension(
        &mut self,
        lookup_idx: ObjIdx,
        subtable_idx: ObjIdx,
        lookup_type: u16,
        idx_map: &mut FnvHashMap<usize, usize>,
    ) -> Result<usize, RepackErrorFlags> {
        let ext_idx = if let Some(idx) = idx_map.get(&subtable_idx) {
            let Some(subtable_v) = self.vertices.get_mut(subtable_idx) else {
                return Err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX);
            };
            subtable_v.remove_parent(lookup_idx);
            *idx
        } else {
            let idx = self.create_extension_subtable(subtable_idx, lookup_type)?;
            idx_map.insert(subtable_idx, idx);

            let Some(subtable_v) = self.vertices.get_mut(subtable_idx) else {
                return Err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX);
            };
            subtable_v.remap_parent(lookup_idx, idx);
            idx
        };

        for l in self.vertices[lookup_idx].real_links.values_mut() {
            if l.obj_idx() == subtable_idx {
                l.update_obj_idx(ext_idx);
            }
        }
        self.vertices[ext_idx].add_parent(lookup_idx, false);
        Ok(ext_idx)
    }

    pub(crate) fn make_extension(
        &mut self,
        lookup_idx: ObjIdx,
        lookup_type: u16,
        extension_type: Option<u16>,
        idx_map: &mut FnvHashMap<usize, usize>,
    ) -> Result<(), RepackErrorFlags> {
        let Some(ext_type) = extension_type else {
            return Ok(());
        };
        let Some(lookup_v) = self.vertices.get(lookup_idx) else {
            return Err(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX);
        };

        let subtable_idxes = lookup_v.child_idxes();
        for subtable_idx in subtable_idxes {
            self.make_subtable_extension(lookup_idx, subtable_idx, lookup_type, idx_map)?;
        }

        let mut lookup_table = Lookup::from_graph(self, lookup_idx)?;
        lookup_table.set_lookup_type(ext_type);
        Ok(())
    }
}
