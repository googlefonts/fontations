//! Handling offsets

use crate::serialize::{OffsetWhence, SerializeErrorFlags, Serializer};
use crate::{Plan, SubsetTable};
use write_fonts::{
    read::{MinByteRange, TableRef},
    types::{FixedSize, Scalar},
};

pub(crate) trait SerializeSubset {
    fn serialize_subset<'a, T: SubsetTable<'a>>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        args: &T::ArgsForSubset,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags>;
}

impl<O: Scalar> SerializeSubset for O {
    fn serialize_subset<'a, T: SubsetTable<'a>>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        args: &T::ArgsForSubset,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags> {
        s.push()?;
        match t.subset(plan, s, args) {
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
                )
            }
            Err(e) => {
                s.pop_discard();
                Err(e)
            }
        }
    }
}

// this is trait is used to copy simple tables only which implemented MinByteRange trait
pub(crate) trait SerializeCopy {
    fn serialize_copy<T: MinByteRange>(
        t: &TableRef<T>,
        s: &mut Serializer,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags>;
}

impl<O: Scalar> SerializeCopy for O {
    fn serialize_copy<T: MinByteRange>(
        t: &TableRef<T>,
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
}
