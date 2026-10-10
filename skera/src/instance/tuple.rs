//! HarfBuzz's tuple_variations_t pipeline: change axis limits, infer missing
//! deltas, merge equal tents, discard constants, and compile residual tuples.
use super::{
    iup,
    rebase::{self, Triple},
    AxisPlan,
};
use crate::SubsetError;
use std::collections::BTreeMap;
use write_fonts::{
    read::{
        tables::variations::{TupleDelta, TupleVariation},
        FontRef, TableProvider,
    },
    tables::variations::{PackedDeltas, PackedPointNumbers, Tuple, TupleVariationHeader},
    types::{F2Dot14, Tag},
};

type Point = [f32; 2];
fn error() -> SubsetError {
    SubsetError::SubsetTableError(Tag::new(b"gvar"))
}
fn rounded(value: f32) -> Result<i32, SubsetError> {
    let value = (value as f64 + 0.5).floor();
    if !value.is_finite() || value < i32::MIN as f64 || value > i32::MAX as f64 {
        return Err(error());
    }
    Ok(value as i32)
}

struct TupleDeltaData {
    tents: Vec<Option<Triple>>,
    deltas: Vec<Point>,
    referenced: Vec<bool>,
}
pub(super) struct TupleVariations {
    tuples: Vec<TupleDeltaData>,
}
impl TupleVariations {
    pub fn new() -> Self {
        Self { tuples: Vec::new() }
    }
    pub fn add<T: TupleDelta>(
        &mut self,
        tuple: &TupleVariation<T>,
        axis_count: usize,
        deltas: Vec<Point>,
        referenced: Vec<bool>,
    ) -> Result<(), SubsetError> {
        if tuple.peak().len() != axis_count {
            return Err(error());
        }
        let mut tents = Vec::with_capacity(axis_count);
        for i in 0..axis_count {
            let peak = tuple.peak().get(i).ok_or_else(error)?.to_f64();
            let start = tuple
                .intermediate_start()
                .and_then(|t| t.get(i))
                .map_or(peak.min(0.), |c| c.to_f64());
            let end = tuple
                .intermediate_end()
                .and_then(|t| t.get(i))
                .map_or(peak.max(0.), |c| c.to_f64());
            tents.push((peak != 0.).then_some(Triple(start, peak, end)));
        }
        self.tuples.push(TupleDeltaData {
            tents,
            deltas,
            referenced,
        });
        Ok(())
    }

    pub fn instantiate(
        &mut self,
        font: &FontRef,
        axes: &AxisPlan,
        points: Option<(&[Point], &[usize])>,
        default_points: Option<&mut [Point]>,
    ) -> Result<(), SubsetError> {
        let fvar = font.fvar().map_err(|_| error())?;
        let mut order: Vec<_> = fvar
            .axes()
            .map_err(|_| error())?
            .iter()
            .enumerate()
            .filter(|(i, a)| {
                axes.pinned[*i] || axes.values.iter().any(|(tag, _)| *tag == a.axis_tag())
            })
            .map(|(i, a)| (a.axis_tag(), i))
            .collect();
        order.sort_by_key(|a| a.0);
        // HarfBuzz scales deltas after each sorted axis, with float storage.
        for (_, axis) in order {
            let mut next = Vec::new();
            for tuple in std::mem::take(&mut self.tuples) {
                let Some(tent) = tuple.tents[axis] else {
                    next.push(tuple);
                    continue;
                };
                if tent.0 > tent.1 || tent.1 > tent.2 || tent.0 < 0. && tent.2 > 0. {
                    continue;
                }
                let solutions = rebase::rebase(tent, axes.normalized[axis], axes.distances[axis]);
                if next.len().saturating_add(solutions.len()) > 65535 {
                    return Err(error());
                }
                for (gain, tent) in solutions {
                    let gain = gain as f32;
                    let mut deltas = tuple.deltas.clone();
                    for (delta, &referenced) in deltas.iter_mut().zip(&tuple.referenced) {
                        if referenced {
                            delta[0] *= gain;
                            delta[1] *= gain;
                        }
                    }
                    let mut tents = tuple.tents.clone();
                    tents[axis] = tent;
                    next.push(TupleDeltaData {
                        tents,
                        deltas,
                        referenced: tuple.referenced.clone(),
                    });
                }
            }
            self.tuples = next;
        }
        if let Some((points, contours)) = points {
            for tuple in &mut self.tuples {
                tuple.calc_inferred_deltas(points, contours)?;
            }
        }
        self.merge_tuple_variations(default_points)?;
        // Keep only the output axis order; constant contributions have already
        // been applied to the outline or CVT by the default-location pass.
        for tuple in &mut self.tuples {
            tuple.tents = tuple
                .tents
                .iter()
                .zip(&axes.pinned)
                .filter_map(|(&t, &pin)| (!pin).then_some(t))
                .collect();
        }
        Ok(())
    }

