#![parse_module(read_fonts::exp::tables::cff)]

// The CFF INDEX, written for the reworked framework.
//
// This exists separately from `cff.rs` because that input cannot say what the
// spec says. An INDEX is:
//
//     count: Card16
//     if count != 0:
//         offSize: OffSize
//         offsets: [count + 1 offsets of offSize bytes]
//         data
//
// An empty INDEX is *two bytes*: just the count. The existing declaration puts
// `off_size` in the fixed header, so codegen computes MIN_SIZE as 3 and a legal
// empty INDEX fails to parse — which is why `read-fonts/src/ps/cff/index.rs`
// carries a hand-written `Empty` variant that never goes near the generated
// table.
//
// `#[if_nonzero($count)]` says it directly, and MIN_SIZE stops at the first
// conditional field, so it becomes 2.

/// A CFF [INDEX](https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#5-index-data)
table Index {
    /// Number of objects stored in INDEX.
    count: u16,
    /// Object array element size. Absent when the INDEX is empty.
    #[if_nonzero($count)]
    off_size: u8,
    /// Bytes containing `count + 1` offsets each of `off_size`.
    #[if_nonzero($count)]
    #[count(add_multiply($count, 1, $off_size))]
    offsets: [u8],
    /// Array containing the object data.
    #[if_nonzero($count)]
    #[count(..)]
    data: [u8],
}

/// A CFF2 [INDEX](https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#5-index-data),
/// which differs only in the width of the count.
table Index2 {
    /// Number of objects stored in INDEX.
    count: u32,
    /// Object array element size. Absent when the INDEX is empty.
    #[if_nonzero($count)]
    off_size: u8,
    /// Bytes containing `count + 1` offsets each of `off_size`.
    #[if_nonzero($count)]
    #[count(add_multiply($count, 1, $off_size))]
    offsets: [u8],
    /// Array containing the object data.
    #[if_nonzero($count)]
    #[count(..)]
    data: [u8],
}
