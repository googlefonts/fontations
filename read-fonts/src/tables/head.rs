//! The [head](https://docs.microsoft.com/en-us/typography/opentype/spec/head) table

use types::Tag;

/// 'head'
pub const TAG: Tag = Tag::new(b"head");

include!("../../generated/generated_head.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::BeBuffer;

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

        let head = super::Head::read(buf.font_data()).unwrap();
        assert_eq!(head.version(), MajorMinor::VERSION_1_0);
        assert_eq!(head.font_revision(), Fixed::from_f64(2.8));
        assert_eq!(head.units_per_em(), 4096);
        assert_eq!(head.created().as_secs(), -500);
        assert_eq!(head.y_min(), -50);
    }
}
