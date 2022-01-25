//! a datetime type

/// A simple datetime type.
///
/// This represented as a number of seconds since 12:00 midnight, January 1, 1904, UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LongDateTime(i64);

impl LongDateTime {
    /// The number of seconds since 00:00 1904-01-01, UTC.
    ///
    /// This can be a negative number, which presumably represents a date prior
    /// to the reference date.
    pub fn as_secs(&self) -> i64 {
        self.0
    }
}

super::conversion::impl_from_be_by_proxy!(LongDateTime, 8);
//TODO: maybe a 'chrono' feature for constructing these sanely?
