//! Errors that occur during writing

use std::sync::Arc;

use crate::{graph::Graph, validate::ValidationReport};

/// A packing could not be found that satisfied all offsets
#[derive(Clone)]
pub struct PackingError {
    // this is Arc so that we can clone and still be sent between threads.
    pub(crate) graph: Arc<Graph>,
}

/// An error occured while writing this table
#[derive(Debug, Clone)]
pub enum Error {
    ValidationFailed(ValidationReport),
    PackingFailed(PackingError),
}

impl PackingError {
    /// Write a graphviz file representing the failed packing to the provided path.
    ///
    /// Has the same semantics as [`std::fs::write`].
    #[cfg(feature = "dot2")]
    pub fn write_graph_viz(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        self.graph.write_graph_viz(path)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ValidationFailed(report) => report.fmt(f),
            Error::PackingFailed(error) => error.fmt(f),
        }
    }
}

impl std::fmt::Display for PackingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Table packing failed with {} overflows",
            self.graph.find_overflows().len()
        )
    }
}

impl std::fmt::Debug for PackingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl std::error::Error for PackingError {}
impl std::error::Error for Error {}

impl From<ValidationReport> for Error {
    fn from(value: ValidationReport) -> Self {
        Error::ValidationFailed(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Some users, notably fontmake-rs, like Send errors.
    #[test]
    fn assert_compiler_error_is_send() {
        fn send_me_baby<T: Send>() {}
        send_me_baby::<Error>();
    }
}
