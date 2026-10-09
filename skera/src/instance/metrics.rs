//! Update CFF2 metrics at the instance's new default, including empty glyphs.
use super::{scalars::VariationScalars, AxisPlan, StorePlan};
use crate::SubsetError;
use std::collections::BTreeMap;
use write_fonts::{
    read::{
        tables::variations::{DeltaSetIndex, DeltaSetIndexMap, ItemVariationStore},
        FontRef, ReadError, TableProvider,
    },
    types::{GlyphId, Tag},
};

type Tables = BTreeMap<Tag, Vec<u8>>;
fn error(tag: Tag) -> SubsetError {
    SubsetError::SubsetTableError(tag)
}
fn signed(data: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}
fn put(data: &mut [u8], offset: usize, value: i32) -> Result<(), SubsetError> {
    data.get_mut(offset..offset + 2)
        .ok_or_else(|| error(Tag::new(b"head")))?
        .copy_from_slice(&(value.clamp(-32768, 32767) as i16).to_be_bytes());
    Ok(())
}
fn rounded(value: f64) -> i32 {
    // HarfBuzz's metric/bounds roundf rounds ties toward positive infinity.
    // Its CFF blend folding instead uses round (ties away from zero).
    (value + 0.5)
        .floor()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

struct MetricStore<'a> {
    store: ItemVariationStore<'a>,
    scalars: VariationScalars,
}
impl<'a> MetricStore<'a> {
    fn new(store: ItemVariationStore<'a>, coords: &[write_fonts::types::F2Dot14]) -> Option<Self> {
        let scalars = VariationScalars::new(&store, coords)?;
        Some(Self { store, scalars })
    }
    fn indexed(&self, index: DeltaSetIndex) -> Option<f64> {
        self.scalars.delta(&self.store, index).map(f64::from)
    }
    fn mapped(
        &self,
        map: Option<Result<DeltaSetIndexMap, ReadError>>,
        gid: GlyphId,
        implicit: bool,
    ) -> Option<f64> {
        let index = if let Some(map) = map {
            map.ok()?.get(gid.to_u32()).ok()?
        } else if implicit {
            DeltaSetIndex {
                outer: 0,
                inner: u16::try_from(gid.to_u32()).ok()?,
            }
        } else {
            return Some(0.);
        };
        self.indexed(index)
    }
}

fn mvar_store<'a>(
    font: &FontRef<'a>,
    coords: &[write_fonts::types::F2Dot14],
) -> Option<MetricStore<'a>> {
    MetricStore::new(font.mvar().ok()?.item_variation_store()?.ok()?, coords)
}
fn mvar_delta(font: &FontRef, store: &MetricStore, tag: Tag) -> Option<f64> {
    let mvar = font.mvar().ok()?;
    let records = mvar.value_records();
    let index = records
        .binary_search_by(|record| record.value_tag().cmp(&tag))
        .ok()?;
    let record = &records[index];
    store.indexed(DeltaSetIndex {
        outer: record.delta_set_outer_index(),
        inner: record.delta_set_inner_index(),
    })
}

// Match HarfBuzz's CFF vertical-origin fallback when VORG is absent: center
// the glyph in the horizontal line's advance, including MVAR at this location.
fn horizontal_span(font: &FontRef, axes: &AxisPlan) -> i64 {
    use write_fonts::read::tables::os2::SelectionFlags;
    let typo = font.os2().ok().filter(|os2| {
        os2.fs_selection()
            .contains(SelectionFlags::USE_TYPO_METRICS)
    });
    let (ascender, descender) = if let Some(os2) = typo {
        (os2.s_typo_ascender() as f64, os2.s_typo_descender() as f64)
    } else if let Ok(hhea) = font.hhea() {
        (
            hhea.ascender().to_i16() as f64,
            hhea.descender().to_i16() as f64,
        )
    } else {
        let upem = font.head().map_or(1000., |h| h.units_per_em() as f64);
        return (upem * 0.8).round() as i64 + (upem * 0.2).round() as i64;
    };
    let store = mvar_store(font, &axes.metric_coords);
    let delta = |tag| {
        store
            .as_ref()
            .and_then(|s| mvar_delta(font, s, Tag::new(tag)))
            .unwrap_or(0.)
    };
    let ascender = rounded((ascender + delta(b"hasc")).abs()) as i64;
    let descender = rounded((descender + delta(b"hdsc")).abs()) as i64;
    ascender + descender
}

