use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::IdenteditError;
use crate::failed_diff::{
    FailedDiffAnalysis, FailedDiffError, analyze_failed_diff, parse_failed_diff,
};

use super::EditIntentArgs;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct FailedDiffResponse {
    pub(crate) mode: &'static str,
    pub(crate) file: PathBuf,
    pub(crate) preview_only: bool,
    #[serde(flatten)]
    pub(crate) analysis: FailedDiffAnalysis,
}

pub(crate) fn prepare_failed_diff_handoff(
    args: &EditIntentArgs,
) -> Result<FailedDiffResponse, IdenteditError> {
    let diff_path = args
        .from_diff
        .as_deref()
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "Internal CLI error: failed-diff handoff requires --from-diff.".to_string(),
        })?;
    if args.has_ordinary_intent_arguments() {
        return Err(IdenteditError::InvalidRequest {
            message:
                "--from-diff cannot be combined with target, operation, text-source, or config flags."
                    .to_string(),
        });
    }

    let diff_text = read_diff_input(diff_path)?;
    let parsed = parse_failed_diff(&diff_text).map_err(map_failed_diff_error)?;
    let file = resolve_source_file(args.file.as_deref(), parsed.header_file.as_deref())?;
    let source = fs::read_to_string(&file).map_err(|error| IdenteditError::io(&file, error))?;
    let analysis = analyze_failed_diff(&source, parsed).map_err(map_failed_diff_error)?;

    Ok(FailedDiffResponse {
        mode: "failed_diff_handoff",
        file,
        preview_only: true,
        analysis,
    })
}

fn read_diff_input(path: &Path) -> Result<String, IdenteditError> {
    if path == Path::new("-") {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|source| IdenteditError::StdinRead { source })?;
        Ok(input)
    } else {
        fs::read_to_string(path).map_err(|error| IdenteditError::io(path, error))
    }
}

fn resolve_source_file(
    explicit_file: Option<&Path>,
    header_file: Option<&str>,
) -> Result<PathBuf, IdenteditError> {
    match (explicit_file, header_file) {
        (Some(explicit), Some(header)) => {
            let header = Path::new(header);
            if !same_existing_file(explicit, header) {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Explicit FILE '{}' conflicts with diff header path '{}'.",
                        explicit.display(),
                        header.display()
                    ),
                });
            }
            Ok(explicit.to_path_buf())
        }
        (Some(explicit), None) => Ok(explicit.to_path_buf()),
        (None, Some(header)) => Ok(PathBuf::from(header)),
        (None, None) => Err(IdenteditError::InvalidRequest {
            message: "FILE is required when --from-diff input has no supported file header."
                .to_string(),
        }),
    }
}

fn same_existing_file(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn map_failed_diff_error(error: FailedDiffError) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: error.to_string(),
    }
}
