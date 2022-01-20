//! fixed-point numerical types

/// 32-bit signed fixed-point number (16.16)
#[derive(Debug, Clone, Copy)]
pub struct Fixed(f32); // temporary impl

/// 16-bit signed fixed number with the low 14 bits of fraction (2.14).
#[derive(Debug, Clone, Copy)]
pub struct F2dot14(f32);