pub(super) fn instance(
    font: &FontRef,
    axes: &AxisPlan,
    tables: &mut Tables,
) -> Result<(), SubsetError> {
    let source_bounds = crate::cff::instance_bounds(font, axes)?;
    let count = font
        .maxp()
        .map_err(|_| error(Tag::new(b"maxp")))?
        .num_glyphs() as usize;
    let hmtx = font.hmtx().ok();
    let vmtx = font.vmtx().ok();
    let hvar = font.hvar().ok();
    let vvar = font.vvar().ok();
    let hvar_store = hvar
        .as_ref()
        .and_then(|v| MetricStore::new(v.item_variation_store().ok()?, &axes.metric_coords));
    let vvar_store = vvar
        .as_ref()
        .and_then(|v| MetricStore::new(v.item_variation_store().ok()?, &axes.metric_coords));
    let vorg = font.vorg().ok();
    let horizontal_span = horizontal_span(font, axes);
    let mut bounds = Vec::new();
    let mut union: Option<[i32; 4]> = None;
    for bound in source_bounds {
        // HarfBuzz treats an all-zero rounded extent as having no bounds;
        // preserve the input bearing for these glyphs, as for empty programs.
        let bound = bound.map(|b| b.map(rounded)).filter(|b| *b != [0; 4]);
        if let Some(b) = bound {
            union = Some(match union {
                None => b,
                Some(a) => [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ],
            });
        }
        bounds.push(bound);
    }
    if let Some(head) = tables.get_mut(&Tag::new(b"head")) {
        for (i, v) in union.unwrap_or([0; 4]).into_iter().enumerate() {
            put(head, 36 + i * 2, v)?;
        }
    }
    for vertical in [false, true] {
        if (vertical && vmtx.is_none()) || (!vertical && hmtx.is_none()) {
            continue;
        }
        let mut out = Vec::new();
        let mut max_advance = 0;
        let mut min_leading = i32::MAX;
        let mut min_trailing = i32::MAX;
        let mut max_extent = i32::MIN;
        let mut origins = Vec::new();
        for (gid, bound) in bounds.iter().enumerate() {
            let gid = GlyphId::new(gid as u32);
            let base = if vertical {
                vmtx.as_ref().and_then(|m| m.advance(gid))
            } else {
                hmtx.as_ref().and_then(|m| m.advance(gid))
            }
            .unwrap_or(0) as i32;
            let delta = if vertical {
                vvar_store
                    .as_ref()
                    .and_then(|s| s.mapped(vvar.as_ref()?.advance_height_mapping(), gid, true))
            } else {
                hvar_store
                    .as_ref()
                    .and_then(|s| s.mapped(hvar.as_ref()?.advance_width_mapping(), gid, true))
            }
            .map_or(0, rounded);
            let advance = base.saturating_add(delta).clamp(0, 65535);
            let old_bearing = if vertical {
                vmtx.as_ref().and_then(|m| m.side_bearing(gid))
            } else {
                hmtx.as_ref().and_then(|m| m.side_bearing(gid))
            }
            .unwrap_or(0) as i32;
            let origin = if vertical {
                let origin = if let Some(vorg) = &vorg {
                    (vorg.vertical_origin_y(gid) as i32).saturating_add(
                        vvar_store
                            .as_ref()
                            .and_then(|s| s.mapped(vvar.as_ref()?.v_org_mapping(), gid, false))
                            .map_or(0, rounded),
                    )
                } else {
                    bound.map_or(0, |b| {
                        let height = b[3] as i64 - b[1] as i64;
                        (b[3] as i64 + ((horizontal_span - height) >> 1))
                            .clamp(i32::MIN as i64, i32::MAX as i64) as i32
                    })
                };
                origins.push((gid, origin));
                origin
            } else {
                0
            };
            let leading = if let Some(b) = bound {
                if vertical {
                    origin.saturating_sub(b[3])
                } else {
                    b[0]
                }
            } else {
                old_bearing
            };
            let extent = bound.map_or(0, |b| {
                if vertical {
                    b[3].saturating_sub(b[1])
                } else {
                    b[2].saturating_sub(b[0])
                }
            });
            out.extend((advance as u16).to_be_bytes());
            out.extend((leading.clamp(-32768, 32767) as i16).to_be_bytes());
            max_advance = max_advance.max(advance);
            min_leading = min_leading.min(leading);
            min_trailing = min_trailing.min(advance.saturating_sub(leading).saturating_sub(extent));
            max_extent = max_extent.max(leading.saturating_add(extent));
        }
        if !vertical {
            if let Some(os2) = tables.get_mut(&Tag::new(b"OS/2")) {
                let (sum, count) = out.chunks_exact(4).fold((0u64, 0u64), |(sum, count), m| {
                    let advance = u16::from_be_bytes([m[0], m[1]]) as u64;
                    (sum + advance, count + u64::from(advance != 0))
                });
                let average = if count == 0 {
                    0
                } else {
                    (sum + count / 2) / count
                };
                put(os2, 2, average as i32)?;
            }
        }
        tables.insert(Tag::new(if vertical { b"vmtx" } else { b"hmtx" }), out);
        if let Some(header) = tables.get_mut(&Tag::new(if vertical { b"vhea" } else { b"hhea" })) {
            header
                .get_mut(10..12)
                .ok_or_else(|| error(Tag::new(b"hhea")))?
                .copy_from_slice(&(max_advance as u16).to_be_bytes());
            for (offset, value) in [(12, min_leading), (14, min_trailing), (16, max_extent)] {
                put(header, offset, value)?;
            }
            header
                .get_mut(34..36)
                .ok_or_else(|| error(Tag::new(b"hhea")))?
                .copy_from_slice(&(count as u16).to_be_bytes());
        }
        if vertical {
            if let Some(vorg) = &vorg {
                let default = vorg.default_vert_origin_y() as i32;
                let records: Vec<_> = origins.into_iter().filter(|(_, o)| *o != default).collect();
                let mut out = vec![0, 1, 0, 0];
                out.extend((default as i16).to_be_bytes());
                out.extend((records.len() as u16).to_be_bytes());
                for (gid, origin) in records {
                    out.extend((gid.to_u32() as u16).to_be_bytes());
                    out.extend((origin.clamp(-32768, 32767) as i16).to_be_bytes());
                }
                tables.insert(Tag::new(b"VORG"), out);
            }
        }
    }
    apply_mvar(font, axes, tables)?;
    if !axes.all_pinned() {
        use write_fonts::{
            from_obj::ToOwnedTable,
            tables::{hvar::Hvar, mvar::Mvar, vvar::Vvar},
        };
        if let Some(table) = &hvar {
            let store = table
                .item_variation_store()
                .map_err(|_| error(Tag::new(b"HVAR")))?;
            let mut table: Hvar = table.to_owned_table();
            table.item_variation_store = StorePlan::new(&store, axes)?.rebuild(&store)?.into();
            tables.insert(
                Tag::new(b"HVAR"),
                write_fonts::dump_table(&table).map_err(|_| error(Tag::new(b"HVAR")))?,
            );
        }
        if let Some(table) = &vvar {
            let store = table
                .item_variation_store()
                .map_err(|_| error(Tag::new(b"VVAR")))?;
            let mut table: Vvar = table.to_owned_table();
            table.item_variation_store = StorePlan::new(&store, axes)?.rebuild(&store)?.into();
            tables.insert(
                Tag::new(b"VVAR"),
                write_fonts::dump_table(&table).map_err(|_| error(Tag::new(b"VVAR")))?,
            );
        }
        if let Ok(table) = font.mvar() {
            let store = table
                .item_variation_store()
                .transpose()
                .map_err(|_| error(Tag::new(b"MVAR")))?;
            let mut table: Mvar = table.to_owned_table();
            table.item_variation_store = store
                .as_ref()
                .map(|s| StorePlan::new(s, axes)?.rebuild(s))
                .transpose()?
                .map(Into::into)
                .unwrap_or_default();
            tables.insert(
                Tag::new(b"MVAR"),
                write_fonts::dump_table(&table).map_err(|_| error(Tag::new(b"MVAR")))?,
            );
        }
    }
    update_os2(axes, tables);
    Ok(())
}

