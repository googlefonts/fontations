//! Flat variation streams appended directly to the destination buffer.
//! Variable stores use Skera's serializer and repacker.

// Match the packed-stream run choices used by write-fonts and FontTools.
// The slice stays borrowed and the stream goes directly into its final buffer.
pub(super) fn packed_deltas(mut values: &[i32], out: &mut Vec<u8>) {
    fn width(value: i32) -> u8 {
        if value == 0 {
            0
        } else if i8::try_from(value).is_ok() {
            1
        } else if i16::try_from(value).is_ok() {
            2
        } else {
            4
        }
    }
    while let Some(&first) = values.first() {
        let kind = width(first);
        let mut count = 1;
        if kind == 0 {
            count = values.iter().take(64).take_while(|&&v| v == 0).count();
        } else {
            while count < 64 && count < values.len() {
                let current = width(values[count]);
                let next = values.get(count + 1).copied().map(width);
                if match kind {
                    1 => current > 1 || current == 0 && next == Some(0),
                    2 => {
                        current == 0 || current == 4 || current == 1 && matches!(next, Some(0 | 1))
                    }
                    4 => current != 4,
                    _ => false,
                } {
                    break;
                }
                count += 1;
            }
        }
        out.push(
            (count as u8 - 1)
                | match kind {
                    0 => 0x80,
                    1 => 0,
                    2 => 0x40,
                    _ => 0xc0,
                },
        );
        for &value in &values[..count] {
            match kind {
                0 => {}
                1 => out.push(value as u8),
                2 => out.extend((value as i16).to_be_bytes()),
                _ => out.extend(value.to_be_bytes()),
            }
        }
        values = &values[count..];
    }
}

pub(super) fn packed_points(points: &[u16], out: &mut Vec<u8>) {
    if points.len() < 128 {
        out.push(points.len() as u8);
    } else {
        out.extend(((points.len() as u16) | 0x8000).to_be_bytes());
    }
    let mut points = points;
    let mut previous = 0u16;
    while let Some(&first) = points.first() {
        let words = first - previous > 255;
        let mut last = previous;
        let mut count = 0;
        for &point in points.iter().take(128) {
            if (point - last > 255) != words {
                break;
            }
            last = point;
            count += 1;
        }
        out.push((count as u8 - 1) | if words { 0x80 } else { 0 });
        for &point in &points[..count] {
            let delta = point - previous;
            previous = point;
            if words {
                out.extend(delta.to_be_bytes());
            } else {
                out.push(delta as u8);
            }
        }
        points = &points[count..];
    }
}

/// Serialize the flat store model through Skera's normal graph and repacker.
/// Store models contain a region list and byte rows, with no shared layout graph.
pub(crate) fn item_store(
    table: &write_fonts::tables::variations::ItemVariationStore,
) -> Result<Vec<u8>, crate::SubsetError> {
    use crate::{serialize::Serializer, SubsetTable};
    let size = item_store_size(table);
    let mut s = Serializer::new(size)
        .map_err(|_| crate::SubsetError::SubsetTableError(read_fonts::types::Tag::new(b"GDEF")))?;
    s.start_serialize()
        .map_err(|_| crate::SubsetError::SubsetTableError(read_fonts::types::Tag::new(b"GDEF")))?;
    table
        .subset(&crate::Plan::default(), &mut s, ())
        .map_err(|_| crate::SubsetError::SubsetTableError(read_fonts::types::Tag::new(b"GDEF")))?;
    s.end_serialize();
    if s.in_error() {
        return Err(crate::SubsetError::SubsetTableError(
            read_fonts::types::Tag::new(b"GDEF"),
        ));
    }
    Ok(s.copy_bytes())
}
impl crate::SubsetTable<'_> for write_fonts::tables::variations::ItemVariationStore {
    type ArgsForSubset = ();
    type Output = ();
    fn subset(
        &self,
        _: &crate::Plan,
        s: &mut crate::serialize::Serializer,
        _: (),
    ) -> Result<(), crate::serialize::SerializeErrorFlags> {
        use crate::offset::SerializeCopy;
        use read_fonts::types::Offset32;
        s.embed(1u16)?;
        let region_pos = s.embed(0u32)?;
        s.embed(self.item_variation_data.len() as u16)?;
        for data in &self.item_variation_data {
            let pos = s.embed(0u32)?;
            if let Some(data) = data.as_ref() {
                let mut bytes =
                    Vec::with_capacity(6 + data.region_indexes.len() * 2 + data.delta_sets.len());
                bytes.extend(data.item_count.to_be_bytes());
                bytes.extend(data.word_delta_count.to_be_bytes());
                bytes.extend((data.region_indexes.len() as u16).to_be_bytes());
                for index in &data.region_indexes {
                    bytes.extend(index.to_be_bytes());
                }
                bytes.extend(&data.delta_sets);
                Offset32::serialize_copy_from_bytes(&bytes, s, pos)?;
            }
        }
        let list = self.variation_region_list.as_ref();
        let mut bytes = Vec::new();
        bytes.extend(list.axis_count.to_be_bytes());
        bytes.extend((list.variation_regions.len() as u16).to_be_bytes());
        for region in &list.variation_regions {
            for axis in &region.region_axes {
                for coord in [axis.start_coord, axis.peak_coord, axis.end_coord] {
                    bytes.extend(coord.to_bits().to_be_bytes());
                }
            }
        }
        Offset32::serialize_copy_from_bytes(&bytes, s, region_pos)
    }
}

