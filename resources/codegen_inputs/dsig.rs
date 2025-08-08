#![parse_module(read_fonts::tables::dsig)]

/// [DSIG (Digital Signature Table)](https://docs.microsoft.com/en-us/typography/opentype/spec/dsig#table-structure) table
#[tag = "DSIG"]
table Dsig {
    /// Version number of the DSIG table (0x00000001)
    #[compile(1)]
    version: u32,

    /// Number of signatures in the table
    #[compile(array_len($signature_records))]
    num_signatures: u16,

    /// Permission flags
    flags: PermissionFlags,

    /// Array of signature records
    #[count($num_signatures)]
    signature_records: [SignatureRecord],
}

/// [Permission flags](https://learn.microsoft.com/en-us/typography/opentype/spec/dsig#table-structure)
flags u16 PermissionFlags {
    /// Bit 0: Cannot be resigned
    CANNOT_BE_RESIGNED = 0b0000_0000_0000_0001,
}

/// [Signature Record](https://learn.microsoft.com/en-us/typography/opentype/spec/dsig#table-structure)
record SignatureRecord {
    /// Format of the signature
    // TODO: How do we validate?
    // TODO: Can we use an enum, even though the format is on the 'outside'?
    #[compile(1)]
    format: u32,

    /// Length of signature in bytes
    // TODO: Can this be derived automatically?
    #[compile(self.compute_signature_block_len())]
    length: u32,
    
    /// Offset to the signature block from the beginning of the table
    // TODO: Do we need to factor in the outer length?
    signature_block_offset: Offset32<SignatureBlockFormat1>,
}

/// [Signature Block Format 1](https://learn.microsoft.com/en-us/typography/opentype/spec/dsig#table-structure)
table SignatureBlockFormat1 {
    /// Reserved for future use; set to zero.
    #[skip_getter]
    #[compile(0)]
    _reserved1: u16,

    /// Reserved for future use; set to zero.
    #[skip_getter]
    #[compile(0)]
    _reserved2: u16,

    /// Length (in bytes) of the PKCS#7 packet in the signature field.
    #[compile(array_len($signature))]
    signature_length: u32,

    /// PKCS#7 packet
    #[count($signature_length)]
    signature: [u8],
}
