//! Handling offsets

use crate::serialize::{OffsetWhence, SerializeErrorFlags, Serializer};
use crate::{Plan, Serialize, SubsetTable};
use read_fonts::{
    types::{FixedSize, Scalar},
    MinByteRange,
};

pub(crate) trait SerializeSubset {
    fn serialize_subset<'a, T: SubsetTable<'a>>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        args: T::ArgsForSubset,
        pos: usize,
    ) -> Result<T::Output, SerializeErrorFlags>;
}

impl<O: Scalar> SerializeSubset for O {
    fn serialize_subset<'a, T: SubsetTable<'a>>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        args: T::ArgsForSubset,
        pos: usize,
    ) -> Result<T::Output, SerializeErrorFlags> {
        let key = t.source_key();
        if let Some(key) = key {
            if let Some(&obj_idx) = s.source_objects.get(&key) {
                if let Some(output) = t.cached_output() {
                    s.add_link(
                        pos..pos + O::RAW_BYTE_LEN,
                        obj_idx,
                        OffsetWhence::Head,
                        0,
                        false,
                    )?;
                    return Ok(output);
                }
            }
            if s.active_sources.len() >= crate::MAX_NESTING_LEVEL || !s.active_sources.insert(key) {
                return Err(s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR));
            }
        }
        if let Err(error) = s.push() {
            if let Some(key) = key {
                s.active_sources.remove(&key);
            }
            return Err(error);
        }
        let result = t.subset(plan, s, args);
        if let Some(key) = key {
            s.active_sources.remove(&key);
        }
        match result {
            Ok(ret) => {
                let Some(obj_idx) = s.pop_pack(true) else {
                    return Err(s.error());
                };
                if let Some(key) = key {
                    s.source_objects.insert(key, obj_idx);
                }
                s.add_link(
                    pos..pos + O::RAW_BYTE_LEN,
                    obj_idx,
                    OffsetWhence::Head,
                    0,
                    false,
                )?;
                Ok(ret)
            }
            Err(error) => {
                s.pop_discard();
                Err(error)
            }
        }
    }
}

// this is trait is used to copy simple tables only which implemented MinByteRange trait
pub(crate) trait SerializeCopy {
    fn serialize_copy<'a, T: MinByteRange<'a>>(
        t: &T,
        s: &mut Serializer,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags>;

    fn serialize_copy_from_bytes(
        src_bytes: &[u8],
        s: &mut Serializer,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags>;
}

impl<O: Scalar> SerializeCopy for O {
    fn serialize_copy<'a, T: MinByteRange<'a>>(
        t: &T,
        s: &mut Serializer,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags> {
        s.push()?;
        s.embed_bytes(t.min_table_bytes())?;

        let Some(obj_idx) = s.pop_pack(true) else {
            return Err(s.error());
        };
        s.add_link(
            pos..pos + O::RAW_BYTE_LEN,
            obj_idx,
            OffsetWhence::Head,
            0,
            false,
        )
    }

    fn serialize_copy_from_bytes(
        src_bytes: &[u8],
        s: &mut Serializer,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags> {
        s.push()?;
        s.embed_bytes(src_bytes)?;

        let Some(obj_idx) = s.pop_pack(true) else {
            return Err(s.error());
        };
        s.add_link(
            pos..pos + O::RAW_BYTE_LEN,
            obj_idx,
            OffsetWhence::Head,
            0,
            false,
        )
    }
}

/// this is trait is used to serialize a table that an offset at pos points to
pub(crate) trait SerializeSerialize {
    fn serialize_serialize<'a, T: Serialize<'a>>(
        s: &mut Serializer,
        args: T::Args,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags>;
}

impl<O: Scalar> SerializeSerialize for O {
    fn serialize_serialize<'a, T: Serialize<'a>>(
        s: &mut Serializer,
        args: T::Args,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags> {
        s.push()?;
        match T::serialize(s, args) {
            Ok(()) => {
                let Some(obj_idx) = s.pop_pack(true) else {
                    return Err(s.error());
                };
                s.add_link(
                    pos..pos + O::RAW_BYTE_LEN,
                    obj_idx,
                    OffsetWhence::Head,
                    0,
                    false,
                )?;
                Ok(())
            }
            Err(e) => {
                s.pop_discard();
                Err(e)
            }
        }
    }
}