pub(super) fn update_os2(axes: &AxisPlan, tables: &mut Tables) {
    if let Some(os2) = tables.get_mut(&Tag::new(b"OS/2")) {
        for &(tag, value) in &axes.values {
            let (offset, value) = if tag == Tag::new(b"wght") {
                (4, value.clamp(1., 1000.).round() as u16)
            } else if tag == Tag::new(b"wdth") {
                let widths = [50., 62.5, 75., 87.5, 100., 112.5, 125., 150., 200.];
                let value = value.clamp(50., 200.);
                let i = widths.partition_point(|w| *w < value).min(8);
                let class = if i == 0 {
                    1.
                } else {
                    i as f32 + (value - widths[i - 1]) / (widths[i] - widths[i - 1])
                };
                (6, class.round() as u16)
            } else {
                continue;
            };
            if let Some(bytes) = os2.get_mut(offset..offset + 2) {
                bytes.copy_from_slice(&value.to_be_bytes());
            }
        }
    }
}

fn apply_mvar(font: &FontRef, axes: &AxisPlan, tables: &mut Tables) -> Result<(), SubsetError> {
    let Some(store) = mvar_store(font, &axes.coords) else {
        return Ok(());
    };
    let sync = [(b"hasc", 4, 68), (b"hdsc", 6, 70), (b"hlgp", 8, 72)].map(|(tag, h, os)| {
        let same = tables
            .get(&Tag::new(b"hhea"))
            .and_then(|d| signed(d, h))
            .zip(tables.get(&Tag::new(b"OS/2")).and_then(|d| signed(d, os)))
            .is_some_and(|(a, b)| a == b);
        (*tag, h, same)
    });
    for (metric, table, offset, unsigned) in [
        (b"hasc", b"OS/2", 68, false),
        (b"hdsc", b"OS/2", 70, false),
        (b"hlgp", b"OS/2", 72, false),
        (b"hcla", b"OS/2", 74, true),
        (b"hcld", b"OS/2", 76, true),
        (b"vasc", b"vhea", 4, false),
        (b"vdsc", b"vhea", 6, false),
        (b"vlgp", b"vhea", 8, false),
        (b"hcrs", b"hhea", 18, false),
        (b"hcrn", b"hhea", 20, false),
        (b"hcof", b"hhea", 22, false),
        (b"vcrs", b"vhea", 18, false),
        (b"vcrn", b"vhea", 20, false),
        (b"vcof", b"vhea", 22, false),
        (b"xhgt", b"OS/2", 86, false),
        (b"cpht", b"OS/2", 88, false),
        (b"sbxs", b"OS/2", 10, false),
        (b"sbys", b"OS/2", 12, false),
        (b"sbxo", b"OS/2", 14, false),
        (b"sbyo", b"OS/2", 16, false),
        (b"spxs", b"OS/2", 18, false),
        (b"spys", b"OS/2", 20, false),
        (b"spxo", b"OS/2", 22, false),
        (b"spyo", b"OS/2", 24, false),
        (b"strs", b"OS/2", 26, false),
        (b"stro", b"OS/2", 28, false),
        (b"undo", b"post", 8, false),
        (b"unds", b"post", 10, false),
    ] {
        let Some(delta) = mvar_delta(font, &store, Tag::new(metric)) else {
            continue;
        };
        let delta = rounded(delta);
        if let Some(data) = tables.get_mut(&Tag::new(table)) {
            if let Some(old) = signed(data, offset) {
                let old = if unsigned {
                    old as u16 as i32
                } else {
                    old as i32
                };
                let value = old.saturating_add(delta);
                if unsigned {
                    data[offset..offset + 2]
                        .copy_from_slice(&(value.clamp(0, 65535) as u16).to_be_bytes());
                } else {
                    put(data, offset, value)?;
                }
            }
        }
    }
    for (metric, offset, same) in sync {
        if same {
            if let Some(delta) = mvar_delta(font, &store, Tag::new(&metric)) {
                if let Some(data) = tables.get_mut(&Tag::new(b"hhea")) {
                    let old = signed(data, offset).ok_or_else(|| error(Tag::new(b"hhea")))?;
                    put(data, offset, (old as i32).saturating_add(rounded(delta)))?;
                }
            }
        }
    }
    if let Some(data) = tables.get_mut(&Tag::new(b"gasp")) {
        for i in 0..10 {
            let tag = [b'g', b's', b'p', b'0' + i];
            let offset = 4 + i as usize * 4;
            if let Some(delta) = mvar_delta(font, &store, Tag::new(&tag)) {
                if let Some(old) = signed(data, offset) {
                    data[offset..offset + 2].copy_from_slice(
                        &((old as u16 as i32)
                            .saturating_add(rounded(delta))
                            .clamp(0, 65535) as u16)
                            .to_be_bytes(),
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::{
        tables::{
            hvar::Hvar,
            mvar::{Mvar, ValueRecord},
            variations::{ItemVariationStore, *},
        },
        types::{F2Dot14, MajorMinor},
        FontBuilder,
    };

    #[test]
    fn fractional_metrics_match_harfbuzz_float_evaluation() {
        // hb-subset FONT --gids=* --notdef-outline --variations=REQUEST
        // These locations crossed a rounding boundary with 16.16 scalars.
        for (file, request, gid, advance, bearing) in [
            (
                "32bit_var_store.otf",
                "wght=330.096435546875,CNTR=9.759521484375",
                1,
                39790,
                50,
            ),
            (
                "AdobeVFPrototype.otf",
                "wght=502.618408203125,CNTR=69.097900390625",
                2,
                304,
                80,
            ),
            (
                "AdobeVFPrototype.otf",
                "wght=226.66015625,CNTR=30.206298828125",
                62,
                301,
                50,
            ),
            (
                "AdobeVFPrototype.otf",
                "wght=255.755615234375,CNTR=35.07080078125",
                12,
                530,
                38,
            ),
            ("Cantarell-VF-ABC.otf", "wght=726.043701171875", 1, 667, -2),
        ] {
            let bytes = std::fs::read(format!("test-data/fonts/{file}")).unwrap();
            let font = FontRef::new(&bytes).unwrap();
            let output =
                crate::instance_font(&font, &crate::parse_axis_limits(request).unwrap()).unwrap();
            let output = FontRef::new(&output).unwrap();
            assert_eq!(
                output.hmtx().unwrap().advance(GlyphId::new(gid)),
                Some(advance),
                "{file} {request}"
            );
            assert_eq!(
                output.hmtx().unwrap().side_bearing(GlyphId::new(gid)),
                Some(bearing),
                "{file} {request}"
            );
        }
    }

    #[test]
    fn cff_vertical_metrics_without_vorg_match_harfbuzz() {
        let bytes = std::fs::read("test-data/fonts/NotoSansJP-VF.subset.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let mut builder = FontBuilder::new();
        for record in font.table_directory().table_records() {
            if record.tag() != Tag::new(b"VORG") {
                builder.add_raw(record.tag(), font.data_for_tag(record.tag()).unwrap());
            }
        }
        let bytes = builder.build();
        let font = FontRef::new(&bytes).unwrap();
        let bytes =
            crate::instance_font(&font, &crate::parse_axis_limits("wght=500").unwrap()).unwrap();
        let full = FontRef::new(&bytes).unwrap();
        // HarfBuzz --gids=* --notdef-outline --variations=wght=500 after
        // removing only VORG from the source. Empty notdef retains its bearing.
        for (gid, expected) in [0, 262, 262, 258, 261, 259].into_iter().enumerate() {
            assert_eq!(
                full.vmtx().unwrap().side_bearing(GlyphId::new(gid as u32)),
                Some(expected)
            );
        }
    }

    #[test]
    fn style_metadata_uses_the_effective_clamped_pin() {
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        for (request, expected) in [("wght=10000", 900), ("wght=-10000", 200)] {
            let bytes =
                crate::instance_font(&font, &crate::parse_axis_limits(request).unwrap()).unwrap();
            let output = FontRef::new(&bytes).unwrap();
            assert_eq!(output.os2().unwrap().us_weight_class(), expected);
        }
    }

    #[test]
    fn large_metric_deltas_saturate_without_overflow() {
        let bytes = std::fs::read("test-data/fonts/AdobeVFPrototype.otf").unwrap();
        let font = FontRef::new(&bytes).unwrap();
        let count = font.maxp().unwrap().num_glyphs();
        let store = |delta: i32| ItemVariationStore {
            variation_region_list: VariationRegionList::new(
                2,
                vec![VariationRegion::new(vec![
                    RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE),
                    RegionAxisCoordinates::new(F2Dot14::ZERO, F2Dot14::ZERO, F2Dot14::ZERO),
                ])],
            )
            .into(),
            item_variation_data: vec![ItemVariationData {
                item_count: count,
                word_delta_count: 0x8001,
                region_indexes: vec![0],
                delta_sets: std::iter::repeat_n(delta, count as usize)
                    .flat_map(i32::to_be_bytes)
                    .collect(),
            }
            .into()],
        };
        for (delta, advance, average, ascent) in [
            (i32::MAX, u16::MAX, i16::MAX, i16::MAX),
            (i32::MIN, 0, 0, i16::MIN),
        ] {
            let mut builder = FontBuilder::new();
            for r in font.table_directory().table_records() {
                builder.add_raw(r.tag(), font.data_for_tag(r.tag()).unwrap());
            }
            builder
                .add_table(&Hvar::new(store(delta), None, None, None))
                .unwrap();
            builder
                .add_table(&Mvar {
                    version: MajorMinor::VERSION_1_0,
                    value_record_size: 8,
                    value_record_count: 1,
                    item_variation_store: store(delta).into(),
                    value_records: vec![ValueRecord::new(Tag::new(b"hasc"), 0, 0)],
                })
                .unwrap();
            let bytes = builder.build();
            let font = FontRef::new(&bytes).unwrap();
            let bytes = crate::instance_font(
                &font,
                &crate::parse_axis_limits("wght=900,CNTR=drop").unwrap(),
            )
            .unwrap();
            let full = FontRef::new(&bytes).unwrap();
            for gid in 0..count {
                assert_eq!(
                    full.hmtx().unwrap().advance(GlyphId::new(gid as u32)),
                    Some(advance)
                );
            }
            assert_eq!(full.os2().unwrap().x_avg_char_width(), average);
            assert_eq!(full.os2().unwrap().s_typo_ascender(), ascent);
        }
    }
}
