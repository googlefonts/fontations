use crate::decode_error::DecodeError;
use brotlic_sys::{
    BrotliDecoderAttachDictionary, BrotliDecoderCreateInstance, BrotliDecoderDecompressStream,
    BrotliDecoderDestroyInstance, BrotliDecoderResult_BROTLI_DECODER_RESULT_ERROR,
    BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT,
    BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_OUTPUT,
    BrotliDecoderResult_BROTLI_DECODER_RESULT_SUCCESS,
    BrotliSharedDictionaryType_BROTLI_SHARED_DICTIONARY_RAW, BROTLI_FALSE,
};
use core::ptr;

pub fn shared_brotli_decode_c(
    encoded: &[u8],
    shared_dictionary: Option<&[u8]>,
    max_uncompressed_length: usize,
) -> Result<Vec<u8>, DecodeError> {
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
                error = Some(DecodeError::InvalidStream(
                    "BrotliDecoderResult_BROTLI_DECODER_RESULT_ERROR".to_string(),
                ));
                break;
            }
            BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT if available_in == 0 => {
                // Needs more input and none is available.
                error = Some(DecodeError::InvalidStream(
                    "BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT".to_string(),
                ));
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
