//! The [DSIG](https://learn.microsoft.com/en-us/typography/opentype/spec/dsig) table

include!("../../generated/generated_dsig.rs");

impl SignatureRecord {
    fn compute_signature_block_len(&self) -> u32 {
        self.signature_block.compute_len()
    }
}

impl SignatureBlockFormat1 {
    const HEADER_LENGTH: usize = 2 + 2 + 4;

    fn compute_len(&self) -> u32 {
        (Self::HEADER_LENGTH + self.signature.len())
            .try_into()
            .expect("DSIG signature block overflow")
    }

    fn compute_signature_len(&self) -> u32 {
        self.signature
            .len()
            .try_into()
            .expect("DSIG signature format 1 overflow")
    }
}
