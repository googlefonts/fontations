//! Sanitization context, options, and callbacks.

use font_types::Tag;

use crate::error::MessageLevel;

/// Action to take for a specific font table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TableAction {
    /// Use the sanitizer's default action for the table.
    #[default]
    Default,
    /// Parse, validate, and sanitize the table.
    Sanitize,
    /// Pass the raw table data through without sanitizing.
    PassThru,
    /// Drop the table from the sanitized output font.
    Drop,
    /// Sanitize the table, but drop it if it fails instead of failing the whole font.
    SanitizeSoft,
}

/// A context for logging messages and configuring table actions during sanitization.
pub trait SanitizeContext {
    /// Called when the sanitizer emits an informational warning or error message.
    fn message(&mut self, level: MessageLevel, message: &str);

    /// Called to determine what action to take for a table with the given `tag`.
    fn get_table_action(&mut self, tag: Tag) -> TableAction {
        let _ = tag;
        TableAction::Default
    }
}

/// A standard context implementation that collects messages and provides default table actions.
#[derive(Debug, Clone)]
pub struct DefaultContext {
    /// Log of messages emitted during sanitization.
    pub messages: Vec<(MessageLevel, String)>,
    /// Fallback action for known standard OpenType tables that are not yet explicitly sanitized.
    pub unimplemented_action: TableAction,
}

impl Default for DefaultContext {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            unimplemented_action: TableAction::PassThru,
        }
    }
}

impl DefaultContext {
    /// Create a new default context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the fallback action for unimplemented standard tables.
    pub fn with_unimplemented_action(mut self, action: TableAction) -> Self {
        self.unimplemented_action = action;
        self
    }
}

impl SanitizeContext for DefaultContext {
    fn message(&mut self, level: MessageLevel, message: &str) {
        self.messages.push((level, message.to_string()));
    }

    fn get_table_action(&mut self, _tag: Tag) -> TableAction {
        TableAction::Default
    }
}

/// An empty context that ignores all messages.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullContext;

impl SanitizeContext for NullContext {
    fn message(&mut self, _level: MessageLevel, _message: &str) {}
}