pub(crate) fn item_store_size(
    table: &write_fonts::tables::variations::ItemVariationStore,
) -> usize {
    let mut size = 32;
    for data in table.item_variation_data.iter().filter_map(|v| v.as_ref()) {
        size += 16 + data.region_indexes.len() * 2 + data.delta_sets.len();
    }
    size += table.variation_region_list.variation_regions.len()
        * usize::from(table.variation_region_list.axis_count)
        * 6;
    size
}

pub(crate) fn segment_map(
    map: &write_fonts::tables::avar::SegmentMaps,
) -> Result<Vec<u8>, crate::SubsetError> {
    let count = u16::try_from(map.axis_value_maps.len())
        .map_err(|_| crate::SubsetError::SubsetTableError(read_fonts::types::Tag::new(b"avar")))?;
    let mut bytes = Vec::with_capacity(2 + usize::from(count) * 4);
    bytes.extend(count.to_be_bytes());
    for value in &map.axis_value_maps {
        bytes.extend(value.from_coordinate.to_bits().to_be_bytes());
        bytes.extend(value.to_coordinate.to_bits().to_be_bytes());
    }
    Ok(bytes)
}
pub(crate) fn fvar(table: &write_fonts::tables::fvar::Fvar) -> Result<Vec<u8>, crate::SubsetError> {
    let error = || crate::SubsetError::SubsetTableError(read_fonts::types::Tag::new(b"fvar"));
    let arrays = table.axis_instance_arrays.as_ref();
    let axis_count = u16::try_from(arrays.axes.len()).map_err(|_| error())?;
    let instance_count = u16::try_from(arrays.instances.len()).map_err(|_| error())?;
    let ps = arrays
        .instances
        .first()
        .is_some_and(|v| v.post_script_name_id.is_some());
    let size = u16::try_from(4 + usize::from(axis_count) * 4 + usize::from(ps) * 2)
        .map_err(|_| error())?;
    let mut out = Vec::new();
    for value in [1u16, 0, 16, 2, axis_count, 20, instance_count, size] {
        out.extend(value.to_be_bytes());
    }
    for axis in &arrays.axes {
        out.extend(axis.axis_tag.into_bytes());
        for value in [axis.min_value, axis.default_value, axis.max_value] {
            out.extend(value.to_bits().to_be_bytes());
        }
        out.extend(axis.flags.to_be_bytes());
        out.extend(axis.axis_name_id.to_u16().to_be_bytes());
    }
    for instance in &arrays.instances {
        if instance.coordinates.len() != usize::from(axis_count)
            || instance.post_script_name_id.is_some() != ps
        {
            return Err(error());
        }
        out.extend(instance.subfamily_name_id.to_u16().to_be_bytes());
        out.extend(instance.flags.to_be_bytes());
        for value in &instance.coordinates {
            out.extend(value.to_bits().to_be_bytes());
        }
        if let Some(name) = instance.post_script_name_id {
            out.extend(name.to_u16().to_be_bytes());
        }
    }
    Ok(out)
}
pub(crate) fn avar(table: &write_fonts::tables::avar::Avar) -> Result<Vec<u8>, crate::SubsetError> {
    use crate::{
        offset::{SerializeCopy, SerializeSubset},
        serialize::Serializer,
    };
    use read_fonts::types::Offset32;
    let error = || crate::SubsetError::SubsetTableError(read_fonts::types::Tag::new(b"avar"));
    let maps = table
        .axis_segment_maps
        .iter()
        .map(segment_map)
        .collect::<Result<Vec<_>, _>>()?;
    let version = if table.axis_index_map.is_some() || table.var_store.is_some() {
        2u16
    } else {
        1u16
    };
    let mut s = Serializer::new(
        32 + maps.iter().map(Vec::len).sum::<usize>()
            + table.var_store.as_ref().map_or(0, item_store_size)
            + table.axis_index_map.as_ref().map_or(0, |map| match map {
                write_fonts::tables::variations::DeltaSetIndexMap::Format0(v) => {
                    v.map_data.len() + 6
                }
                write_fonts::tables::variations::DeltaSetIndexMap::Format1(v) => {
                    v.map_data.len() + 6
                }
            }),
    )
    .map_err(|_| error())?;
    s.start_serialize().map_err(|_| error())?;
    s.embed(version).map_err(|_| error())?;
    s.embed(0u16).map_err(|_| error())?;
    s.embed(0u16).map_err(|_| error())?;
    s.embed(u16::try_from(maps.len()).map_err(|_| error())?)
        .map_err(|_| error())?;
    for map in maps {
        s.embed_bytes(&map).map_err(|_| error())?;
    }
    if version == 2 {
        let pos = s.embed(0u32).map_err(|_| error())?;
        if let Some(map) = table.axis_index_map.as_ref() {
            use write_fonts::tables::variations::DeltaSetIndexMap;
            let (format, entry, data) = match map {
                DeltaSetIndexMap::Format0(v) => (0u8, v.entry_format, &v.map_data),
                DeltaSetIndexMap::Format1(v) => (1u8, v.entry_format, &v.map_data),
            };
            let mut bytes = vec![format, entry.bits()];
            let count = data.len() / usize::from(entry.entry_size());
            if format == 0 {
                bytes.extend(u16::try_from(count).map_err(|_| error())?.to_be_bytes());
            } else {
                bytes.extend(u32::try_from(count).map_err(|_| error())?.to_be_bytes());
            }
            bytes.extend(data);
            Offset32::serialize_copy_from_bytes(&bytes, &mut s, pos).map_err(|_| error())?;
        }
        let pos = s.embed(0u32).map_err(|_| error())?;
        if let Some(store) = table.var_store.as_ref() {
            Offset32::serialize_subset(store, &mut s, &crate::Plan::default(), (), pos)
                .map_err(|_| error())?;
        }
    }
    s.end_serialize();
    if s.in_error() {
        return Err(error());
    }
    Ok(s.copy_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::tables::variations::{PackedDeltas, PackedPointNumbers};

    #[test]
    fn delta_streams_match_writer_at_run_and_integer_boundaries() {
        let values = [
            i32::MIN,
            -32769,
            -32768,
            -129,
            -128,
            -1,
            0,
            1,
            127,
            128,
            32767,
            32768,
            i32::MAX,
        ];
        for &a in &values {
            for &b in &values {
                for len in [0, 1, 2, 63, 64, 65, 128, 129] {
                    let values: Vec<_> = (0..len).map(|i| if i % 3 == 0 { a } else { b }).collect();
                    let mut bytes = Vec::new();
                    packed_deltas(&values, &mut bytes);
                    assert_eq!(
                        bytes,
                        write_fonts::dump_table(&PackedDeltas::new(values)).unwrap()
                    );
                }
            }
        }
    }

    #[test]
    fn point_streams_match_writer_at_count_and_distance_boundaries() {
        for points in [
            vec![],
            vec![0],
            vec![255, 256, 511, 767, 768, 65535],
            (0..127).collect(),
            (0..128).collect(),
            (0..129).collect(),
            (0..255).map(|v| v * 256).collect(),
            (0..=32766).collect(),
        ] {
            let mut bytes = Vec::new();
            packed_points(&points, &mut bytes);
            assert_eq!(
                bytes,
                write_fonts::dump_table(&PackedPointNumbers::Some(points)).unwrap()
            );
        }
    }
}
