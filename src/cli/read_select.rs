use std::collections::HashSet;
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::hash::hash_bytes;
use crate::provider::ProviderRegistry;
use crate::selector::Selector;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinReadRequest {
    command: String,
    #[serde(default)]
    file: Option<PathBuf>,
    #[serde(default)]
    files: Vec<PathBuf>,
    selector: Selector,
}

#[derive(Debug, Serialize)]
pub struct ReadSelectResponse {
    pub handles: Vec<ReadSelectHandle>,
    pub summary: ReadSelectSummary,
    pub file_preconditions: Vec<FilePrecondition>,
}

#[derive(Debug, Serialize)]
pub struct ReadSelectHandle {
    pub file: PathBuf,
    pub span: crate::handle::Span,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub identity: String,
    pub expected_old_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReadSelectSummary {
    pub files_scanned: usize,
    pub matches: usize,
}

#[derive(Debug, Serialize)]
pub struct FilePrecondition {
    pub file: PathBuf,
    pub expected_file_hash: String,
}

pub fn run_read_select_from_stdin(verbose: bool) -> Result<ReadSelectResponse, IdenteditError> {
    let mut request_body = String::new();
    std::io::stdin()
        .read_to_string(&mut request_body)
        .map_err(|error| IdenteditError::StdinRead { source: error })?;

    let request: StdinReadRequest = serde_json::from_str(&request_body)
        .map_err(|error| IdenteditError::InvalidJsonRequest { source: error })?;

    if request.command != "read" {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Unsupported command '{}' in stdin JSON mode; expected 'read'",
                request.command
            ),
        });
    }

    let files = files_from_stdin_request(request.file, request.files)?;

    let provider_registry = ProviderRegistry::default();
    let mut selected_handles = Vec::new();
    let mut file_preconditions = Vec::new();
    let mut seen_canonical_paths = HashSet::with_capacity(files.len());
    #[cfg(unix)]
    let mut seen_file_keys = HashSet::with_capacity(files.len());

    for file in &files {
        let canonical_path =
            fs::canonicalize(file).map_err(|error| IdenteditError::io(file, error))?;
        if !seen_canonical_paths.insert(canonical_path.clone()) {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Duplicate file entry in read input is not supported: '{}' appears more than once",
                    canonical_path.display()
                ),
            });
        }
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&canonical_path)
                .map_err(|error| IdenteditError::io(&canonical_path, error))?;
            let file_key = (metadata.dev(), metadata.ino());
            if !seen_file_keys.insert(file_key) {
                return Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "Duplicate file entry in read input is not supported: '{}' appears more than once",
                        canonical_path.display()
                    ),
                });
            }
        }

        let source = fs::read(file).map_err(|error| IdenteditError::io(file, error))?;
        let provider = provider_registry.provider_for(file)?;
        let parsed_handles = provider.parse(file, &source)?;
        let filtered_handles = request.selector.filter(parsed_handles)?;
        selected_handles.extend(
            filtered_handles
                .into_iter()
                .map(|handle| ReadSelectHandle::from_selection_handle(handle, verbose)),
        );

        file_preconditions.push(FilePrecondition {
            file: file.clone(),
            expected_file_hash: hash_bytes(&source),
        });
    }

    Ok(ReadSelectResponse {
        summary: ReadSelectSummary {
            files_scanned: files.len(),
            matches: selected_handles.len(),
        },
        handles: selected_handles,
        file_preconditions,
    })
}

fn files_from_stdin_request(
    file: Option<PathBuf>,
    files: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, IdenteditError> {
    if let Some(single_file) = file {
        if !files.is_empty() {
            return Err(IdenteditError::InvalidRequest {
                message: "Provide either 'file' or 'files' in stdin JSON mode, not both"
                    .to_string(),
            });
        }

        return Ok(vec![single_file]);
    }

    if files.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "Either 'file' or a non-empty 'files' array is required in stdin JSON mode"
                .to_string(),
        });
    }

    Ok(files)
}

impl ReadSelectHandle {
    fn from_selection_handle(handle: SelectionHandle, verbose: bool) -> Self {
        let SelectionHandle {
            file,
            span,
            kind,
            name,
            identity,
            expected_old_hash,
            text,
        } = handle;

        Self {
            file,
            span,
            kind,
            name,
            identity,
            expected_old_hash,
            text: if verbose { Some(text) } else { None },
        }
    }
}
