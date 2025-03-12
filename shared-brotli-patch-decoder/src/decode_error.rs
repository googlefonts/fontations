use std::io::{self, ErrorKind};

#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    InitFailure,
    InvalidStream(String),
    InvalidDictionary,
    MaxSizeExceeded,
    ExcessInputData,
    IoError(io::ErrorKind),
}

impl DecodeError {
    pub fn from_io_error(err: io::Error) -> Self {
        match err.kind() {
            ErrorKind::OutOfMemory => DecodeError::MaxSizeExceeded,
            ErrorKind::UnexpectedEof => DecodeError::InvalidStream(format!("IOError: {:?}", err)),
            _ => DecodeError::IoError(err.kind()),
        }
    }
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::InitFailure => write!(f, "Failed to initialize the brotli decoder."),
            DecodeError::InvalidStream(msg) => {
                write!(
                    f,
                    "Brotli compressed stream is invalid, decoding failed: {}",
                    msg
                )
            }
            DecodeError::InvalidDictionary => write!(f, "Shared dictionary format is invalid."),
            DecodeError::MaxSizeExceeded => write!(f, "Decompressed size greater than maximum."),
            DecodeError::ExcessInputData => write!(
                f,
                "There is unconsumed data in the input stream after decoding."
            ),
            DecodeError::IoError(kind) => write!(f, "Generic IO error: {}", kind),
        }
    }
}

impl std::error::Error for DecodeError {}
