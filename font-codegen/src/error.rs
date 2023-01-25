use std::path::Path;

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceOffset};

#[derive(Debug)]
pub struct ErrorReport {
    src: Option<NamedSource>,
    message: String,
    location: Option<LabeledSpan>,
}

impl Diagnostic for ErrorReport {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.src.as_ref().map(|x| x as _)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        self.location
            .as_ref()
            .map(|loc| Box::new(std::iter::once(loc.clone())) as _)
    }
}

impl std::fmt::Display for ErrorReport {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ErrorReport {}

impl ErrorReport {
    pub fn message(message: impl Into<String>) -> Self {
        ErrorReport {
            src: None,
            message: message.into(),
            location: None,
        }
    }

    pub fn from_error_src(error: &syn::Error, path: &Path, text: String) -> Self {
        let message = error.to_string();
        let span = error.span();
        let start = span.start();
        // we add + 1 to these offsets because of weird upstream behaviour I'm too lazy
        // to try and land a fix for. If spans are off-by-one, delete these + 1s :)
        let start = SourceOffset::from_location(&text, start.line, start.column + 1);
        let end = span.end();
        let end = SourceOffset::from_location(&text, end.line, end.column + 1);
        let start_off = start.offset();
        let len = end.offset() - start_off;
        let location = LabeledSpan::new(Some(message), start_off, len);
        let src = NamedSource::new(path.to_string_lossy(), text);
        ErrorReport {
            message: "parsing failed".into(),
            src: Some(src),
            location: Some(location),
        }
    }
}
