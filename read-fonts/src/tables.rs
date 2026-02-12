//! The various font tables

#[cfg(feature = "aat")]
pub mod aat;
#[cfg(feature = "ankr")]
pub mod ankr;
#[cfg(feature = "avar")]
pub mod avar;
#[cfg(feature = "base")]
pub mod base;
#[cfg(feature = "bitmap")]
pub mod bitmap;
#[cfg(feature = "cbdt")]
pub mod cbdt;
#[cfg(feature = "cblc")]
pub mod cblc;
#[cfg(feature = "cff")]
pub mod cff;
#[cfg(feature = "cff2")]
pub mod cff2;
#[cfg(feature = "cmap")]
pub mod cmap;
#[cfg(feature = "colr")]
pub mod colr;
#[cfg(feature = "cpal")]
pub mod cpal;
#[cfg(feature = "cvar")]
pub mod cvar;
#[cfg(feature = "dsig")]
pub mod dsig;
#[cfg(feature = "ebdt")]
pub mod ebdt;
#[cfg(feature = "eblc")]
pub mod eblc;
#[cfg(feature = "feat")]
pub mod feat;
#[cfg(feature = "fvar")]
pub mod fvar;
#[cfg(feature = "gasp")]
pub mod gasp;
#[cfg(feature = "gdef")]
pub mod gdef;
#[cfg(feature = "glyf")]
pub mod glyf;
#[cfg(feature = "gpos")]
pub mod gpos;
#[cfg(feature = "gsub")]
pub mod gsub;
#[cfg(feature = "gvar")]
pub mod gvar;
#[cfg(feature = "hdmx")]
pub mod hdmx;
#[cfg(feature = "head")]
pub mod head;
#[cfg(feature = "hhea")]
pub mod hhea;
#[cfg(feature = "hmtx")]
pub mod hmtx;
#[cfg(feature = "hvar")]
pub mod hvar;
#[cfg(feature = "kern")]
pub mod kern;
#[cfg(feature = "kerx")]
pub mod kerx;
#[cfg(feature = "layout")]
pub mod layout;
#[cfg(feature = "loca")]
pub mod loca;
#[cfg(feature = "ltag")]
pub mod ltag;
#[cfg(feature = "maxp")]
pub mod maxp;
#[cfg(feature = "meta")]
pub mod meta;
#[cfg(feature = "morx")]
pub mod morx;
#[cfg(feature = "mvar")]
pub mod mvar;
#[cfg(feature = "name")]
pub mod name;
#[cfg(feature = "os2")]
pub mod os2;
#[cfg(feature = "post")]
pub mod post;
#[cfg(feature = "postscript")]
pub mod postscript;
#[cfg(feature = "sbix")]
pub mod sbix;
#[cfg(feature = "stat")]
pub mod stat;
#[cfg(feature = "svg")]
pub mod svg;
#[cfg(feature = "trak")]
pub mod trak;
#[cfg(feature = "varc")]
pub mod varc;
#[cfg(feature = "variations")]
pub mod variations;
#[cfg(feature = "vhea")]
pub mod vhea;
#[cfg(feature = "vmtx")]
pub mod vmtx;
#[cfg(feature = "vorg")]
pub mod vorg;
#[cfg(feature = "vvar")]
pub mod vvar;

#[cfg(feature = "ift")]
pub mod ift;

mod glyf_types;

/// Computes the table checksum for the given data.
///
/// See the OpenType [specification](https://learn.microsoft.com/en-us/typography/opentype/spec/otff#calculating-checksums)
/// for details.
pub fn compute_checksum(table: &[u8]) -> u32 {
    let mut sum = 0u32;
    let mut iter = table.chunks_exact(4);
    for quad in &mut iter {
        // this can't fail, and we trust the compiler to avoid a branch
        let array: [u8; 4] = quad.try_into().unwrap_or_default();
        sum = sum.wrapping_add(u32::from_be_bytes(array));
    }

    let rem = match *iter.remainder() {
        [a] => u32::from_be_bytes([a, 0, 0, 0]),
        [a, b] => u32::from_be_bytes([a, b, 0, 0]),
        [a, b, c] => u32::from_be_bytes([a, b, c, 0]),
        _ => 0,
    };

    sum.wrapping_add(rem)
}
