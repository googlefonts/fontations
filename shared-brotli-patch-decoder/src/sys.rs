// Note: These manual bindings and the `cc` build dependency are a temporary measure.
// We can revert to using `brotlic-sys` if https://github.com/AronParker/brotlic/pull/5
// is ever merged.

#![allow(non_camel_case_types)]
#![allow(dead_code)]
#![allow(non_upper_case_globals)]

use std::os::raw::{c_int, c_void};

pub type BrotliDecoderState = c_void;

// From brotli/c/include/brotli/decode.h
pub type BrotliDecoderResult = c_int;
pub const BrotliDecoderResult_BROTLI_DECODER_RESULT_ERROR: BrotliDecoderResult = 0;
pub const BrotliDecoderResult_BROTLI_DECODER_RESULT_SUCCESS: BrotliDecoderResult = 1;
pub const BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT: BrotliDecoderResult = 2;
pub const BrotliDecoderResult_BROTLI_DECODER_RESULT_NEEDS_MORE_OUTPUT: BrotliDecoderResult = 3;

// From brotli/c/include/brotli/shared_dictionary.h
pub type BrotliSharedDictionaryType = c_int;
pub const BrotliSharedDictionaryType_BROTLI_SHARED_DICTIONARY_RAW: BrotliSharedDictionaryType = 0;
pub const BrotliSharedDictionaryType_BROTLI_SHARED_DICTIONARY_SERIALIZED:
    BrotliSharedDictionaryType = 1;

// From brotli/c/include/brotli/types.h
pub type BROTLI_BOOL = c_int;
pub const BROTLI_TRUE: BROTLI_BOOL = 1;
pub const BROTLI_FALSE: BROTLI_BOOL = 0;

// From brotli/c/include/brotli/types.h
pub type brotli_alloc_func =
    Option<unsafe extern "C" fn(opaque: *mut c_void, size: usize) -> *mut c_void>;
pub type brotli_free_func = Option<unsafe extern "C" fn(opaque: *mut c_void, address: *mut c_void)>;

// Functions from brotli/c/include/brotli/decode.h
extern "C" {
    pub fn BrotliDecoderCreateInstance(
        alloc_func: brotli_alloc_func,
        free_func: brotli_free_func,
        opaque: *mut c_void,
    ) -> *mut BrotliDecoderState;

    pub fn BrotliDecoderAttachDictionary(
        state: *mut BrotliDecoderState,
        type_: BrotliSharedDictionaryType,
        data_size: usize,
        data: *const u8,
    ) -> BROTLI_BOOL;

    pub fn BrotliDecoderDecompressStream(
        state: *mut BrotliDecoderState,
        available_in: *mut usize,
        next_in: *mut *const u8,
        available_out: *mut usize,
        next_out: *mut *mut u8,
        total_out: *mut usize,
    ) -> BrotliDecoderResult;

    pub fn BrotliDecoderDestroyInstance(state: *mut BrotliDecoderState);
}
