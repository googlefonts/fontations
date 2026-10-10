//! Keep STAT axes and filter axis values as in HarfBuzz's hb-ot-stat-table.hh.
use super::{rebase::Triple, AxisPlan};
use crate::{
    offset::SerializeCopy,
    repack::resolve_overflows,
    serialize::{OffsetWhence, SerializeErrorFlags, Serializer},
    SubsetError,
};
use skrifa::MetadataProvider;
use std::collections::BTreeMap;
use write_fonts::{
    read::{tables::stat::AxisValue, FontRef, TableProvider},
    types::{Fixed, Offset16, Offset32, Tag},
};

const TAG: Tag = Tag::new(b"STAT");

fn keep_axis_value(
    value: &AxisValue,
    design_axes: &[u8],
    axis_size: usize,
    ranges: &BTreeMap<Tag, Triple>,
) -> Result<bool, ()> {
    let in_range = |index: u16, value: Fixed| -> Result<bool, ()> {
        let record = design_axes.get(index as usize * axis_size..).ok_or(())?;
        let tag = Tag::new(record.get(..4).ok_or(())?.try_into().map_err(|_| ())?);
        let value = value.to_f32() as f64;
        // STAT may describe axes absent from fvar, and unspecified axes are
        // not filtered even when a value falls outside the original fvar range.
        Ok(ranges
            .get(&tag)
            .is_none_or(|range| value >= range.0 && value <= range.2))
    };
    match value {
        AxisValue::Format1(v) => in_range(v.axis_index(), v.value()),
        AxisValue::Format2(v) => in_range(v.axis_index(), v.nominal_value()),
        AxisValue::Format3(v) => in_range(v.axis_index(), v.value()),
        AxisValue::Format4(v) => {
            for record in v.axis_values() {
                if !in_range(record.axis_index(), record.value())? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

pub(super) fn instance(
    font: &FontRef,
    axes: &AxisPlan,
    tables: &mut BTreeMap<Tag, Vec<u8>>,
) -> Result<(), SubsetError> {
    let Some(data) = font.data_for_tag(TAG) else {
        return Ok(());
    };
    let err = || SubsetError::SubsetTableError(TAG);
    let stat = font.stat().map_err(|_| err())?;
    if stat.version().major != 1 {
        return Err(err());
    }
    let header = data
        .as_bytes()
        .get(..stat.elided_fallback_name_id_byte_range().end)
        .ok_or_else(err)?;
    let axis_size = stat.design_axis_size() as usize;
    let axis_count = stat.design_axis_count() as usize;
    if axis_count != 0 && axis_size < 8 {
        return Err(err());
    }
    let design_axes = if axis_count == 0 {
        &[][..]
    } else {
        let start = stat.design_axes_offset().to_u32() as usize;
        if start == 0 {
            return Err(err());
        }
        data.as_bytes()
            .get(start..start + axis_count * axis_size)
            .ok_or_else(err)?
    };
    let ranges = font
        .axes()
        .iter()
        .filter(|axis| axes.values.iter().any(|(tag, _)| *tag == axis.tag()))
        .map(|axis| (axis.tag(), axes.user[axis.index()]))
        .collect();
    let mut values = Vec::new();
    if let Some(array) = stat.offset_to_axis_values() {
        let array = array.map_err(|_| err())?;
        for value in array.axis_values().iter() {
            let value = value.map_err(|_| err())?;
            if keep_axis_value(&value, design_axes, axis_size, &ranges).map_err(|_| err())? {
                values.push(value);
            }
        }
    } else if stat.axis_value_count() != 0 {
        return Err(err());
    }
    // Preserve the original version, fallback name, axis records, and every
    // retained value verbatim. Axis indices refer to STAT axes, not fvar axes.
    let mut s = Serializer::new(data.len() * 2 + 64);
    let serialize = |s: &mut Serializer| -> Result<(), SerializeErrorFlags> {
        s.start_serialize()?;
        s.embed_bytes(header)?;
        s.copy_assign(8, 0u32);
        s.copy_assign(12, values.len() as u16);
        s.copy_assign(14, 0u32);
        if !design_axes.is_empty() {
            Offset32::serialize_copy_from_bytes(design_axes, s, 8)?;
        }
        if !values.is_empty() {
            s.push()?;
            for value in &values {
                let pos = s.embed(0u16)?;
                Offset16::serialize_copy(value, s, pos)?;
            }
            let obj = s.pop_pack(true).ok_or_else(|| s.error())?;
            s.add_link(14..18, obj, OffsetWhence::Head, 0, false)?;
        }
        s.end_serialize();
        Ok(())
    };
    serialize(&mut s).map_err(|_| err())?;
    if s.in_error() && !s.only_offset_overflow() {
        return Err(err());
    }
    let bytes = if s.offset_overflow() {
        resolve_overflows(&s, TAG, 32).map_err(|_| err())?
    } else {
        s.copy_bytes()
    };
    tables.insert(TAG, bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::{
        read::{FontData, FontRead},
        tables::stat::{AxisValue as OwnedValue, AxisValueTableFlags},
        types::NameId,
    };

    #[test]
    fn fixed_values_use_harfbuzz_float_comparison_at_range_boundaries() {
        let bytes = write_fonts::dump_table(&OwnedValue::format_1(
            0,
            AxisValueTableFlags::empty(),
            NameId::new(300),
            Fixed::from_bits((10000 << 16) + 1),
        ))
        .unwrap();
        let value = AxisValue::read(FontData::new(&bytes)).unwrap();
        let axes = b"wght\0\0\0\0";
        let ranges = [(Tag::new(b"wght"), Triple(10000., 10000., 10000.))]
            .into_iter()
            .collect();
        assert!(keep_axis_value(&value, axes, 8, &ranges).unwrap());
    }
}
