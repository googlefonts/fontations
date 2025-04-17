#[cfg(feature = "c-brotli")]
mod c_brotli;

#[cfg(feature = "rust-brotli")]
mod rust_brotli;

pub mod decode_error;

#[cfg(fuzzing)]
use decode_error::DecodeError;

/// A Shared Brotli Decoder.
///
/// Shared brotli (<https://datatracker.ietf.org/doc/draft-vandevenne-shared-brotli-format/>) is an
/// extension of brotli to allow the decompression to include a shared dictionary.
pub trait SharedBrotliDecoder {
    /// Decodes shared brotli encoded data using the optional shared dictionary.
    ///
    /// The shared dictionary is a raw LZ77 style dictionary, see:
    /// <https://datatracker.ietf.org/doc/html/draft-vandevenne-shared-brotli-format#section-3.2>
    ///
    /// Will fail if the decoded result will be greater than max_uncompressed_length. Any excess data
    /// in encoded after the encoded stream finishes is also considered an error.
    fn decode(
        &self,
        encoded: &[u8],
        shared_dictionary: Option<&[u8]>,
        max_uncompressed_length: usize,
    ) -> Result<Vec<u8>, decode_error::DecodeError>;
}

/// The brotli decoder provided by this crate.
///
/// By default a rust wrapper around the c brotli decoder implementation is used.
pub struct BuiltInBrotliDecoder;

/// An implementation that just passes through the input data.
///
/// Useful in fuzzers and unit testing.
pub struct NoopBrotliDecoder;

impl SharedBrotliDecoder for Box<dyn SharedBrotliDecoder> {
    fn decode(
        &self,
        encoded: &[u8],
        shared_dictionary: Option<&[u8]>,
        max_uncompressed_length: usize,
    ) -> Result<Vec<u8>, decode_error::DecodeError> {
        self.as_ref()
            .decode(encoded, shared_dictionary, max_uncompressed_length)
    }
}

impl SharedBrotliDecoder for BuiltInBrotliDecoder {
    fn decode(
        &self,
        encoded: &[u8],
        shared_dictionary: Option<&[u8]>,
        max_uncompressed_length: usize,
    ) -> Result<Vec<u8>, decode_error::DecodeError> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "c-brotli")] {
                #[allow(clippy::needless_return)]
                return c_brotli::shared_brotli_decode_c(
                    encoded,
                    shared_dictionary,
                    max_uncompressed_length,
                );
            } else if #[cfg(feature = "rust-brotli")] {
                return rust_brotli::shared_brotli_decode_rust(encoded, shared_dictionary, max_uncompressed_length);
            } else {
                compile_error!("At least one of 'c-brotli' or 'rust-brotli' must be enabled.");
            }
        }
    }
}

impl SharedBrotliDecoder for NoopBrotliDecoder {
    fn decode(
        &self,
        encoded: &[u8],
        _shared_dictionary: Option<&[u8]>,
        max_uncompressed_length: usize,
    ) -> Result<Vec<u8>, decode_error::DecodeError> {
        if encoded.len() <= max_uncompressed_length {
            Ok(encoded.to_vec())
        } else {
            Err(decode_error::DecodeError::MaxSizeExceeded)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decode_error::DecodeError;

    const TARGET: &[u8] = "hijkabcdeflmnohijkabcdeflmno\n".as_bytes();
    const BASE: &str = "abcdef\n";

    // This patch was manually generated with a brotli encoder (https://github.com/google/brotli)
    // uncompressed = TARGET
    // dict = BASE
    const SHARED_DICT_PATCH: [u8; 23] = [
        0xa1, 0xe0, 0x00, 0xc0, 0x2f, 0x3a, 0x38, 0xf4, 0x01, 0xd1, 0xaf, 0x54, 0x84, 0x14, 0x71,
        0x2a, 0x80, 0x04, 0xa2, 0x1c, 0xd3, 0xdd, 0x07,
    ];

    // This patch was manually generated with a brotli encoder (https://github.com/google/brotli)
    // uncompressed = TARGET
    const NO_DICT_PATCH: [u8; 26] = [
        0xa1, 0xe0, 0x00, 0xc0, 0x2f, 0x96, 0x1c, 0xf3, 0x03, 0xb1, 0xcf, 0x45, 0x95, 0x22, 0x4a,
        0xc5, 0x03, 0x21, 0xb2, 0x9a, 0x58, 0xd4, 0x7c, 0xf6, 0x1e, 0x00u8,
    ];

    #[test]
    fn brotli_decode_with_shared_dict() {
        assert_eq!(
            Ok(TARGET.to_vec()),
            BuiltInBrotliDecoder.decode(&SHARED_DICT_PATCH, Some(BASE.as_bytes()), TARGET.len(),)
        );
    }

    #[test]
    fn brotli_decode_without_shared_dict() {
        let base = "".as_bytes();

        assert_eq!(
            Ok(TARGET.to_vec()),
            BuiltInBrotliDecoder.decode(&NO_DICT_PATCH, None, TARGET.len())
        );

        // Check that empty base is handled the same as no base.
        assert_eq!(
            Ok(TARGET.to_vec()),
            BuiltInBrotliDecoder.decode(&NO_DICT_PATCH, Some(base), TARGET.len())
        );
    }

    #[test]
    fn brotli_decode_too_little_output() {
        assert_eq!(
            Err(DecodeError::MaxSizeExceeded),
            BuiltInBrotliDecoder.decode(
                &SHARED_DICT_PATCH,
                Some(BASE.as_bytes()),
                TARGET.len() - 1
            )
        );
    }

    #[test]
    fn brotli_decode_excess_output() {
        assert_eq!(
            Ok(TARGET.to_vec()),
            BuiltInBrotliDecoder.decode(
                &SHARED_DICT_PATCH,
                Some(BASE.as_bytes()),
                TARGET.len() + 1,
            )
        );
    }

    // TODO(garretrieger): there doesn't seem to be an easy way to detect this condition with
    // the rust brotli implementation. So disable for now. However, we need to make this behaviour
    // consistent between the two possible implementations. Either don't check for this in the c
    // version, or figure out how to have a similar check in rust.
    #[cfg(feature = "c-brotli")]
    #[test]
    fn brotli_decode_too_much_input() {
        let mut patch: Vec<u8> = NO_DICT_PATCH.to_vec();
        patch.push(0u8);

        assert_eq!(
            Err(DecodeError::ExcessInputData),
            BuiltInBrotliDecoder.decode(&patch, None, TARGET.len())
        );
    }

    #[test]
    fn brotli_decode_input_missing() {
        // Check what happens if input stream is missing some trailing bytes
        let patch: Vec<u8> = NO_DICT_PATCH[..NO_DICT_PATCH.len() - 1].to_vec();
        assert_eq!(
            Err(DecodeError::InvalidStream),
            BuiltInBrotliDecoder.decode(&patch, None, TARGET.len())
        );
    }

    #[test]
    fn brotli_decode_invalid() {
        let patch = [0xFF, 0xFF, 0xFFu8];
        assert_eq!(
            Err(DecodeError::InvalidStream),
            BuiltInBrotliDecoder.decode(&patch, None, 10)
        );
    }
}
