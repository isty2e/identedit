use std::collections::HashSet;
use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::hash::hash_bytes;
use crate::hashline::{format_line_ref, show_hashed_lines};
use crate::provider::ProviderRegistry;
use crate::selector::Selector;

#[derive(Debug, Args)]
pub struct SelectArgs {
    #[arg(
        long,
        value_enum,
        default_value_t = SelectMode::Ast,
        help = "Selection mode in flag mode (ast|line)"
    )]
    pub mode: SelectMode,
    #[arg(
        long,
        value_name = "KIND",
        help = "Node kind to select (flag mode only)"
    )]
    pub kind: Option<String>,
    #[arg(
        long,
        value_name = "GLOB",
        help = "Glob pattern for symbol names (flag mode only)"
    )]
    pub name: Option<String>,
    #[arg(
        long = "exclude-kind",
        value_name = "KIND",
        help = "Exclude a node kind (repeatable)"
    )]
    pub exclude_kinds: Vec<String>,
    #[arg(long, help = "Read select request JSON from stdin")]
    pub json: bool,
    #[arg(long, help = "Include full matched text in each handle")]
    pub verbose: bool,
    #[arg(
        value_name = "FILE",
        help = "Input file(s) in flag mode; omit when using --json"
    )]
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinSelectRequest {
    command: String,
    #[serde(default)]
    file: Option<PathBuf>,
    #[serde(default)]
    files: Vec<PathBuf>,
    selector: Selector,
}

#[derive(Debug, Serialize)]
pub struct SelectResponse {
    pub handles: Vec<SelectHandle>,
    pub summary: SelectSummary,
    pub file_preconditions: Vec<FilePrecondition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum SelectMode {
    Ast,
    Line,
}

#[derive(Debug, Serialize)]
#[serde(tag = "target_type", rename_all = "snake_case")]
pub enum SelectHandle {
    Node {
        file: PathBuf,
        span: crate::handle::Span,
        kind: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        identity: String,
        expected_old_hash: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Line {
        file: PathBuf,
        line: usize,
        anchor: String,
        hash: String,
        text: String,
    },
}

#[derive(Debug, Serialize)]
pub struct SelectSummary {
    pub files_scanned: usize,
    pub matches: usize,
}

#[derive(Debug, Serialize)]
pub struct FilePrecondition {
    pub file: PathBuf,
    pub expected_file_hash: String,
}

pub fn run_select(args: SelectArgs) -> Result<SelectResponse, IdenteditError> {
    let input = if args.json {
        if !args.files.is_empty() {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "--json mode does not accept positional FILE arguments; provide files via stdin JSON payload"
                        .to_string(),
            });
        }
        if args.mode != SelectMode::Ast {
            return Err(IdenteditError::InvalidRequest {
                message: "--json mode currently supports only --mode ast".to_string(),
            });
        }
        reject_json_mode_selector_flags(&args)?;
        select_input_from_stdin(args.verbose)?
    } else {
        select_input_from_flags(args)?
    };

    let provider_registry = ProviderRegistry::default();
    let mut selected_handles = Vec::new();
    let mut file_preconditions = Vec::new();
    let mut seen_canonical_paths = HashSet::with_capacity(input.files.len());
    #[cfg(unix)]
    let mut seen_file_keys = HashSet::with_capacity(input.files.len());

