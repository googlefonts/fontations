//! A buffer for bytes that is optimized for small sizes.

use std::{num::NonZeroU8, sync::Arc};

const SHORT_BUF_LEN: usize = 23;

/// A buffer for bytes that is optimized for small sizes.
///
/// It uses a small-object optimization to avoid allocations for buffers
/// up to 23 bytes long. For larger buffers, it uses an `Arc<[u8]>` to
/// allow for cheap cloning.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BytesBuffer(BytesBufferRepr);

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum BytesBufferRepr {
    Short {
        // NonZeroU8 allows enables an enum tag optimization that allows us to increase
        // SHORT_BUF_LEN from 22 to 23.
        len: NonZeroU8,
        data: [u8; SHORT_BUF_LEN],
    },
    // TODO: We can use `Option<Arc<[u8]>>` to optimize the empty buffer case if needed.
    Long(Arc<[u8]>),
}

impl BytesBuffer {
    /// Creates a new `BytesBuffer` from a byte slice.
    pub fn new(bytes: &[u8]) -> BytesBuffer {
        let repr = match bytes.len() {
            len @ 1..=SHORT_BUF_LEN => {
                let mut data = [0u8; 23];
                data[0..len].copy_from_slice(bytes);
                BytesBufferRepr::Short {
                    len: NonZeroU8::new(len as u8).unwrap(),
                    data,
                }
            }
            _ => BytesBufferRepr::Long(bytes.into()),
        };
        BytesBuffer(repr)
    }

    /// Returns the buffer contents as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        match &self.0 {
            BytesBufferRepr::Short { len, data } => &data[..len.get() as usize],
            BytesBufferRepr::Long(data) => data,
        }
    }

    /// Returns `true` if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

impl AsRef<[u8]> for BytesBuffer {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::fmt::Debug for BytesBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Ok(s) = std::str::from_utf8(self.as_slice()) {
            write!(f, "{:?}", s)
        } else {
            write!(f, "{:?}", self.as_slice())
        }
    }
}

impl From<&str> for BytesBuffer {
    fn from(s: &str) -> Self {
        BytesBuffer::new(s.as_bytes())
    }
}

/// A builder for creating a `BytesBuffer`.
#[derive(Debug, Default)]
pub(crate) struct BytesBufferBuilder {
    len: usize,
    data: [u8; SHORT_BUF_LEN],
    long_data: Option<Vec<u8>>,
}

impl BytesBufferBuilder {
    /// Creates a new `BytesBufferBuilder` with at least the specified capacity.
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        if capacity > SHORT_BUF_LEN {
            Self {
                len: 0,
                data: [0; SHORT_BUF_LEN],
                long_data: Some(Vec::with_capacity(capacity)),
            }
        } else {
            Self::default()
        }
    }

    /// Returns `true` if the builder is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the current builder contents as a byte slice.
    pub(crate) fn as_slice(&self) -> &[u8] {
        if let Some(long_data) = &self.long_data {
            long_data.as_slice()
        } else {
            &self.data[..self.len]
        }
    }

    /// Pushes a single byte to the end of the builder.
    pub(crate) fn push(&mut self, byte: u8) {
        if let Some(long_data) = &mut self.long_data {
            long_data.push(byte);
        } else if self.len < SHORT_BUF_LEN {
            self.data[self.len] = byte;
            self.len += 1;
        } else {
            let mut long_data = Vec::with_capacity(SHORT_BUF_LEN * 2);
            long_data.extend_from_slice(&self.data[..self.len]);
            long_data.push(byte);
            self.long_data = Some(long_data);
        }
    }

    /// Extends the builder with the contents of a byte slice.
    pub(crate) fn extend_from_slice(&mut self, slice: &[u8]) {
        if let Some(long_data) = &mut self.long_data {
            long_data.extend_from_slice(slice);
        } else if self.len + slice.len() <= SHORT_BUF_LEN {
            self.data[self.len..self.len + slice.len()].copy_from_slice(slice);
            self.len += slice.len();
        } else {
            let required_cap = self.len.checked_add(slice.len()).unwrap();
            let capacity = 2 * std::cmp::max(SHORT_BUF_LEN, required_cap);
            let mut long_data = Vec::with_capacity(capacity);
            long_data.extend_from_slice(&self.data[..self.len]);
            long_data.extend_from_slice(slice);
            self.long_data = Some(long_data);
        }
    }

    /// Consumes the builder and returns a `BytesBuffer`.
    pub(crate) fn build(self) -> BytesBuffer {
        if let Some(long_data) = self.long_data {
            BytesBuffer(BytesBufferRepr::Long(long_data.into()))
        } else if self.len > 0 {
            BytesBuffer(BytesBufferRepr::Short {
                len: NonZeroU8::new(self.len as u8).unwrap(),
                data: self.data,
            })
        } else {
            BytesBuffer(BytesBufferRepr::Long(Arc::new([])))
        }
    }
}