    fn merge_tuple_variations(
        &mut self,
        mut default_points: Option<&mut [Point]>,
    ) -> Result<(), SubsetError> {
        let mut result: Vec<TupleDeltaData> = Vec::new();
        let mut map = BTreeMap::new();
        for tuple in std::mem::take(&mut self.tuples) {
            if tuple.tents.iter().all(Option::is_none) {
                // IUP uses the float outline after rebased constant tuples
                // have been applied, in tuple order, just as HB does.
                if let Some(points) = default_points.as_deref_mut() {
                    if points.len() != tuple.deltas.len() {
                        return Err(error());
                    }
                    for (point, delta) in points.iter_mut().zip(&tuple.deltas) {
                        point[0] += delta[0];
                        point[1] += delta[1];
                    }
                }
                continue;
            }
            let key: Vec<_> = tuple
                .tents
                .iter()
                .map(|t| {
                    let t = t.unwrap_or(Triple(0., 0., 0.));
                    let bits = |v: f64| if v == 0. { 0 } else { v.to_bits() };
                    (bits(t.0), bits(t.1), bits(t.2))
                })
                .collect();
            if let Some(&i) = map.get(&key) {
                let target: &mut TupleDeltaData = &mut result[i];
                if target.deltas.len() != tuple.deltas.len() {
                    return Err(error());
                }
                for (a, b) in target.deltas.iter_mut().zip(&tuple.deltas) {
                    a[0] += b[0];
                    a[1] += b[1];
                }
                for (a, b) in target.referenced.iter_mut().zip(tuple.referenced) {
                    *a |= b;
                }
            } else {
                map.insert(key, result.len());
                result.push(tuple);
            }
        }
        if result.len() > 4095 {
            return Err(error());
        }
        self.tuples = result;
        Ok(())
    }

    fn peaks(&self) -> impl Iterator<Item = Vec<i16>> + '_ {
        self.tuples.iter().map(|t| t.coords().1)
    }

    pub fn optimize(
        &mut self,
        points: &[Point],
        contours: &[usize],
        is_composite: bool,
    ) -> Result<(), SubsetError> {
        for tuple in &mut self.tuples {
            let rounded = tuple
                .deltas
                .iter()
                .map(|d| Ok([rounded(d[0])?, rounded(d[1])?]))
                .collect::<Result<Vec<_>, SubsetError>>()?;
            let mut selected = iup::optimize(points, &rounded, contours).ok_or_else(error)?;
            if is_composite && selected.iter().all(|&v| !v) && !selected.is_empty() {
                selected[0] = true;
            }
            let count = selected.iter().filter(|&&v| v).count();
            // The packed count has 15 bits and packed point indices have 16.
            // Dense tuples can still represent larger point arrays.
            if count > 0x7fff || selected.iter().rposition(|&v| v).is_some_and(|i| i > 65535) {
                continue;
            }
            if count == 0 {
                tuple.referenced = selected;
                continue;
            }
            let size = |refs: &[bool]| -> Result<usize, SubsetError> {
                Ok(point_bytes(refs)?.len() + delta_bytes(&tuple.deltas, refs, 2)?.len())
            };
            // As in HB, keep an optimization only when the actual packed
            // point and delta streams are smaller than their dense encoding.
            if size(&selected)? < size(&tuple.referenced)? {
                tuple.referenced = selected;
            }
        }
        self.tuples.retain(|t| t.referenced.iter().any(|&v| v));
        Ok(())
    }

    pub fn compile_bytes(
        &self,
        shared: &BTreeMap<Vec<i16>, u16>,
        is_gvar: bool,
    ) -> Result<Vec<u8>, SubsetError> {
        if self.tuples.is_empty() {
            return Ok(Vec::new());
        }
        let mut headers = Vec::new();
        let mut point_sets: BTreeMap<Vec<bool>, (usize, Vec<u8>)> = BTreeMap::new();
        if is_gvar {
            for tuple in &self.tuples {
                let entry = point_sets
                    .entry(tuple.referenced.clone())
                    .or_insert((0, point_bytes(&tuple.referenced)?));
                entry.0 += 1;
            }
        }
        // Preserve the dense default encoding. For optimized sparse tuples,
        // use HB's point-set counts and packed-byte savings calculation.
        let dense = point_sets.keys().all(|refs| refs.iter().all(|&v| v));
        let shared_points = if is_gvar && (dense || point_sets.values().all(|(n, _)| *n > 1)) {
            point_sets
                .iter()
                .max_by_key(|(_, (n, bytes))| (n - 1) * bytes.len())
                .map(|(refs, (_, bytes))| (refs, bytes))
        } else {
            None
        };
        let mut data = shared_points.map_or_else(Vec::new, |(_, bytes)| bytes.clone());
        for tuple in &self.tuples {
            let (start, peak, end) = tuple.coords();
            let private = shared_points.is_none_or(|(refs, _)| *refs != tuple.referenced);
            let mut deltas = if is_gvar && private {
                point_sets
                    .get(&tuple.referenced)
                    .ok_or_else(error)?
                    .1
                    .clone()
            } else if !is_gvar {
                vec![0]
            } else {
                Vec::new()
            };
            deltas.extend(delta_bytes(
                &tuple.deltas,
                &tuple.referenced,
                if is_gvar { 2 } else { 1 },
            )?);
            let shared_index = shared.get(&peak).copied();
            let intermediate = start
                .iter()
                .zip(&peak)
                .zip(&end)
                .any(|((&s, &p), &e)| s != p.min(0) || e != p.max(0));
            let tup = |v: Vec<i16>| Tuple::new(v.into_iter().map(F2Dot14::from_bits).collect());
            let header = TupleVariationHeader::new(
                u16::try_from(deltas.len()).map_err(|_| error())?,
                shared_index,
                shared_index.is_none().then(|| tup(peak)),
                intermediate.then(|| (tup(start), tup(end))),
                private,
            );
            headers.extend(write_fonts::dump_table(&header).map_err(|_| error())?);
            data.extend(deltas);
        }
        let offset =
            u16::try_from(if is_gvar { 4 } else { 8 } + headers.len()).map_err(|_| error())?;
        let count = self.tuples.len() as u16 | if shared_points.is_some() { 0x8000 } else { 0 };
        let mut out = count.to_be_bytes().to_vec();
        out.extend(offset.to_be_bytes());
        out.extend(headers);
        out.extend(data);
        Ok(out)
    }
}

