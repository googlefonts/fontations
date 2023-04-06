//! Errors that occur during writing

use crate::{graph::Graph, validate::ValidationReport};

/// A packing could not be found that satisfied all offsets
#[derive(Clone, Debug)]
pub struct PackingError {
    pub(crate) graph: std::rc::Rc<Graph>,
}

/// An error occured while writing this table
#[derive(Debug)]
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

impl std::error::Error for PackingError {}
impl std::error::Error for Error {}
