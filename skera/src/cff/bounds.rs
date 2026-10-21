//! Continuous CFF2 control bounds with float scalars and double coordinates.
use super::{
    charstring::{CharStringActions, Interpreter, Program, Value},
    instance::{gains, resolve},
    source::{Cff2, Source},
    Error, Result,
};
use crate::instance::AxisPlan;
use read_fonts::{
    types::{GlyphId, Tag},
    FontRef, TableProvider,
};

struct Bounds<'a> {
    scalars: &'a [Vec<f64>],
    args: Vec<f64>,
    point: [f64; 2],
    extent: Option<[f64; 4]>,
    open: bool,
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
impl CharStringActions<'_> for Bounds<'_> {
    fn command(&mut self, op: u16, args: &[Value], _mask: &[u8]) {
        if self.result.is_err() {
            return;
        }
        // Keep capacity between drawing commands; bounds need numeric values
        // but do not need to retain a command stream.
        let mut values = std::mem::take(&mut self.args);
        values.clear();
        self.result = (|| {
            for v in args {
                values.push(if let Some((ivs, deltas)) = &v.blend {
                    resolve(v.default, deltas, self.scalars.get(*ivs).ok_or(Error)?)?
                } else {
                    v.default
                });
            }
            self.path(op, &values)
        })();
        self.args = values;
    }
}

pub(super) fn bounds(font: &FontRef, axes: &AxisPlan) -> Result<Vec<Option<[f64; 4]>>> {
    let data = font.data_for_tag(Tag::new(b"CFF2")).ok_or(Error)?;
    let source = Source::new(data.as_bytes())?;
    let scalars = if let Some(store) = source.font.var_store() {
        (0..store.item_variation_data_count() as usize)
            .map(|i| gains(store, i, &axes.metric_coords))
            .collect::<Result<_>>()?
    } else {
        vec![]
    };
    (0..font.maxp().map_err(|_| Error)?.num_glyphs() as usize)
        .map(|gid| {
            if gid >= source.font.num_glyphs() as usize {
                return Ok(None);
            }
            let fd = source.fd(GlyphId::new(gid as u32))?;
            let mut bounds = Bounds {
                scalars: &scalars,
                args: Vec::new(),
                point: [0.; 2],
                extent: None,
                open: false,
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
