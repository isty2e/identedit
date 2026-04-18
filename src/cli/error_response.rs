use serde::Serialize;

use crate::error::IdenteditError;

const SERIALIZATION_FALLBACK: &str =
    "{\"error\":{\"type\":\"serialization_error\",\"message\":\"Failed to serialize error response\"}}";

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    r#type: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
}

pub fn render_error_response(error: &IdenteditError) -> String {
    serde_json::to_string_pretty(&error_response(error))
        .unwrap_or_else(|_| SERIALIZATION_FALLBACK.to_string())
}

fn error_response(error: &IdenteditError) -> ErrorResponse {
    match error {
        IdenteditError::NoProvider {
            extension: _,
            supported_extensions,
        } => ErrorResponse {
            error: ErrorBody {
                r#type: "no_provider".to_string(),
                message: error.to_string(),
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
        IdenteditError::InvalidRequest { .. } | IdenteditError::InvalidJsonRequest { .. } => {
            ErrorResponse {
                error: ErrorBody {
                    r#type: "invalid_request".to_string(),
                    message: error.to_string(),
                    suggestion: None,
                },
            }
        }
        IdenteditError::ResourceBusy { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "resource_busy".to_string(),
                message: error.to_string(),
                suggestion: Some("Retry after the current apply operation completes".to_string()),
            },
        },
        IdenteditError::PathChanged { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "path_changed".to_string(),
                message: error.to_string(),
                suggestion: Some(
                    "Re-run 'identedit read' and 'identedit edit', then retry apply".to_string(),
                ),
            },
        },
        IdenteditError::InvalidNamePattern { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "invalid_selector".to_string(),
                message: error.to_string(),
                suggestion: Some("Use a valid glob pattern such as 'process_*'".to_string()),
            },
        },
        IdenteditError::ParseFailure { .. } | IdenteditError::LanguageSetup { .. } => {
            ErrorResponse {
                error: ErrorBody {
                    r#type: "parse_failure".to_string(),
                    message: error.to_string(),
                    suggestion: None,
                },
            }
        }
        IdenteditError::GrammarInstall { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "grammar_install_failed".to_string(),
                message: error.to_string(),
                suggestion: None,
            },
        },
        IdenteditError::Io { .. } | IdenteditError::StdinRead { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "io_error".to_string(),
                message: error.to_string(),
                suggestion: None,
            },
        },
        IdenteditError::ResponseSerialization { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "serialization_error".to_string(),
                message: error.to_string(),
                suggestion: None,
            },
        },
        IdenteditError::TargetMissing { .. } | IdenteditError::TargetMissingSelector { .. } => {
            ErrorResponse {
                error: ErrorBody {
                    r#type: "target_missing".to_string(),
                    message: error.to_string(),
                    suggestion: Some(
                        "Re-run 'identedit read' to inspect current handles".to_string(),
                    ),
                },
            }
        }
        IdenteditError::AmbiguousTarget { .. }
        | IdenteditError::AmbiguousTargetSelector { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "ambiguous_target".to_string(),
                message: error.to_string(),
                suggestion: Some(
                    "Use a more specific selector or re-run 'identedit read' to disambiguate"
                        .to_string(),
                ),
            },
        },
        IdenteditError::PreconditionFailed { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "precondition_failed".to_string(),
                message: error.to_string(),
                suggestion: Some("Re-run 'identedit read' to get updated handles".to_string()),
            },
        },
        IdenteditError::RollbackFailed { .. } => ErrorResponse {
            error: ErrorBody {
                r#type: "rollback_failed".to_string(),
                message: error.to_string(),
                suggestion: Some(
                    "Inspect affected files, manually reconcile rollback failures, then re-run identedit read/edit/apply"
                        .to_string(),
                ),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::error_response;
    use crate::error::IdenteditError;

    fn assert_error_type(
        error: IdenteditError,
        expected_type: &str,
        expected_suggestion_substring: Option<&str>,
    ) {
        let response = error_response(&error);
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
            Some("identedit read"),
        );
        assert_error_type(
            IdenteditError::AmbiguousTarget {
                identity: "id-2".to_string(),
                file: "fixture.py".to_string(),
                candidates: 2,
            },
            "ambiguous_target",
            Some("more specific selector"),
        );
        assert_error_type(
            IdenteditError::PreconditionFailed {
                expected_hash: "old".to_string(),
                actual_hash: "new".to_string(),
            },
            "precondition_failed",
            Some("identedit read"),
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
            Some("identedit read"),
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
