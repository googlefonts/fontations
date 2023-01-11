//! Various fixed point math functions used in TrueType scaling and hinting.

pub fn hypot(mut a: i32, mut b: i32) -> i32 {
    a = a.abs();
    b = b.abs();
    if a > b {
        a + ((3 * b) >> 3)
    } else {
        b + ((3 * a) >> 3)
    }
}

pub fn floor(x: i32) -> i32 {
    x & !63
}

pub fn round(x: i32) -> i32 {
    floor(x + 32)
}

#[inline(always)]
pub fn mul(a: i32, b: i32) -> i32 {
    let ab = a as i64 * b as i64;
    ((ab + 0x8000 - i64::from(ab < 0)) >> 16) as i32
}

pub fn div(mut a: i32, mut b: i32) -> i32 {
    let mut sign = 1;
    if a < 0 {
        a = -a;
        sign = -1;
    }
    if b < 0 {
        b = -b;
        sign = -sign;
    }
    let q = if b == 0 {
        0x7FFFFFFF
    } else {
        ((((a as u64) << 16) + ((b as u64) >> 1)) / (b as u64)) as u32
    };
    if sign < 0 {
        -(q as i32)
    } else {
        q as i32
    }
}

pub fn muldiv(mut a: i32, mut b: i32, mut c: i32) -> i32 {
    let mut sign = 1;
    if a < 0 {
        a = -a;
        sign = -1;
    }
    if b < 0 {
        b = -b;
        sign = -sign;
    }
    if c < 0 {
        c = -c;
        sign = -sign;
    }
    if c == 0 {
        a = 0x7FFFFFFF;
    } else if a + b <= 129894 - (c >> 17) {
        a = (a * b + (c >> 1)) / c;
    } else {
        a = ((a as i64) * (b as i64) / (c as i64)) as i32;
    }
    if sign < 0 {
        -a
    } else {
        a
    }
}

pub fn transform(x: i32, y: i32, xx: i32, yx: i32, xy: i32, yy: i32) -> (i32, i32) {
    let scale = 0x10000;
    (
        muldiv(x, xx, scale) + muldiv(y, xy, scale),
        muldiv(x, yx, scale) + muldiv(y, yy, scale),
    )
}
