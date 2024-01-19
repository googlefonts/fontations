//! Fixed point math helpers that are specific to TrueType hinting.
//!
//! These are reimplemented in terms of font-types types when possible.

use read_fonts::types::{Fixed, Point};

pub fn floor(x: i32) -> i32 {
    x & !63
}

pub fn round(x: i32) -> i32 {
    floor(x + 32)
}

pub fn ceil(x: i32) -> i32 {
    floor(x + 63)
}

fn floor_pad(x: i32, n: i32) -> i32 {
    x & !(n - 1)
}

pub fn round_pad(x: i32, n: i32) -> i32 {
    floor_pad(x + n / 2, n)
}

#[inline(always)]
pub fn mul(a: i32, b: i32) -> i32 {
    (Fixed::from_bits(a) * Fixed::from_bits(b)).to_bits()
}

pub fn div(a: i32, b: i32) -> i32 {
    (Fixed::from_bits(a) / Fixed::from_bits(b)).to_bits()
}

/// Fixed point multiply and divide: a * b / c
pub fn mul_div(a: i32, b: i32, c: i32) -> i32 {
    Fixed::from_bits(a)
        .mul_div(Fixed::from_bits(b), Fixed::from_bits(c))
        .to_bits()
}

/// Fixed point multiply and divide without rounding: a * b / c
///
/// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/base/ftcalc.c#L200>
pub fn mul_div_no_round(mut a: i32, mut b: i32, mut c: i32) -> i32 {
    let mut s = 1;
    if a < 0 {
        a = -a;
        s = -1;
    }
    if b < 0 {
        b = -b;
        s = -s;
    }
    if c < 0 {
        c = -c;
        s = -s;
    }
    let d = if c > 0 {
        ((a as i64) * (b as i64)) / c as i64
    } else {
        0x7FFFFFFF
    };
    if s < 0 {
        -(d as i32)
    } else {
        d as i32
    }
}

/// Multiplication for 2.14 fixed point.
///
/// Based on <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/truetype/ttinterp.c#L1234>
pub fn mul14(a: i32, b: i32) -> i32 {
    let mut v = a as i64 * b as i64;
    v += 0x2000 + (v >> 63);
    (v >> 14) as i32
}

/// Normalize a vector in 2.14 fixed point.
///
/// Direct port of <https://gitlab.freedesktop.org/freetype/freetype/-/blob/57617782464411201ce7bbc93b086c1b4d7d84a5/src/base/ftcalc.c#L800>
pub fn normalize14(x: i32, y: i32) -> Point<i32> {
    use core::num::Wrapping;
    let (mut sx, mut sy) = (Wrapping(1i32), Wrapping(1i32));
    let mut ux = Wrapping(x as u32);
    let mut uy = Wrapping(y as u32);
    const ZERO: Wrapping<u32> = Wrapping(0);
    let mut result = Point::default();
    if x < 0 {
        ux = ZERO - ux;
        sx = -sx;
    }
    if y < 0 {
        uy = ZERO - uy;
        sy = -sy;
    }
    if ux == ZERO {
        result.x = x / 4;
        if uy.0 > 0 {
            result.y = (sy * Wrapping(0x10000) / Wrapping(4)).0;
        }
        return result;
    }
    if uy == ZERO {
        result.y = y / 4;
        if ux.0 > 0 {
            result.x = (sx * Wrapping(0x10000) / Wrapping(4)).0;
        }
        return result;
    }
    let mut len = if ux > uy {
        ux + (uy >> 1)
    } else {
        uy + (ux >> 1)
    };
    let mut shift = Wrapping(len.0.leading_zeros() as i32);
    shift -= Wrapping(15)
        + if len >= (Wrapping(0xAAAAAAAAu32) >> shift.0 as usize) {
            Wrapping(1)
        } else {
            Wrapping(0)
        };
    if shift.0 > 0 {
        let s = shift.0 as usize;
        ux <<= s;
        uy <<= s;
        len = if ux > uy {
            ux + (uy >> 1)
        } else {
            uy + (ux >> 1)
        };
    } else {
        let s = -shift.0 as usize;
        ux >>= s;
        uy >>= s;
        len >>= s;
    }
    let mut b = Wrapping(0x10000) - Wrapping(len.0 as i32);
    let x = Wrapping(ux.0 as i32);
    let y = Wrapping(uy.0 as i32);
    let mut z;
    let mut u;
    let mut v;
    loop {
        u = Wrapping((x + ((x * b) >> 16)).0 as u32);
        v = Wrapping((y + ((y * b) >> 16)).0 as u32);
        z = Wrapping(-((u * u + v * v).0 as i32)) / Wrapping(0x200);
        z = z * ((Wrapping(0x10000) + b) >> 8) / Wrapping(0x10000);
        b += z;
        if z <= Wrapping(0) {
            break;
        }
    }
    Point::new(
        (Wrapping(u.0 as i32) * sx / Wrapping(4)).0,
        (Wrapping(v.0 as i32) * sy / Wrapping(4)).0,
    )
}
