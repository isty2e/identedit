use std::path::Path;

use miette::Diagnostic;
use serde::Serialize;
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

    #[error(
        "Multiple targets matched identity '{identity}' in file '{file}' ({candidates} candidates)"
    )]
    AmbiguousTarget {
        identity: String,
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

    pub fn to_error_response(&self) -> ErrorResponse {
        match self {
            Self::NoProvider {
                extension: _,
                supported_extensions,
            } => ErrorResponse {
                error: ErrorBody {
                    r#type: "no_provider".to_string(),
                    message: self.to_string(),
                    suggestion: Some(format!(
                        "Supported extensions: {}",
                        supported_extensions
                            .iter()
                            .map(|extension| format!(".{extension}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                },
            },
            Self::InvalidRequest { .. } | Self::InvalidJsonRequest { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "invalid_request".to_string(),
                    message: self.to_string(),
                    suggestion: None,
                },
            },
            Self::ResourceBusy { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "resource_busy".to_string(),
                    message: self.to_string(),
                    suggestion: Some(
                        "Retry after the current apply operation completes".to_string(),
                    ),
                },
            },
            Self::PathChanged { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "path_changed".to_string(),
                    message: self.to_string(),
                    suggestion: Some(
                        "Re-run 'identedit select' and 'identedit transform', then retry apply".to_string(),
                    ),
                },
            },
            Self::InvalidNamePattern { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "invalid_selector".to_string(),
                    message: self.to_string(),
                    suggestion: Some("Use a valid glob pattern such as 'process_*'".to_string()),
                },
            },
            Self::ParseFailure { .. } | Self::LanguageSetup { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "parse_failure".to_string(),
                    message: self.to_string(),
                    suggestion: None,
                },
            },
            Self::GrammarInstall { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "grammar_install_failed".to_string(),
                    message: self.to_string(),
                    suggestion: None,
                },
            },
            Self::Io { .. } | Self::StdinRead { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "io_error".to_string(),
                    message: self.to_string(),
                    suggestion: None,
                },
            },
            Self::ResponseSerialization { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "serialization_error".to_string(),
                    message: self.to_string(),
                    suggestion: None,
                },
            },
            Self::TargetMissing { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "target_missing".to_string(),
                    message: self.to_string(),
                    suggestion: Some("Re-run 'identedit select' to get updated handles".to_string()),
                },
            },
            Self::AmbiguousTarget { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "ambiguous_target".to_string(),
                    message: self.to_string(),
                    suggestion: Some(
                        "Provide span_hint or refresh handles from 'identedit select'".to_string(),
                    ),
                },
            },
            Self::PreconditionFailed { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "precondition_failed".to_string(),
                    message: self.to_string(),
                    suggestion: Some("Re-run 'identedit select' to get updated handles".to_string()),
                },
            },
            Self::RollbackFailed { .. } => ErrorResponse {
                error: ErrorBody {
                    r#type: "rollback_failed".to_string(),
                    message: self.to_string(),
                    suggestion: Some(
                        "Inspect affected files, manually reconcile rollback failures, then re-run identedit select/transform/apply".to_string(),
                    ),
                },
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub r#type: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::IdenteditError;

    fn assert_error_type(
        error: IdenteditError,
        expected_type: &str,
        expected_suggestion_substring: Option<&str>,
    ) {
        let response = error.to_error_response();
        assert_eq!(response.error.r#type, expected_type);

        match (
            response.error.suggestion.as_deref(),
            expected_suggestion_substring,
        ) {
            (Some(actual), Some(expected_substring)) => {
                assert!(
                    actual.contains(expected_substring),
                    "suggestion should contain '{expected_substring}', got '{actual}'"
                );
            }
            (None, None) => {}
            (actual, expected) => {
                panic!("suggestion mismatch; actual={actual:?}, expected_contains={expected:?}")
            }
        }
    }

    #[test]
    fn no_provider_maps_to_no_provider_with_supported_extensions_suggestion() {
        assert_error_type(
            IdenteditError::NoProvider {
                extension: "<none>".to_string(),
                supported_extensions: vec!["json".to_string(), "py".to_string()],
            },
            "no_provider",
            Some(".json, .py"),
        );
    }

    #[test]
    fn invalid_json_request_maps_to_invalid_request_without_suggestion() {
        let parse_error =
            serde_json::from_str::<serde_json::Value>("{").expect_err("invalid JSON should fail");
        assert_error_type(
            IdenteditError::InvalidJsonRequest {
                source: parse_error,
            },
            "invalid_request",
            None,
        );
    }

    #[test]
    fn io_and_stdin_read_map_to_io_error_without_suggestion() {
        let io_error = std::io::Error::other("boom");
        assert_error_type(
            IdenteditError::Io {
                path: "fixture.py".to_string(),
                source: io_error,
            },
            "io_error",
            None,
        );

        let stdin_error = std::io::Error::other("stdin boom");
        assert_error_type(
            IdenteditError::StdinRead {
                source: stdin_error,
            },
            "io_error",
            None,
        );
    }

    #[test]
    fn transform_target_errors_map_to_specific_api_types() {
        assert_error_type(
            IdenteditError::TargetMissing {
                identity: "id-1".to_string(),
                file: "fixture.py".to_string(),
            },
            "target_missing",
            Some("identedit select"),
        );
        assert_error_type(
            IdenteditError::AmbiguousTarget {
                identity: "id-2".to_string(),
                file: "fixture.py".to_string(),
                candidates: 2,
            },
            "ambiguous_target",
            Some("span_hint"),
        );
        assert_error_type(
            IdenteditError::PreconditionFailed {
                expected_hash: "old".to_string(),
                actual_hash: "new".to_string(),
            },
            "precondition_failed",
            Some("identedit select"),
        );
    }

    #[test]
    fn parse_related_and_lock_related_errors_keep_distinct_response_types() {
        assert_error_type(
            IdenteditError::ParseFailure {
                provider: "tree-sitter-python",
                message: "syntax error".to_string(),
            },
            "parse_failure",
            None,
        );
        assert_error_type(
            IdenteditError::LanguageSetup {
                message: "init error".to_string(),
            },
            "parse_failure",
            None,
        );
        assert_error_type(
            IdenteditError::ResourceBusy {
                path: "fixture.py".to_string(),
            },
            "resource_busy",
            Some("Retry after"),
        );
        assert_error_type(
            IdenteditError::PathChanged {
                path: "fixture.py".to_string(),
            },
            "path_changed",
            Some("identedit select"),
        );
    }

    #[test]
    fn rollback_failed_maps_to_dedicated_error_type_with_recovery_suggestion() {
        assert_error_type(
            IdenteditError::RollbackFailed {
                message: "commit failed after first file".to_string(),
            },
            "rollback_failed",
            Some("manually reconcile rollback failures"),
        );
    }
}
