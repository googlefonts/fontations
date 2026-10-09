//! Continuous CFF2 control bounds with float scalars and double coordinates.
use super::{
    charstring::{BlendScalars, CharStringActions, Command, Interpreter, Program, Value},
    instance::gains,
    source::{Cff2, Source},
    Error, Result,
};
use crate::instance::AxisPlan;
use write_fonts::{
    read::{FontRef, TableProvider},
    types::{GlyphId, Tag},
};

struct ContinuousScalars(Vec<Vec<f64>>);
impl BlendScalars for ContinuousScalars {
    fn scalars(&self, ivs: usize) -> Result<&[f64]> {
        self.0.as_slice().scalars(ivs)
    }
    fn finish_blend(&self, value: f64) -> f64 {
        value
    }
}

struct Bounds<'a> {
    scalars: &'a ContinuousScalars,
    point: [f64; 2],
    extent: Option<[f64; 4]>,
    open: bool,
    remaining: usize,
    result: Result<()>,
}
impl Bounds<'_> {
    fn include(&mut self, point: [f64; 2]) {
        self.extent = Some(if let Some(b) = self.extent {
            [
                b[0].min(point[0]),
                b[1].min(point[1]),
                b[2].max(point[0]),
                b[3].max(point[1]),
            ]
        } else {
            [point[0], point[1], point[0], point[1]]
        });
    }
    fn start(&mut self) {
        if !self.open {
            self.include(self.point);
            self.open = true;
        }
    }
    fn line(&mut self, dx: f64, dy: f64) {
        self.start();
        self.point[0] += dx;
        self.point[1] += dy;
        self.include(self.point);
    }
    fn curve(&mut self, deltas: &[f64]) {
        self.start();
        for pair in deltas.chunks_exact(2) {
            self.point[0] += pair[0];
            self.point[1] += pair[1];
            self.include(self.point);
        }
    }
    fn path(&mut self, op: u16, a: &[f64]) -> Result<()> {
        let n = a.len();
        match op {
            4 | 21 | 22 => {
                let (dx, dy) = match (op, a) {
                    (4, [dy]) => (0., *dy),
                    (22, [dx]) => (*dx, 0.),
                    (21, [dx, dy]) => (*dx, *dy),
                    _ => return Err(Error),
                };
                self.point[0] += dx;
                self.point[1] += dy;
                self.open = false;
            }
            5 if n >= 2 && n % 2 == 0 => {
                for pair in a.chunks_exact(2) {
                    self.line(pair[0], pair[1]);
                }
            }
            6 | 7 if n > 0 => {
                for (i, &delta) in a.iter().enumerate() {
                    if (op == 6) == (i % 2 == 0) {
                        self.line(delta, 0.);
                    } else {
                        self.line(0., delta);
                    }
                }
            }
            8 if n >= 6 && n % 6 == 0 => {
                for curve in a.chunks_exact(6) {
                    self.curve(curve);
                }
            }
            24 if n >= 8 && (n - 2) % 6 == 0 => {
                for curve in a[..n - 2].chunks_exact(6) {
                    self.curve(curve);
                }
                self.line(a[n - 2], a[n - 1]);
            }
            25 if n >= 8 && (n - 6) % 2 == 0 => {
                for line in a[..n - 6].chunks_exact(2) {
                    self.line(line[0], line[1]);
                }
                self.curve(&a[n - 6..]);
            }
            26 | 27 if n >= 4 && n % 4 <= 1 => {
                let extra = if n % 4 == 1 { a[0] } else { 0. };
                for (i, c) in a[n % 4..].chunks_exact(4).enumerate() {
                    let extra = if i == 0 { extra } else { 0. };
                    if op == 26 {
                        self.curve(&[extra, c[0], c[1], c[2], 0., c[3]]);
                    } else {
                        self.curve(&[c[0], extra, c[1], c[2], c[3], 0.]);
                    }
                }
            }
            30 | 31 if n >= 4 && n % 4 <= 1 => {
                let count = n / 4;
                for (i, c) in a[..count * 4].chunks_exact(4).enumerate() {
                    let extra = if i + 1 == count && n % 4 == 1 {
                        a[n - 1]
                    } else {
                        0.
                    };
                    if (op == 31) == (i % 2 == 0) {
                        self.curve(&[c[0], 0., c[1], c[2], extra, c[3]]);
                    } else {
                        self.curve(&[0., c[0], c[1], c[2], c[3], extra]);
                    }
                }
            }
            0x122 if n == 7 => {
                self.curve(&[a[0], 0., a[1], a[2], a[3], 0.]);
                self.curve(&[a[4], 0., a[5], -a[2], a[6], 0.]);
            }
            0x123 if n == 13 => {
                self.curve(&a[..6]);
                self.curve(&a[6..12]);
            }
            0x124 if n == 9 => {
                self.curve(&[a[0], a[1], a[2], a[3], a[4], 0.]);
                self.curve(&[a[5], 0., a[6], a[7], a[8], -a[1] - a[3] - a[7]]);
            }
            0x125 if n == 11 => {
                let (dx, dy) = a[..10]
                    .chunks_exact(2)
                    .fold((0., 0.), |(dx, dy), p| (dx + p[0], dy + p[1]));
                self.curve(&a[..6]);
                if dx.abs() > dy.abs() {
                    self.curve(&[a[6], a[7], a[8], a[9], a[10], -dy]);
                } else {
                    self.curve(&[a[6], a[7], a[8], a[9], -dx, a[10]]);
                }
            }
            1 | 3 | 18 | 19 | 20 | 23 | 0x100 => {}
            _ => return Err(Error),
        }
        if self.point.iter().any(|v| !v.is_finite()) {
            return Err(Error);
        }
        Ok(())
    }
}
impl CharStringActions for Bounds<'_> {
    fn token(&mut self, _: Program, _: usize, _: &[u8]) {}
    fn call(&mut self, _: Program, _: usize, _: Program, _: &Value) -> Result<()> {
        Ok(())
    }
    fn discard(&mut self, _: impl Iterator<Item = (Program, usize)>) {}
    fn command(&mut self, command: Command) {
        if self.result.is_err() {
            return;
        }
        self.result = (|| {
            let args = command
                .args
                .iter()
                .map(|v| v.resolve(self.scalars, &mut self.remaining))
                .collect::<Result<Vec<_>>>()?;
            self.path(command.op, &args)
        })();
    }
}

