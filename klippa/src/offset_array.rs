//! Subset arrays of offsets
use crate::{offset::SerializeSubset, Plan, SerializeErrorFlags, Serializer, SubsetTable};
use write_fonts::{
    read::{ArrayOfNullableOffsets, ArrayOfOffsets, FontReadWithArgs, Offset, ReadArgs},
    types::{FixedSize, Scalar},
};

pub(crate) trait SubsetOffsetArray<'a, T: SubsetTable<'a>> {
    fn subset_offset(
        &self,
        idx: usize,
        s: &mut Serializer,
        plan: &Plan,
        args: &T::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags>;
}

impl<'a, T, O> SubsetOffsetArray<'a, T> for ArrayOfOffsets<'a, T, O>
where
    O: Scalar + Offset + FixedSize + SerializeSubset,
    T: ReadArgs + FontReadWithArgs<'a> + SubsetTable<'a>,
    T::Args: Copy + 'static,
{
    fn subset_offset(
        &self,
        idx: usize,
        s: &mut Serializer,
        plan: &Plan,
        args: &T::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let t = self
            .get(idx)
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_OTHER)?;
        let snap = s.snapshot();
        let offset_pos = s.allocate_size(O::RAW_BYTE_LEN, true)?;

        if O::serialize_subset(&t, s, plan, args, offset_pos).is_err() {
            s.revert_snapshot(snap);
            return Err(s.error());
        }

        Ok(())
    }
}

impl<'a, T, O> SubsetOffsetArray<'a, T> for ArrayOfNullableOffsets<'a, T, O>
where
    O: Scalar + Offset + FixedSize + SerializeSubset,
    T: ReadArgs + FontReadWithArgs<'a> + SubsetTable<'a>,
    T::Args: Copy + 'static,
{
    fn subset_offset(
        &self,
        idx: usize,
        s: &mut Serializer,
        plan: &Plan,
        args: &T::ArgsForSubset,
    ) -> Result<(), SerializeErrorFlags> {
        let Some(Ok(t)) = self.get(idx) else {
            return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
        };

        let snap = s.snapshot();
        let offset_pos = s.allocate_size(O::RAW_BYTE_LEN, true)?;

        if O::serialize_subset(&t, s, plan, args, offset_pos).is_err() {
            s.revert_snapshot(snap);
            return Err(s.error());
        }

        Ok(())
    }
}
