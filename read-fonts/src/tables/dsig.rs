//! The [DSIG](https://learn.microsoft.com/en-us/typography/opentype/spec/dsig) table

include!("../../generated/generated_dsig.rs");

#[cfg(test)]
mod tests {
    use font_test_data::bebuffer::BeBuffer;

    use super::{Dsig, PermissionFlags};
    use crate::{FontData, FontRead};

    /// An empty, dummy DSIG, as inserted by fonttools.
    /// See <https://github.com/fonttools/fonttools/blob/ec716f11851f8d5a04e3f535b53219d97001482a/Lib/fontTools/fontBuilder.py#L823-L833>.
    #[test]
    fn test_empty() {
        let buf = BeBuffer::new()
            .push(0x1u32) // version
            .push(0x0u16) // numSignatures
            .push(0x0u16); // flags

        let dsig = Dsig::read(FontData::new(buf.data())).unwrap();
        assert_eq!(dsig.version(), 1);
        assert_eq!(dsig.signature_records().len(), 0);
        assert_eq!(dsig.flags(), PermissionFlags::empty());
    }

    // A DSIG with a single entry. For ease-of-testing, we use 0xDEADBEEF
    // instead of a full PKCS#7 packet, as it would not be this crate's
    // responsibility to validate it anyway.
    #[test]
    fn test_beef() {
        let buf = BeBuffer::new()
            .push(0x1_u32) // DsigHeader.version
            .push(0x1_u16) // DsigHeader.numSignatures
            .push(0x1_u16) // DsigHeader.flags
            .push(0x1_u32) // SignatureRecord.format
            .push(0xC_u32) // SignatureRecord.length
            .push(0x14_u32) // SignatureRecord.signatureBlockOffset
            .push(0x0_u16) // SignatureBlockFormat1.reserved1
            .push(0x0_u16) // SignatureBlockFormat1.reserved2
            .push(0x4_u32) // SignatureBlockFormat1.signatureLength
            .push(0xDEADBEEF_u32); // SignatureBlockFormat1.signature

        let data = FontData::new(buf.data());

        let dsig = Dsig::read(data).unwrap();

        assert_eq!(dsig.version(), 1);
        assert_eq!(dsig.signature_records().len(), 1);
        assert_eq!(dsig.flags(), PermissionFlags::CANNOT_BE_RESIGNED);

        let record = dsig.signature_records()[0];
        assert_eq!(record.format(), 1);

        let block = record.signature_block(data).unwrap();
        assert_eq!(block.signature(), &[0xDE, 0xAD, 0xBE, 0xEF]);
    }
}