fn point_bytes(referenced: &[bool]) -> Result<Vec<u8>, SubsetError> {
    let points = if referenced.iter().all(|&v| v) {
        PackedPointNumbers::All
    } else {
        let points = referenced
            .iter()
            .enumerate()
            .filter(|(_, v)| **v)
            .map(|(i, _)| u16::try_from(i).map_err(|_| error()))
            .collect::<Result<Vec<_>, _>>()?;
        if points.is_empty() || points.len() > 0x7fff {
            return Err(error());
        }
        PackedPointNumbers::Some(points)
    };
    write_fonts::dump_table(&points).map_err(|_| error())
}

fn delta_bytes(deltas: &[Point], referenced: &[bool], axes: usize) -> Result<Vec<u8>, SubsetError> {
    let mut bytes = Vec::new();
    for axis in 0..axes {
        let values = deltas
            .iter()
            .zip(referenced)
            .filter(|(_, v)| axes == 1 || **v)
            .map(|(d, _)| rounded(d[axis]))
            .collect::<Result<Vec<_>, _>>()?;
        bytes.extend(write_fonts::dump_table(&PackedDeltas::new(values)).map_err(|_| error())?);
    }
    Ok(bytes)
}

impl TupleDeltaData {
    fn coords(&self) -> (Vec<i16>, Vec<i16>, Vec<i16>) {
        let encode = |v: f64| ((v as f32 * 16384. + 0.5).floor().clamp(-32768., 32767.)) as i16;
        let mut start = Vec::new();
        let mut peak = Vec::new();
        let mut end = Vec::new();
        for tent in &self.tents {
            let t = tent.unwrap_or(Triple(0., 0., 0.));
            start.push(encode(t.0));
            peak.push(encode(t.1));
            end.push(encode(t.2));
        }
        (start, peak, end)
    }

    fn calc_inferred_deltas(
        &mut self,
        points: &[Point],
        contours: &[usize],
    ) -> Result<(), SubsetError> {
        if self.deltas.len() != points.len() {
            return Err(error());
        }
        let mut start = 0;
        for &end in contours {
            if end < start || end >= points.len().saturating_sub(4) {
                return Err(error());
            }
            let refs: Vec<_> = (start..=end).filter(|&i| self.referenced[i]).collect();
            for (j, &a) in refs.iter().enumerate() {
                let b = refs[(j + 1) % refs.len()];
                let mut i = if a == end { start } else { a + 1 };
                while i != b {
                    for axis in 0..2 {
                        // The subsetter infers rebased deltas in double, then
                        // stores the result as float, unlike runtime gvar IUP.
                        self.deltas[i][axis] = infer_delta(
                            points[i][axis] as f64,
                            points[a][axis] as f64,
                            points[b][axis] as f64,
                            self.deltas[a][axis] as f64,
                            self.deltas[b][axis] as f64,
                        ) as f32;
                    }
                    i = if i == end { start } else { i + 1 };
                }
            }
            start = end + 1;
        }
        self.referenced.fill(true);
        Ok(())
    }
}

