//! Handling offsets

use crate::serialize::{OffsetWhence, SerializeErrorFlags, Serializer};
use crate::{Plan, SubsetTable, SubsetTableWithArgs, SubsetTableWithFontData};
use skrifa::raw::FontData;
use write_fonts::{
    read::TableRef,
    types::{FixedSize, Scalar},
};

pub trait SerializeSubset {
    fn serialize_subset<T: SubsetTable>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags>;

    fn serialize_subset_with_args<T: SubsetTableWithArgs, Args>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        pos: usize,
        args: &Args,
    ) -> Result<(), SerializeErrorFlags>;

    fn serialize_subset_with_font_data<'a, T: SubsetTableWithFontData>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        pos: usize,
        data: FontData<'a>,
    ) -> Result<(), SerializeErrorFlags>;
}

impl<Offset: Scalar> SerializeSubset for Offset {
    fn serialize_subset<T: SubsetTable>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags> {
        s.push()?;
        match t.subset(plan, s) {
            Ok(()) => {
                let Some(obj_idx) = s.pop_pack(true) else {
                    return Err(s.error());
                };
                s.add_link(
                    pos..pos + Offset::RAW_BYTE_LEN,
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

    fn serialize_subset_with_args<T: SubsetTableWithArgs, Args>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        pos: usize,
        args: &Args,
    ) -> Result<(), SerializeErrorFlags> {
        s.push()?;
        match t.subset_with_args(plan, s, args) {
            Ok(()) => {
                let Some(obj_idx) = s.pop_pack(true) else {
                    return Err(s.error());
                };
                s.add_link(
                    pos..pos + Offset::RAW_BYTE_LEN,
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

    fn serialize_subset_with_font_data<'a, T: SubsetTableWithFontData>(
        t: &T,
        s: &mut Serializer,
        plan: &Plan,
        pos: usize,
        data: FontData<'a>,
    ) -> Result<(), SerializeErrorFlags> {
        s.push()?;
        match t.subset_with_font_data(plan, s, data) {
            Ok(()) => {
                let Some(obj_idx) = s.pop_pack(true) else {
                    return Err(s.error());
                };
                s.add_link(
                    pos..pos + Offset::RAW_BYTE_LEN,
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

pub trait SerializeCopy {
    fn serialize_copy<'a, T>(
        t: &TableRef<'a, T>,
        s: &mut Serializer,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags>;
}

impl<Offset: Scalar> SerializeCopy for Offset {
    fn serialize_copy<T>(
        t: &TableRef<T>,
        s: &mut Serializer,
        pos: usize,
    ) -> Result<(), SerializeErrorFlags> {
        s.push()?;
        s.embed_bytes(t.offset_data().as_bytes())?;

        let Some(obj_idx) = s.pop_pack(true) else {
            return Err(s.error());
        };
        s.add_link(
            pos..pos + Offset::RAW_BYTE_LEN,
            obj_idx,
            OffsetWhence::Head,
            0,
            false,
        )
    }
}
