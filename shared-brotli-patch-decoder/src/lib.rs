use brotlic_sys::{
    BrotliDecoderAttachDictionary, BrotliDecoderCreateInstance, BrotliDecoderDecompressStream,
    BrotliDecoderDestroyInstance, BrotliDecoderResult_BROTLI_DECODER_RESULT_ERROR,
    BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT,
    BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_OUTPUT,
    BrotliDecoderResult_BROTLI_DECODER_RESULT_SUCCESS,
    BrotliSharedDictionaryType_BROTLI_SHARED_DICTIONARY_RAW, BROTLI_FALSE,
};
use core::ptr;

#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    InitFailure,
    InvalidStream,
    InvalidDictionary,
    MaxSizeExceeded,
    ExcessInputData,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::InitFailure => write!(f, "Failed to initialize the brotli decoder."),
            DecodeError::InvalidStream => {
                write!(f, "Brotli compressed stream is invalid, decoding failed.")
            }
            DecodeError::InvalidDictionary => write!(f, "Shared dictionary format is invalid."),
            DecodeError::MaxSizeExceeded => write!(f, "Decompressed size greater than maximum."),
            DecodeError::ExcessInputData => write!(
                f,
                "There is unconsumed data in the input stream after decoding."
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decodes shared brotli encoded data using the optional shared dictionary.
///
/// The shared dictionary is a raw LZ77 style dictionary, see:
/// <https://datatracker.ietf.org/doc/html/draft-vandevenne-shared-brotli-format-11#section-3.2>
///
/// Will fail if the decoded result will be greater then max_uncompressed_length. Any excess data
/// in encoded after the encoded stream finishes is also considered an error.
pub fn shared_brotli_decode(
    encoded: &[u8],
    shared_dictionary: Option<&[u8]>,
    max_uncompressed_length: usize,
) -> Result<Vec<u8>, DecodeError> {
    #[cfg(fuzzing)]
    {
        // When running under a fuzzer disable brotli decoding and instead just pass through the input data.
        // This allows the fuzzer to more effectively explore code gated behind brotli decoding.
        // TODO(garretrieger): instead consider modifying the top level IFT apis to allow a custom brotli decoder
        //   implementation to be provided. This would allow fuzzing to sub in a custom impl that could return all
        //   of the possible errors that the standard impl here can generate.
        return if encoded.len() <= max_uncompressed_length {
            Ok(encoded.to_vec())
        } else {
            Err(DecodeError::MaxSizeExceeded)
        };
    }

    let decoder = unsafe { BrotliDecoderCreateInstance(None, None, ptr::null_mut()) };
    if decoder.is_null() {
        return Err(DecodeError::InitFailure);
    }

    if let Some(shared_dictionary) = shared_dictionary {
        if unsafe {
            BrotliDecoderAttachDictionary(
                decoder,
                BrotliSharedDictionaryType_BROTLI_SHARED_DICTIONARY_RAW,
                shared_dictionary.len(),
                shared_dictionary.as_ptr(),
            )
        } == BROTLI_FALSE
        {
            unsafe {
                BrotliDecoderDestroyInstance(decoder);
            }
            return Err(DecodeError::InvalidDictionary);
        }
    }

    let mut sink = vec![0u8; max_uncompressed_length];

    let mut next_in = encoded.as_ptr();
    let mut available_in = encoded.len();
    let mut next_out = sink.as_mut_ptr();
    let mut available_out = sink.len();
    let mut total_out = 0;

    let mut error: Option<DecodeError> = None;
    loop {
        let result = unsafe {
            BrotliDecoderDecompressStream(
                decoder,
                &mut available_in,
                &mut next_in,
                &mut available_out,
                &mut next_out,
                &mut total_out,
            )
        };

        #[allow(non_upper_case_globals)]
        match result {
            BrotliDecoderResult_BROTLI_DECODER_RESULT_SUCCESS => break,
            BrotliDecoderResult_BROTLI_DECODER_RESULT_ERROR => {
                error = Some(DecodeError::InvalidStream);
                break;
            }
            BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT if available_in == 0 => {
                // Needs more input and none is available.
                error = Some(DecodeError::InvalidStream);
                break;
            }
            BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_OUTPUT if available_out == 0 => {
                // Needs more output space, but none is available.
                error = Some(DecodeError::MaxSizeExceeded);
                break;
            }
            _ => continue,
        }
    }

    unsafe {
        BrotliDecoderDestroyInstance(decoder);
    }
    if let Some(error) = error {
        return Err(error);
    }

    if available_in > 0 {
        // There's is data left in the input stream, which is not allowed
        return Err(DecodeError::ExcessInputData);
    }

    if total_out > sink.len() {
        return Err(DecodeError::MaxSizeExceeded);
    }

    sink.resize(total_out, 0);

    Ok(sink)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            shared_brotli_decode(&SHARED_DICT_PATCH, Some(BASE.as_bytes()), TARGET.len(),)
        );
    }

    #[test]
    fn brotli_decode_without_shared_dict() {
        let base = "".as_bytes();

        assert_eq!(
            Ok(TARGET.to_vec()),
            shared_brotli_decode(&NO_DICT_PATCH, None, TARGET.len())
        );

        // Check that empty base is handled the same as no base.
        assert_eq!(
            Ok(TARGET.to_vec()),
            shared_brotli_decode(&NO_DICT_PATCH, Some(base), TARGET.len())
        );
    }

    #[test]
    fn brotli_decode_too_little_output() {
        assert_eq!(
            Err(DecodeError::MaxSizeExceeded),
            shared_brotli_decode(&SHARED_DICT_PATCH, Some(BASE.as_bytes()), TARGET.len() - 1)
        );
    }

    #[test]
    fn brotli_decode_excess_output() {
        assert_eq!(
            Ok(TARGET.to_vec()),
            shared_brotli_decode(&SHARED_DICT_PATCH, Some(BASE.as_bytes()), TARGET.len() + 1,)
        );
    }

    #[test]
    fn brotli_decode_too_much_input() {
        let mut patch: Vec<u8> = NO_DICT_PATCH.to_vec();
        patch.push(0u8);

        assert_eq!(
            Err(DecodeError::ExcessInputData),
            shared_brotli_decode(&patch, None, TARGET.len())
        );
    }

    #[test]
    fn brotli_decode_input_missing() {
        // Check what happens if input stream is missing some trailing bytes
        let patch: Vec<u8> = NO_DICT_PATCH[..NO_DICT_PATCH.len() - 1].to_vec();
        assert_eq!(
            Err(DecodeError::InvalidStream),
            shared_brotli_decode(&patch, None, TARGET.len())
        );
    }

    #[test]
    fn brotli_decode_invalid() {
        let patch = [0xFF, 0xFF, 0xFFu8];
        assert_eq!(
            Err(DecodeError::InvalidStream),
            shared_brotli_decode(&patch, None, 10)
        );
    }
}