    for file in &input.files {
        let canonical_path =
            fs::canonicalize(file).map_err(|error| IdenteditError::io(file, error))?;
        if !seen_canonical_paths.insert(canonical_path.clone()) {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Duplicate file entry in select input is not supported: '{}' appears more than once",
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
                        "Duplicate file entry in select input is not supported: '{}' appears more than once",
                        canonical_path.display()
                    ),
                });
            }
        }

        let source = fs::read(file).map_err(|error| IdenteditError::io(file, error))?;
        match &input.mode {
            SelectModeInput::Ast { selector } => {
                let provider = provider_registry.provider_for(file)?;
                let parsed_handles = provider.parse(file, &source)?;
                let filtered_handles = selector.filter(parsed_handles)?;
                selected_handles.extend(
                    filtered_handles
                        .into_iter()
                        .map(|handle| SelectHandle::from_selection_handle(handle, input.verbose)),
                );
            }
            SelectModeInput::Line => {
                let source_text = String::from_utf8(source.clone()).map_err(|error| {
                    IdenteditError::io(
                        file,
                        std::io::Error::new(std::io::ErrorKind::InvalidData, error),
                    )
                })?;
                let lines = show_hashed_lines(&source_text);
                selected_handles.extend(lines.into_iter().map(|line| SelectHandle::Line {
                    file: file.clone(),
                    line: line.line,
                    anchor: format_line_ref(line.line, &line.hash),
                    hash: line.hash,
                    text: line.content,
                }));
            }
        }
        file_preconditions.push(FilePrecondition {
            file: file.clone(),
            expected_file_hash: hash_bytes(&source),
        });
    }

    Ok(SelectResponse {
        summary: SelectSummary {
            files_scanned: input.files.len(),
            matches: selected_handles.len(),
        },
        handles: selected_handles,
        file_preconditions,
    })
}

struct SelectInput {
    files: Vec<PathBuf>,
    mode: SelectModeInput,
    verbose: bool,
}

enum SelectModeInput {
    Ast { selector: Selector },
    Line,
}

fn select_input_from_flags(args: SelectArgs) -> Result<SelectInput, IdenteditError> {
    if args.files.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "FILE is required unless --json mode is enabled".to_string(),
        });
    }
    match args.mode {
        SelectMode::Ast => {
            let kind = args.kind.ok_or_else(|| IdenteditError::InvalidRequest {
                message: "--kind is required in --mode ast unless --json mode is enabled"
                    .to_string(),
            })?;
            Ok(SelectInput {
                files: args.files,
                mode: SelectModeInput::Ast {
                    selector: Selector {
                        kind,
                        name_pattern: args.name,
                        exclude_kinds: args.exclude_kinds,
                    },
                },
                verbose: args.verbose,
            })
        }
        SelectMode::Line => {
            if args.kind.is_some() || args.name.is_some() || !args.exclude_kinds.is_empty() {
                return Err(IdenteditError::InvalidRequest {
                    message:
                        "--mode line does not accept --kind/--name/--exclude-kind selector filters"
                            .to_string(),
                });
            }
            Ok(SelectInput {
                files: args.files,
                mode: SelectModeInput::Line,
                verbose: args.verbose,
            })
        }
    }
}

fn select_input_from_stdin(verbose: bool) -> Result<SelectInput, IdenteditError> {
    let mut request_body = String::new();
    std::io::stdin()
        .read_to_string(&mut request_body)
        .map_err(|error| IdenteditError::StdinRead { source: error })?;

    let request: StdinSelectRequest = serde_json::from_str(&request_body)
        .map_err(|error| IdenteditError::InvalidJsonRequest { source: error })?;

    if request.command != "select" {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Unsupported command '{}' in stdin JSON mode; expected 'select'",
                request.command
            ),
        });
    }

    let files = files_from_stdin_request(request.file, request.files)?;
    Ok(SelectInput {
        files,
        mode: SelectModeInput::Ast {
            selector: request.selector,
        },
        verbose,
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

fn reject_json_mode_selector_flags(args: &SelectArgs) -> Result<(), IdenteditError> {
    if args.kind.is_some() {
        return Err(IdenteditError::InvalidRequest {
            message:
                "--json mode does not accept --kind; provide selector.kind in stdin JSON payload"
                    .to_string(),
        });
    }
    if args.name.is_some() {
        return Err(IdenteditError::InvalidRequest {
            message:
                "--json mode does not accept --name; provide selector.name_pattern in stdin JSON payload"
                    .to_string(),
        });
    }
    if !args.exclude_kinds.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "--json mode does not accept --exclude-kind; provide selector.exclude_kinds in stdin JSON payload"
                .to_string(),
        });
    }

    Ok(())
}

impl SelectHandle {
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
        Self::Node {
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
