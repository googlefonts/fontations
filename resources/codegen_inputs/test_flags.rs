// This file tests the generation of bitflags.

#![parse_module(read_fonts::codegen_test::flags)]

/// Some flags!
flags u16 ValueFormat {
    /// Includes horizontal adjustment for placement
    X_PLACEMENT = 0x0001,
    /// Includes vertical adjustment for placement
    Y_PLACEMENT = 0x0002,
}
