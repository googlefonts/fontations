#![parse_module(read_fonts::codegen_test::embedded_records)]

/// A fixed-size record holding an offset measured from the start of whichever
/// table embeds it. This is the shape of a MATH `MathValueRecord`.
#[skip_constructor]
record ValueAndDevice {
    /// A plain value.
    value: i16,
    /// Offset to a device table, from the beginning of the parent table.
    #[nullable]
    device_offset: Offset16<Device>,
}

/// The target of the offset above, so we can check it resolves.
#[skip_constructor]
table Device {
    marker: u16,
}

/// A record embedded in another record, which already worked; here to catch a
/// regression.
#[skip_constructor]
record Pair {
    first: ValueAndDevice,
    second: ValueAndDevice,
}

/// Several records embedded back to back, the way MathConstants embeds fifty
/// one of them, with scalars interleaved to check the offsets stay aligned.
#[skip_constructor]
table EmbeddedRecords {
    /// A leading scalar, so the records do not start at zero.
    version: u16,
    first: ValueAndDevice,
    second: ValueAndDevice,
    /// A scalar between records.
    middle: u16,
    third: ValueAndDevice,
    /// A record whose own fields are records.
    pair: Pair,
    /// A trailing scalar, to check the table's size accounts for everything
    /// above it.
    trailer: u16,
}

/// An embedded record followed by a variable-length array: the record is still
/// covered by the table's minimum size, so it is allowed.
#[skip_constructor]
table RecordThenArray {
    metrics: ValueAndDevice,
    #[compile(array_len($values))]
    value_count: u16,
    #[count($value_count)]
    values: [u16],
}
