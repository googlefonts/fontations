//! Hinting error definitions.

use super::code::{DecodeError, Program};

#[derive(Clone, Debug)]
pub enum HintErrorKind {
    Decode(DecodeError),
    InvalidOpcode(u8),
    DefinitionInGlyphProgram,
    NestedDefinition,
    InvalidDefintionIndex(usize),
    ValueStackOverflow,
    ValueStackUnderflow,
    CallStackOverflow,
    CallStackUnderflow,
    InvalidStackValue,
    InvalidPointIndex(usize),
    InvalidPointRange(usize, usize),
    InvalidContourIndex(usize),
    InvalidCvtIndex(usize),
    InvalidStorageIndex(usize),
    DivideByZero,
    InvalidZoneIndex(i32),
    NegativeLoopCounter,
    InvalidJump,
}

impl From<DecodeError> for HintErrorKind {
    fn from(value: DecodeError) -> Self {
        Self::Decode(value)
    }
}

#[derive(Clone, Debug)]
pub struct HintError {
    pub program: Program,
    pub pc: usize,
    pub kind: HintErrorKind,
}
