//! Support for scaling CFF outlines.

// Temporary until new scaler API is done.
#![allow(dead_code)]

mod hint;
mod scaler;

pub(crate) use scaler::{Scaler, Subfont};
