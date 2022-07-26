//! OpenType Layout

mod gpos;

include!("../generated/layout.rs");

fn delta_value_count(start_size: u16, end_size: u16, delta_format: DeltaFormat) -> usize {
    let range_len = start_size.saturating_add(1).saturating_sub(end_size) as usize;
    let val_per_word = match delta_format {
        DeltaFormat::Local2BitDeltas => 8,
        DeltaFormat::Local4BitDeltas => 4,
        DeltaFormat::Local8BitDeltas => 2,
        _ => return 0,
    };

    let count = range_len / val_per_word;
    let extra = (range_len % val_per_word).min(1);
    count + extra
}

fn minus_one(val: impl Into<usize>) -> usize {
    val.into().saturating_sub(1)
}
