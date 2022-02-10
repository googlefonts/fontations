use zerocopy::{FromBytes, Unaligned, BE, I16, I32, I64, U16, U32};

pub type Int8 = i8;
pub type Uint8 = u8;
pub type Int16 = I16<BE>;
pub type Uint16 = U16<BE>;
pub type Uint24 = [u8; 3];
pub type Int32 = I32<BE>;
pub type Uint32 = U32<BE>;

pub type Fixed = Int32;
pub type F2dot14 = Int16;
pub type LongDateTime = I64<BE>;

pub type Offset16 = Uint16;
pub type Offset24 = Uint24;
pub type Offset32 = Uint32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Unaligned, FromBytes)]
#[repr(C)]
pub struct Tag([u8; 4]);
pub type Version16Dot16 = Uint32;
