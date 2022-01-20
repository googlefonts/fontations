use crate::Uint24;

/// Offsets to tables

macro_rules! impl_offset {
    ($name:ident, $bits:literal, $ty:ty, $null:expr ) => {
        #[doc = concat!("A", stringify!($bits), "-bit offset to a table.")]
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name($ty);

        impl $name {
            /// A NULL offset
            pub const NULL: Self = Self($null);

            /// Create a new offset.
            pub fn new(raw: $ty) -> Self {
                Self(raw)
            }

            /// Returns true if this offset is NULL.
            pub fn is_null(self) -> bool {
                self == Self::NULL
            }

            /// Return the raw integer value of this offset
            pub fn to_raw(self) -> $ty {
                self.0
            }
        }
    };
}

impl_offset!(Offset16, 16, u16, 0);
impl_offset!(Offset24, 24, Uint24, Uint24::MIN);
impl_offset!(Offset32, 32, u32, 0);
