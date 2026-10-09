//! Update CFF2 metrics at the instance's new default, including empty glyphs.
use super::{AxisPlan, StorePlan};
use crate::SubsetError;
use std::collections::BTreeMap;
use write_fonts::{
    read::{ps::cff::CffFontRef, FontRef, TableProvider},
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
    value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

pub(super) fn instance(
    font: &FontRef,
    axes: &AxisPlan,
    tables: &mut Tables,
) -> Result<(), SubsetError> {
    let tag = Tag::new(b"CFF2");
    let data = font.data_for_tag(tag).ok_or_else(|| error(tag))?;
    let cff = CffFontRef::new(
        data.as_bytes(),
        0,
        font.head().ok().map(|h| h.units_per_em() as i32),
    )
    .map_err(|_| error(tag))?;
    let count = font
        .maxp()
        .map_err(|_| error(Tag::new(b"maxp")))?
        .num_glyphs() as usize;
    let hmtx = font.hmtx().ok();
    let vmtx = font.vmtx().ok();
    let hvar = font.hvar().ok();
    let vvar = font.vvar().ok();
    let vorg = font.vorg().ok();
    let mut bounds = Vec::new();
    let mut union: Option<[i32; 4]> = None;
    for gid in 0..count {
        let gid = GlyphId::new(gid as u32);
        let bound = if gid.to_u32() < cff.num_glyphs() {
            let sf = cff
                .subfont(
                    cff.subfont_index(gid).ok_or_else(|| error(tag))?,
                    &axes.coords,
                )
                .map_err(|_| error(tag))?;
            let mut bounds = write_fonts::read::ps::cs::ControlBoundsSink::new();
            let mut sink = write_fonts::read::ps::cs::NopFilterSink::new(&mut bounds);
            cff.evaluate_charstring(&sf, gid, &axes.coords, &mut sink)
                .map_err(|_| error(tag))?;
            bounds.bounding_box().map(|b| {
                [
                    rounded(b.x_min.to_f64()),
                    rounded(b.y_min.to_f64()),
                    rounded(b.x_max.to_f64()),
                    rounded(b.y_max.to_f64()),
                ]
            })
        } else {
            None
        };
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
                vvar.as_ref()
                    .and_then(|v| v.advance_delta(gid, &axes.coords))
            } else {
                hvar.as_ref()
                    .and_then(|v| v.advance_delta(gid, &axes.coords))
            }
            .map_or(0, |d| rounded(d.to_f64()));
            let advance = base.saturating_add(delta).clamp(0, 65535);
            let old_bearing = if vertical {
                vmtx.as_ref().and_then(|m| m.side_bearing(gid))
            } else {
                hmtx.as_ref().and_then(|m| m.side_bearing(gid))
            }
            .unwrap_or(0) as i32;
            let origin = if vertical {
                let origin = vorg.as_ref().map_or_else(
                    || font.hhea().ok().map_or(0, |h| h.ascender().to_i16() as i32),
                    |v| v.vertical_origin_y(gid) as i32,
                ) + vvar
                    .as_ref()
                    .and_then(|v| v.v_origin_y_delta(gid, &axes.coords))
                    .map_or(0, |d| rounded(d.to_f64()));
                origins.push((gid, origin));
                origin
            } else {
                0
            };
            let leading = if let Some(b) = bound {
                if vertical {
                    origin - b[3]
                } else {
                    b[0]
                }
            } else {
                old_bearing
            };
            let extent = bound.map_or(0, |b| if vertical { b[3] - b[1] } else { b[2] - b[0] });
            out.extend((advance as u16).to_be_bytes());
            out.extend((leading.clamp(-32768, 32767) as i16).to_be_bytes());
            max_advance = max_advance.max(advance);
            min_leading = min_leading.min(leading);
            min_trailing = min_trailing.min(advance - leading - extent);
            max_extent = max_extent.max(leading + extent);
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
    let Ok(mvar) = font.mvar() else {
        return Ok(());
    };
    let Some(instance) = mvar.at(&axes.coords) else {
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
        let Some(delta) = instance.get(Tag::new(metric)) else {
            continue;
        };
        let delta = rounded(delta.to_f64());
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
            if let Some(delta) = instance.get(Tag::new(&metric)) {
                if let Some(data) = tables.get_mut(&Tag::new(b"hhea")) {
                    let old = signed(data, offset).ok_or_else(|| error(Tag::new(b"hhea")))?;
                    put(data, offset, old as i32 + rounded(delta.to_f64()))?;
                }
            }
        }
    }
    if let Some(data) = tables.get_mut(&Tag::new(b"gasp")) {
        for i in 0..10 {
            let tag = [b'g', b's', b'p', b'0' + i];
            let offset = 4 + i as usize * 4;
            if let Some(delta) = instance.get(Tag::new(&tag)) {
                if let Some(old) = signed(data, offset) {
                    data[offset..offset + 2].copy_from_slice(
                        &((old as u16 as i32 + rounded(delta.to_f64())).clamp(0, 65535) as u16)
                            .to_be_bytes(),
                    );
                }
            }
        }
    }
    Ok(())
}
