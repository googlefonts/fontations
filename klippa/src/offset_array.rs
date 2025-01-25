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
            .map_err(|_| SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR)?;
        let snap = s.snapshot();
        let offset_pos = s.allocate_size(O::RAW_BYTE_LEN, true)?;

        if let Err(e) = O::serialize_subset(&t, s, plan, args, offset_pos) {
            s.revert_snapshot(snap);
            return Err(e);
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
        match self.get(idx) {
            Some(Ok(t)) => {
                let snap = s.snapshot();
                let offset_pos = s.allocate_size(O::RAW_BYTE_LEN, true)?;

                match O::serialize_subset(&t, s, plan, args, offset_pos) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        s.revert_snapshot(snap);
                        Err(e)
                    }
                }
            }
            None => Err(SerializeErrorFlags::SERIALIZE_ERROR_EMPTY),
            Some(Err(_)) => Err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR),
        }
    }
}