pub(super) fn bounds(font: &FontRef, axes: &AxisPlan) -> Result<Vec<Option<[f64; 4]>>> {
    let data = font.data_for_tag(Tag::new(b"CFF2")).ok_or(Error)?;
    let source = Source::new(data.as_bytes())?;
    let scalars = ContinuousScalars(if let Some(store) = source.font.var_store() {
        (0..store.item_variation_data_count() as usize)
            .map(|i| gains(store, i, &axes.metric_coords))
            .collect::<Result<_>>()?
    } else {
        vec![]
    });
    (0..font.maxp().map_err(|_| Error)?.num_glyphs() as usize)
        .map(|gid| {
            if gid >= source.font.num_glyphs() as usize {
                return Ok(None);
            }
            let fd = source.fd(GlyphId::new(gid as u32))?;
            let mut bounds = Bounds {
                scalars: &scalars,
                point: [0.; 2],
                extent: None,
                open: false,
                remaining: 200_000,
                result: Ok(()),
            };
            Interpreter::<Cff2, _, _>::new(&source, &mut bounds, fd, source.fds[fd].ivs, false)
                .run(
                    Program::Glyph(gid),
                    source.font.charstrings().get(gid).ok_or(Error)?,
                )?;
            bounds.result?;
            Ok(bounds.extent)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cff::charstring::flatten;
    use write_fonts::read::ps::{cff::index::Index, cs};

    #[test]
    fn zero_extents_preserve_the_original_bearing() {
        use crate::{Plan, SubsetFlags};
        use write_fonts::{read::collections::IntSet, types::NameId, FontBuilder};
        let font = FontRef::new(include_bytes!(
            "../../test-data/fonts/cff2-nested-blends.otf"
        ))
        .unwrap();
        let source = Source::new(font.cff2().unwrap().offset_data().as_bytes()).unwrap();
        let plan = Plan::new(
            &IntSet::all(),
            &IntSet::empty(),
            &font,
            SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
            &IntSet::empty(),
            &IntSet::all(),
            &IntSet::all(),
            &IntSet::<NameId>::all(),
            &IntSet::all(),
        );
        let mut chars: Vec<_> = (0..source.font.num_glyphs() as usize)
            .map(|gid| source.font.charstrings().get(gid).unwrap().to_vec())
            .collect();
        // A drawing operator with zero extents, rather than an empty program.
        chars[0] = vec![139, 139, 21, 139, 139, 5];
        let cff = crate::cff::subset::assemble::<Cff2>(
            &source,
            &plan,
            crate::cff::subset::Programs {
                chars,
                globals: vec![],
                locals: vec![vec![]; source.fds.len()],
            },
        )
        .unwrap();
        let mut metrics = font
            .data_for_tag(Tag::new(b"hmtx"))
            .unwrap()
            .as_bytes()
            .to_vec();
        metrics[2..4].copy_from_slice(&50i16.to_be_bytes());
        let mut builder = FontBuilder::new();
        builder
            .add_raw(Tag::new(b"CFF2"), cff)
            .add_raw(Tag::new(b"hmtx"), metrics);
        let input = builder.copy_missing_tables(font).build();
        let output = crate::instance_font(
            &FontRef::new(&input).unwrap(),
            &crate::parse_axis_limits("wght=0.5,wdth=0.5").unwrap(),
        )
        .unwrap();
        assert_eq!(
            FontRef::new(&output)
                .unwrap()
                .hmtx()
                .unwrap()
                .side_bearing(GlyphId::new(0)),
            Some(50)
        );
    }

    #[test]
    fn control_bounds_cover_every_type2_path_operator() {
        let scalars = ContinuousScalars(vec![]);
        let empty = Index::Empty;
        let context = (&[][..], &empty, &empty, &empty);
        for (op, counts) in [
            (5, vec![2, 6]),
            (6, vec![1, 4]),
            (7, vec![1, 4]),
            (8, vec![6, 12]),
            (24, vec![8, 14]),
            (25, vec![8, 12]),
            (26, vec![4, 5, 8, 9]),
            (27, vec![4, 5, 8, 9]),
            (30, vec![4, 5, 8, 9, 12, 13]),
            (31, vec![4, 5, 8, 9, 12, 13]),
            (0x122, vec![7]),
            (0x123, vec![13]),
            (0x124, vec![9]),
            (0x125, vec![11]),
        ] {
            for n in counts {
                for swap in [false, true] {
                    let args: Vec<_> = (0..n)
                        .map(|i| {
                            if (i % 2 == 0) == swap {
                                (i + 1) as f64
                            } else {
                                -((i + 1) as f64)
                            }
                        })
                        .collect();
                    let mut bounds = Bounds {
                        scalars: &scalars,
                        point: [0.; 2],
                        extent: None,
                        open: false,
                        remaining: 200_000,
                        result: Ok(()),
                    };
                    bounds.path(op, &args).unwrap();
                    let commands = [
                        Command {
                            op: 21,
                            args: vec![Value::plain(0.); 2],
                            mask: vec![],
                        },
                        Command {
                            op,
                            args: args.iter().copied().map(Value::plain).collect(),
                            mask: vec![],
                        },
                    ];
                    let bytes = flatten(&commands, None, true, 0).unwrap();
                    let mut reference = cs::ControlBoundsSink::new();
                    cs::evaluate(&context, None, &bytes, &mut reference).unwrap();
                    let b = reference.bounding_box().unwrap();
                    assert_eq!(
                        bounds.extent,
                        Some([
                            b.x_min.to_f64(),
                            b.y_min.to_f64(),
                            b.x_max.to_f64(),
                            b.y_max.to_f64()
                        ]),
                        "op={op} n={n} swap={swap}"
                    );
                }
            }
        }
    }
}
