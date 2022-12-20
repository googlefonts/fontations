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

pub fn ceil(x: i32) -> i32 {
    floor(x + 63)
}

pub fn floor_pad(x: i32, n: i32) -> i32 {
    x & !(n - 1)
}

pub fn round_pad(x: i32, n: i32) -> i32 {
    floor_pad(x + n / 2, n)
}

pub fn muldiv_no_round(mut a: i32, mut b: i32, mut c: i32) -> i32 {
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

pub fn mul14(a: i32, b: i32) -> i32 {
    let mut v = a as i64 * b as i64;
    v += 0x2000 + (v >> 63);
    (v >> 14) as i32
}

pub fn dot14(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    let mut v1 = ax as i64 * bx as i64;
    let v2 = ay as i64 * by as i64;
    v1 += v2;
    v1 += 0x2000 + (v1 >> 63);
    (v1 >> 14) as i32
}

pub fn transform(x: i32, y: i32, xx: i32, yx: i32, xy: i32, yy: i32) -> (i32, i32) {
    let scale = 0x10000;
    (
        muldiv(x, xx, scale) + muldiv(y, xy, scale),
        muldiv(x, yx, scale) + muldiv(y, yy, scale),
    )
}
