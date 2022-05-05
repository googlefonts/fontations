//! The [head](https://docs.microsoft.com/en-us/typography/opentype/spec/head) table

#[path = "../../generated/generated_head.rs"]
mod generated;

use font_types::Tag;

pub use generated::*;

/// 'name'
pub const TAG: Tag = Tag::new(b"head");

#[cfg(test)]
mod tests {
    use font_types::{test_helpers::BeBuffer, Fixed, FontRead, LongDateTime};

    #[test]
    fn smoke_text() {
        let mut buf = BeBuffer::new();
        buf.extend([1u16, 0u16]);
        buf.push(Fixed::from_f64(2.8));
        buf.extend([42u32, 0x5f0f3cf5]);
        buf.extend([16u16, 4096]); // flags, upm
        buf.extend([LongDateTime::new(-500), LongDateTime::new(101)]);
        buf.extend([-100i16, -50, 400, 711]);
        buf.extend([0u16, 12]); // mac_style / ppem
        buf.extend([2i16, 1, 0]);

        let head = super::Head::read(&buf).unwrap();
        assert_eq!(head.major_version(), 1);
        assert_eq!(head.minor_version(), 0);
        assert_eq!(head.font_revision(), Fixed::from_f64(2.8));
        assert_eq!(head.units_per_em(), 4096);
        assert_eq!(head.created().as_secs(), -500);
        assert_eq!(head.y_min(), -50);
    }
}
