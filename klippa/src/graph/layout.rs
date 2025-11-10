//! Read layout tables in a graph
use crate::{
    graph::{Graph, RepackErrorFlags},
    serialize::{LinkWidth, ObjIdx},
};
use fnv::FnvHashMap;
use write_fonts::types::{FixedSize, Scalar};

struct DataBytes<'a> {
    bytes: &'a mut [u8],
}

impl DataBytes<'_> {
    /// Read a scalar at the provided location in the data bytes.
    /// caller is responsible for ensuring no out-of-bound reading
    fn read_at<T: Scalar>(&self, pos: usize) -> T {
        T::read(&self.bytes[pos..pos + T::RAW_BYTE_LEN]).unwrap()
    }

    /// Write a scalar value over existing data at the provided location in the data bytes.
    /// caller is responsible for ensuring no out-of-bound writing
    fn write_at<T: Scalar>(&mut self, value: T, pos: usize) {
        let src_bytes = value.to_raw();
        self.bytes[pos..pos + T::RAW_BYTE_LEN].copy_from_slice(src_bytes.as_ref());
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }
}
struct ExtensionSubtable<'a>(DataBytes<'a>);

impl<'a> ExtensionSubtable<'a> {
    const FORMAT_BYTE_POS: usize = 0;
    const LOOKUP_TYPE_POS: usize = 2;
    fn from_graph(graph: &'a mut Graph, obj_idx: ObjIdx) -> Result<Self, RepackErrorFlags> {
        let ext = graph
            .obj_data(obj_idx)
            .map(|data| Self(DataBytes { bytes: data }))
            .ok_or(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX)?;

        if !ext.sanitize() {
            return Err(RepackErrorFlags::REPACK_ERROR_EXT_PROMOTION);
        }
        Ok(ext)
    }

    fn sanitize(&self) -> bool {
        self.0.len() >= EXTENSION_TABLE_SIZE
    }

    fn reset(&mut self, lookup_type: u16) {
        assert!(self.0.len() == EXTENSION_TABLE_SIZE);
        // format = 1
        self.0.write_at(1_u16, Self::FORMAT_BYTE_POS);
        self.0.write_at(lookup_type, Self::LOOKUP_TYPE_POS);
    }
}

pub(crate) struct Lookup<'a>(DataBytes<'a>);

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
            .map(|data| Self(DataBytes { bytes: data }))
            .ok_or(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX)?;

        if !lookup.sanitize() {
            return Err(RepackErrorFlags::REPACK_ERROR_EXT_PROMOTION);
        }
        Ok(lookup)
    }

    fn sanitize(&self) -> bool {
        if self.0.len() < Self::LOOKUP_MIN_SIZE {
            return false;
        }
        let num_subtables = self.num_subtables();

        let size = Self::LOOKUP_MIN_SIZE
            + num_subtables as usize * 2
            + if self.use_mark_filtering_set() { 2 } else { 0 };
        self.0.len() >= size
    }

    pub(crate) fn num_subtables(&self) -> u16 {
        self.0.read_at::<u16>(Self::NUM_SUBTABLE_BYTE_POS)
    }

    fn use_mark_filtering_set(&self) -> bool {
        let lookup_flag = self.0.read_at::<u16>(Self::LOOKUP_FLAG_POS);
        (lookup_flag & 0x0010_u16) != 0
    }

    pub(crate) fn lookup_type(&self) -> u16 {
        self.0.read_at::<u16>(Self::LOOKUP_TYPE_POS)
    }

    fn set_lookup_type(&mut self, lookup_type: u16) {
        self.0.write_at(lookup_type, Self::LOOKUP_TYPE_POS);
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
            let subtable_v = self
                .vertices
                .get_mut(subtable_idx)
                .ok_or(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX)?;
            subtable_v.remove_parent(lookup_idx);
            *idx
        } else {
            let idx = self.create_extension_subtable(subtable_idx, lookup_type)?;
            idx_map.insert(subtable_idx, idx);

            let subtable_v = self
                .vertices
                .get_mut(subtable_idx)
                .ok_or(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX)?;
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
        let lookup_v = self
            .vertices
            .get(lookup_idx)
            .ok_or(RepackErrorFlags::GRAPH_ERROR_INVALID_OBJ_INDEX)?;

        let subtable_idxes = lookup_v.child_idxes();
        for subtable_idx in subtable_idxes {
            self.make_subtable_extension(lookup_idx, subtable_idx, lookup_type, idx_map)?;
        }

        let mut lookup_table = Lookup::from_graph(self, lookup_idx)?;
        lookup_table.set_lookup_type(ext_type);
        Ok(())
    }
}