impl<const N: usize> PartialEq<&[u8; N]> for BytesBufferBuilder {
    fn eq(&self, other: &&[u8; N]) -> bool {
        self.as_slice() == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    #[test]
    fn bytes_buffer_is_small() {
        assert!(std::mem::size_of::<BytesBuffer>() <= 24);
    }

    #[test]
    fn empty_slice_creates_empty_long_variant() {
        let buf = BytesBuffer::new(&[]);
        assert!(buf.is_empty());
        assert_eq!(buf.as_slice(), &[]);
        assert!(matches!(buf.0, BytesBufferRepr::Long(_)));
    }

    #[test]
    fn one_byte_slice_creates_short_variant() {
        let buf = BytesBuffer::new(&[42u8]);
        assert!(!buf.is_empty());
        assert_eq!(buf.as_slice(), &[42u8]);
        assert!(matches!(buf.0, BytesBufferRepr::Short { .. }));
    }

    #[test]
    fn max_short_slice_creates_short_variant() {
        let buf = BytesBuffer::new(&[42u8; SHORT_BUF_LEN]);
        assert_eq!(buf.as_slice(), &[42u8; SHORT_BUF_LEN]);
        assert!(matches!(buf.0, BytesBufferRepr::Short { .. }));
    }

    #[test]
    fn overflow_short_slice_creates_long_variant() {
        let buf = BytesBuffer::new(&[42u8; SHORT_BUF_LEN + 1]);
        assert_eq!(buf.as_slice(), &[42u8; SHORT_BUF_LEN + 1]);
        assert!(matches!(buf.0, BytesBufferRepr::Long(_)));
    }

    #[test]
    fn as_slice_on_short_variant_returns_correct_bytes() {
        let buf = BytesBuffer::new(&[1, 2, 3]);
        assert_eq!(buf.as_slice(), &[1, 2, 3]);
    }

    #[test]
    fn as_slice_on_long_variant_returns_correct_bytes() {
        let buf = BytesBuffer::new(&[7; 2 * SHORT_BUF_LEN]);
        assert_eq!(buf.as_slice(), &[7; 2 * SHORT_BUF_LEN]);
    }

    #[test]
    fn is_empty_on_empty_buffer_returns_true() {
        let buf = BytesBuffer::new(&[]);
        assert!(buf.is_empty());
    }

    #[test]
    fn is_empty_on_non_empty_buffer_returns_false() {
        let buf = BytesBuffer::new(&[1]);
        assert!(!buf.is_empty());
    }

    #[test]
    fn debug_format_on_utf8_returns_byte_slice_representation() {
        let buf = BytesBuffer::new(b"abcABC123");
        let formatted = format!("{:?}", buf);
        assert_eq!(formatted, "\"abcABC123\"");
    }

    #[test]
    fn debug_format_on_invalid_utf8_returns_byte_slice_representation() {
        let buf = BytesBuffer::new(&[0, 159, 146, 150]);
        let formatted = format!("{:?}", buf);
        assert_eq!(formatted, "[0, 159, 146, 150]");
    }

    #[test]
    fn partial_eq_on_identical_buffers_returns_true() {
        assert_eq!(BytesBuffer::new(b"abc"), BytesBuffer::new(b"abc"));
    }

    #[test]
    fn partial_eq_on_different_buffers_returns_false() {
        assert_ne!(BytesBuffer::new(b"abc"), BytesBuffer::new(b"xyz"));
    }

    #[test]
    fn clone_on_short_variant_returns_identical_buffer() {
        let buf1 = BytesBuffer::new(b"short");
        let buf2 = buf1.clone();
        assert_eq!(buf1, buf2);
        assert!(matches!(buf2.0, BytesBufferRepr::Short { .. }));
    }

    #[test]
    fn clone_on_long_variant_returns_identical_buffer() {
        let buf1 = BytesBuffer::new(&[0; 100]);
        let buf2 = buf1.clone();
        assert_eq!(buf1, buf2);
        assert!(matches!(buf2.0, BytesBufferRepr::Long(_)));
    }

    #[test]
    fn hash_on_equal_buffers_yields_same_hash_value() {
        let buf1 = BytesBuffer::new(b"hash_me");
        let buf2 = BytesBuffer::new(b"hash_me");

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        buf1.hash(&mut hasher1);
        buf2.hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    #[test]
    fn as_ref_returns_correct_slice() {
        let buf = BytesBuffer::new(b"hello");
        assert_eq!(buf.as_ref(), b"hello");
    }

    #[test]
    fn cmp_on_ordered_buffers_returns_correct_ordering() {
        let buf_small = BytesBuffer::new(b"abc");
        let buf_large = BytesBuffer::new(b"def");
        assert!(buf_small < buf_large);
        assert_eq!(buf_small.cmp(&buf_large), std::cmp::Ordering::Less);
    }

    #[test]
    fn new_builder_creates_empty_short_state() {
        assert_eq!(BytesBufferBuilder::default().len, 0);
        assert!(BytesBufferBuilder::default().long_data.is_none());
    }

    #[test]
    fn with_capacity_less_than_short_buf_len_uses_short_state() {
        let builder = BytesBufferBuilder::with_capacity(10);
        assert_eq!(builder.len, 0);
        assert!(builder.long_data.is_none());
    }

    #[test]
    fn with_capacity_greater_than_short_buf_len_allocates_long_state() {
        let builder = BytesBufferBuilder::with_capacity(30);
        assert_eq!(builder.len, 0);
        assert!(builder.long_data.is_some());
    }

    #[test]
    fn is_empty_on_new_builder_returns_true() {
        assert!(BytesBufferBuilder::default().is_empty());
    }

    #[test]
    fn is_empty_on_builder_initialized_with_large_capacity_returns_true() {
        assert!(BytesBufferBuilder::with_capacity(30).is_empty());
    }

    #[test]
    fn push_on_short_state_under_capacity_retains_short_state() {
        let mut builder = BytesBufferBuilder::default();
        builder.push(42);
        assert_eq!(builder.as_slice(), &[42]);
        assert!(builder.long_data.is_none());
    }

    #[test]
    fn push_on_short_state_at_capacity_transitions_to_long_state() {
        let mut builder = BytesBufferBuilder::default();
        for _ in 0..SHORT_BUF_LEN {
            builder.push(1);
        }
        assert!(builder.long_data.is_none());

        builder.push(2); // 24th element transitions to Long
        assert!(builder.long_data.is_some());
        assert_eq!(builder.as_slice().len(), SHORT_BUF_LEN + 1);
        assert_eq!(builder.as_slice()[SHORT_BUF_LEN], 2);
    }

    #[test]
    fn push_on_preallocated_large_capacity_builder_appends_to_long_data() {
        let mut builder = BytesBufferBuilder::with_capacity(30);
        builder.push(100);
        assert_eq!(builder.as_slice(), &[100]);
        assert!(builder.long_data.is_some());
    }

    #[test]
    fn extend_from_slice_fitting_in_short_state_retains_short_state() {
        let mut builder = BytesBufferBuilder::default();
        builder.extend_from_slice(&[1, 2, 3]);
        assert_eq!(builder.as_slice(), &[1, 2, 3]);
        assert!(builder.long_data.is_none());
    }

    #[test]
    fn extend_from_slice_overflowing_short_state_transitions_to_long_state() {
        let mut builder = BytesBufferBuilder::default();
        builder.extend_from_slice(&[1; SHORT_BUF_LEN - 1]);
        assert!(builder.long_data.is_none());

        builder.extend_from_slice(&[2, 3]); // Exceeds SHORT_BUF_LEN
        assert!(builder.long_data.is_some());
        assert_eq!(builder.as_slice().len(), SHORT_BUF_LEN + 1);
    }

    #[test]
    fn extend_from_slice_on_already_long_state_appends_to_long_data() {
        let mut builder = BytesBufferBuilder::with_capacity(30);
        builder.extend_from_slice(&[1, 2, 3]);
        assert_eq!(builder.as_slice(), &[1, 2, 3]);
        assert!(builder.long_data.is_some());
    }

    #[test]
    fn build_on_empty_builder_returns_empty_long_buffer() {
        let builder = BytesBufferBuilder::default();
        let buf = builder.build();
        assert!(buf.is_empty());
        assert!(matches!(buf.0, BytesBufferRepr::Long(_)));
    }

    #[test]
    fn build_on_short_state_builder_returns_short_buffer() {
        let mut builder = BytesBufferBuilder::default();
        builder.push(1);
        let buf = builder.build();
        assert_eq!(buf.as_slice(), &[1]);
        assert!(matches!(buf.0, BytesBufferRepr::Short { .. }));
    }

    #[test]
    fn build_on_long_state_builder_returns_long_buffer() {
        let mut builder = BytesBufferBuilder::default();
        builder.extend_from_slice(&[1; SHORT_BUF_LEN + 5]);
        let buf = builder.build();
        assert_eq!(buf.as_slice().len(), SHORT_BUF_LEN + 5);
        assert!(matches!(buf.0, BytesBufferRepr::Long(_)));
    }

    #[test]
    fn partial_eq_on_builder_and_matching_slice_returns_true() {
        let mut builder = BytesBufferBuilder::default();
        builder.extend_from_slice(&[1, 2, 3]);
        assert!(builder == &[1, 2, 3]);
    }
}