fn infer_delta(v: f64, a: f64, b: f64, da: f64, db: f64) -> f64 {
    if a == b {
        return if da == db { da } else { 0. };
    }
    if v <= a.min(b) {
        if a < b {
            da
        } else {
            db
        }
    } else if v >= a.max(b) {
        if a > b {
            da
        } else {
            db
        }
    } else {
        let r = (v - a) / (b - a);
        da + r * (db - da)
    }
}

pub(super) fn compile_gvar(
    glyphs: &[TupleVariations],
    axis_count: u16,
) -> Result<Vec<u8>, SubsetError> {
    let mut counts: BTreeMap<Vec<i16>, usize> = BTreeMap::new();
    for glyph in glyphs {
        for peak in glyph.peaks() {
            *counts.entry(peak).or_default() += 1;
        }
    }
    let mut peaks: Vec<_> = counts.into_iter().filter(|(_, n)| *n > 1).collect();
    peaks.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    peaks.truncate(4095);
    let shared: BTreeMap<_, _> = peaks
        .iter()
        .enumerate()
        .map(|(i, (p, _))| (p.clone(), i as u16))
        .collect();
    let mut data = Vec::new();
    let mut offsets = Vec::new();
    for glyph in glyphs {
        offsets.push(u32::try_from(data.len()).map_err(|_| error())?);
        data.extend(glyph.compile_bytes(&shared, true)?);
        if data.len() % 2 != 0 {
            data.push(0);
        }
    }
    offsets.push(u32::try_from(data.len()).map_err(|_| error())?);
    let short = data.len() <= 0x1fffe;
    let shared_offset = 20 + offsets.len() * if short { 2 } else { 4 };
    let data_offset = shared_offset + peaks.len() * axis_count as usize * 2;
    let mut out = vec![0, 1, 0, 0];
    out.extend(axis_count.to_be_bytes());
    out.extend((peaks.len() as u16).to_be_bytes());
    out.extend((shared_offset as u32).to_be_bytes());
    out.extend(
        u16::try_from(glyphs.len())
            .map_err(|_| error())?
            .to_be_bytes(),
    );
    out.extend(u16::from(!short).to_be_bytes());
    out.extend((data_offset as u32).to_be_bytes());
    for offset in offsets {
        if short {
            out.extend(((offset / 2) as u16).to_be_bytes());
        } else {
            out.extend(offset.to_be_bytes());
        }
    }
    for (peak, _) in peaks {
        for coord in peak {
            out.extend(coord.to_be_bytes());
        }
    }
    out.extend(data);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use write_fonts::read::{tables::gvar::Gvar, FontData, FontRead};
    use write_fonts::types::GlyphId;

    #[test]
    fn merged_tuples_round_once_and_preserve_long_deltas() {
        let make = || TupleDeltaData {
            tents: vec![Some(Triple(0., 1., 1.))],
            deltas: vec![[32767., -32768.], [2.4, -2.4]],
            referenced: vec![true; 2],
        };
        let mut tuples = TupleVariations {
            tuples: vec![make(), make()],
        };
        tuples.merge_tuple_variations(None).unwrap();
        let bytes = compile_gvar(&[tuples], 1).unwrap();
        let gvar = Gvar::read(FontData::new(&bytes)).unwrap();
        let data = gvar.glyph_variation_data(GlyphId::new(0)).unwrap().unwrap();
        let tuple = data.tuples().next().unwrap();
        let deltas = tuple
            .deltas()
            .map(|d| [d.x_delta, d.y_delta])
            .collect::<Vec<_>>();
        assert_eq!(deltas, [[65534, -65536], [5, -5]]);
    }

    #[test]
    fn tuple_header_overflow_returns_an_error() {
        let tuple = TupleDeltaData {
            tents: vec![Some(Triple(-1., -0.5, 0.)); 65535],
            deltas: vec![[0.; 2]; 4],
            referenced: vec![true; 4],
        };
        let tuples = TupleVariations {
            tuples: vec![tuple],
        };
        assert!(tuples.compile_bytes(&BTreeMap::new(), true).is_err());
    }
}
