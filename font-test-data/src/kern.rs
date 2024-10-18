// from https://github.com/fonttools/fonttools/blob/729b3d2960efd3/Tests/ttLib/tables/_k_e_r_n_test.py#L9
#[rustfmt::skip]
pub static KERN_VER_0_FMT_0_DATA: &[u8] = &[
    0x00, 0x00, // "0000 "  #  0: version=0
    0x00, 0x01, // "0001 "  #  2: nTables=1
    0x00, 0x00, // "0000 "  #  4: version=0 (bogus field, unused)
    0x00, 0x20, // "0020 "  #  6: length=32
    0x00,       // "00 "  #  8: format=0
    0x01,       // "01 "  #  9: coverage=1
    0x00, 0x03, // "0003 "  # 10: nPairs=3
    0x00, 0x0C, // "000C "  # 12: searchRange=12
    0x00, 0x01, // "0001 "  # 14: entrySelector=1
    0x00, 0x06, // "0006 "  # 16: rangeShift=6
    0x00, 0x04, 0x00, 0x0C, 0xFF, 0xD8, // "0004 000C FFD8 "  # 18: l=4, r=12, v=-40
    0x00, 0x04, 0x00, 0x1C, 0x00, 0x28, // "0004 001C 0028 "  # 24: l=4, r=28, v=40
    0x00, 0x05, 0x00, 0x28, 0xFF, 0xCE, // "0005 0028 FFCE "  # 30: l=5, r=40, v=-50
];
