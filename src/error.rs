use std::path::Path;

use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum IdenteditError {
    #[error("Failed to read file '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read stdin: {source}")]
    StdinRead {
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse stdin JSON request: {source}")]
    InvalidJsonRequest {
        #[source]
        source: serde_json::Error,
    },

    #[error("Failed to serialize response JSON: {source}")]
    ResponseSerialization {
        #[source]
        source: serde_json::Error,
    },

    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },

    #[error("File '{path}' is busy: another apply operation is in progress")]
    ResourceBusy { path: String },

    #[error("File '{path}' changed during apply; retry with a fresh selection")]
    PathChanged { path: String },

    #[error("No structure provider available for extension '{extension}'")]
    NoProvider {
        extension: String,
        supported_extensions: Vec<String>,
    },

    #[error("Tree-sitter language initialization failed: {message}")]
    LanguageSetup { message: String },

    #[error("Grammar install failed: {message}")]
    GrammarInstall { message: String },

    #[error("Provider '{provider}' failed to parse input: {message}")]
    ParseFailure {
        provider: &'static str,
        message: String,
    },

    #[error("Invalid selector glob pattern '{pattern}': {message}")]
    InvalidNamePattern { pattern: String, message: String },

    #[error("No target matched identity '{identity}' in file '{file}'")]
    TargetMissing { identity: String, file: String },

    #[error("No target matched selector '{selector}' in file '{file}'")]
    TargetMissingSelector { selector: String, file: String },

    #[error(
        "Multiple targets matched identity '{identity}' in file '{file}' ({candidates} candidates)"
    )]
    AmbiguousTarget {
        identity: String,
        file: String,
        candidates: usize,
    },

    #[error(
        "Multiple targets matched selector '{selector}' in file '{file}' ({candidates} candidates)"
    )]
    AmbiguousTargetSelector {
        selector: String,
        file: String,
        candidates: usize,
    },

    #[error(
        "Target node has changed since selection. Expected hash '{expected_hash}', got '{actual_hash}'"
    )]
    PreconditionFailed {
        expected_hash: String,
        actual_hash: String,
    },

    #[error("Commit failed and rollback did not fully succeed: {message}")]
    RollbackFailed { message: String },
}

impl IdenteditError {
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            source,
        }
    }
}
