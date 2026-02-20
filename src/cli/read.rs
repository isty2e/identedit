use std::collections::{BTreeMap, HashSet};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use glob::Pattern;
use serde::Serialize;

use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::hash::hash_bytes;
use crate::hashline::{format_line_ref, show_hashed_lines};
use crate::provider::ProviderRegistry;

#[derive(Debug, Args)]
pub struct ReadArgs {
    #[arg(
        long,
        value_enum,
        default_value_t = ReadMode::Ast,
        help = "Read mode (ast|line)"
    )]
    pub mode: ReadMode,
    #[arg(
        long,
        value_name = "KIND",
        help = "Optional node kind filter (ast mode only)"
    )]
    pub kind: Option<String>,
    #[arg(
        long,
        value_name = "GLOB",
        help = "Optional glob pattern for symbol names (ast mode only)"
    )]
    pub name: Option<String>,
    #[arg(
        long = "exclude-kind",
        value_name = "KIND",
        help = "Exclude a node kind (repeatable, ast mode only)"
    )]
    pub exclude_kinds: Vec<String>,
    #[arg(long, help = "Emit structured JSON output")]
    #[arg(action = clap::ArgAction::Count)]
    pub json: u8,
    #[arg(long, help = "Include full matched text in ast mode output")]
    pub verbose: bool,
    #[arg(
        value_name = "FILE",
        num_args = 0..,
        help = "Input files; omit when using --json stdin mode"
    )]
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum ReadMode {
    Ast,
    Line,
}

#[derive(Debug, Serialize)]
pub struct ReadResponse {
    pub handles: Vec<ReadHandle>,
    pub summary: ReadSummary,
    pub file_preconditions: Vec<FilePrecondition>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "target_type", rename_all = "snake_case")]
pub enum ReadHandle {
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
pub struct ReadSummary {
    pub files_scanned: usize,
    pub matches: usize,
}

#[derive(Debug, Serialize)]
pub struct FilePrecondition {
    pub file: PathBuf,
    pub expected_file_hash: String,
}

pub enum ReadCommandOutput {
    Text(String),
    Json(ReadResponse),
}

pub fn run_read(args: ReadArgs) -> Result<ReadCommandOutput, IdenteditError> {
    if args.json > 1 && !args.files.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message:
                "--json stdin mode does not allow positional FILE arguments; provide file paths inside the JSON payload"
                    .to_string(),
        });
    }

    if args.files.is_empty() {
        if args.json == 0 {
            return Err(IdenteditError::InvalidRequest {
                message: "At least one FILE is required".to_string(),
            });
        }
        if args.mode != ReadMode::Ast {
            return Err(IdenteditError::InvalidRequest {
                message: "--json stdin mode currently supports only --mode ast".to_string(),
            });
        }

        if args.json > 1 {
            if args.kind.is_some() {
                return Err(IdenteditError::InvalidRequest {
                    message:
                        "--json stdin mode does not allow --kind; encode selector.kind in the JSON payload"
                            .to_string(),
                });
            }
            if args.name.is_some() {
                return Err(IdenteditError::InvalidRequest {
                    message:
                        "--json stdin mode does not allow --name; encode selector.name_pattern in the JSON payload"
                            .to_string(),
                });
            }
            if !args.exclude_kinds.is_empty() {
                return Err(IdenteditError::InvalidRequest {
                    message:
                        "--json stdin mode does not allow --exclude-kind; encode selector.exclude_kinds in the JSON payload"
                            .to_string(),
                });
            }
        }
        let response = super::read_select::run_read_select_from_stdin(args.verbose)?;
        return Ok(ReadCommandOutput::Json(ReadResponse::from_read_select_response(
            response,
        )));
    }

    let provider_registry = ProviderRegistry::default();
    let mut handles = Vec::new();
    let mut file_preconditions = Vec::new();
    let mut seen_canonical_paths = HashSet::with_capacity(args.files.len());
    #[cfg(unix)]
    let mut seen_file_keys = HashSet::with_capacity(args.files.len());

    let compiled_name_pattern =
        args.name
            .as_deref()
            .map(Pattern::new)
            .transpose()
            .map_err(|error| IdenteditError::InvalidNamePattern {
                pattern: args.name.clone().unwrap_or_default(),
                message: error.msg.to_string(),
            })?;

    for file in &args.files {
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
        match args.mode {
            ReadMode::Ast => {
                let provider = provider_registry.provider_for(file)?;
                let parsed_handles = provider.parse(file, &source)?;
                let filtered_handles = filter_ast_handles(
                    parsed_handles,
                    args.kind.as_deref(),
                    compiled_name_pattern.as_ref(),
                    &args.exclude_kinds,
                );
                handles.extend(
                    filtered_handles
                        .into_iter()
                        .map(|handle| ReadHandle::from_selection_handle(handle, args.verbose)),
                );
            }
            ReadMode::Line => {
                if args.kind.is_some() || args.name.is_some() || !args.exclude_kinds.is_empty() {
                    return Err(IdenteditError::InvalidRequest {
                        message: "--mode line does not accept --kind/--name/--exclude-kind filters"
                            .to_string(),
                    });
                }
                let source_text = String::from_utf8(source.clone()).map_err(|error| {
                    IdenteditError::io(
                        file,
                        std::io::Error::new(std::io::ErrorKind::InvalidData, error),
                    )
                })?;
                let lines = show_hashed_lines(&source_text);
                handles.extend(lines.into_iter().map(|line| ReadHandle::Line {
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

    let response = ReadResponse {
        summary: ReadSummary {
            files_scanned: args.files.len(),
            matches: handles.len(),
        },
        handles,
        file_preconditions,
    };

    if args.json > 0 {
        return Ok(ReadCommandOutput::Json(response));
    }

    Ok(ReadCommandOutput::Text(render_human_readable(
        &response, args.mode,
    )))
}

fn filter_ast_handles(
    handles: Vec<SelectionHandle>,
    kind_filter: Option<&str>,
    name_pattern: Option<&Pattern>,
    exclude_kinds: &[String],
) -> Vec<SelectionHandle> {
    handles
        .into_iter()
        .filter(|handle| {
            if exclude_kinds
                .iter()
                .any(|excluded_kind| excluded_kind == &handle.kind)
            {
                return false;
            }

            if let Some(kind) = kind_filter
                && handle.kind != kind
            {
                return false;
            }

            if let Some(pattern) = name_pattern {
                return handle
                    .name
                    .as_deref()
                    .is_some_and(|symbol_name| pattern.matches(symbol_name));
            }

            true
        })
        .collect()
}

fn render_human_readable(response: &ReadResponse, mode: ReadMode) -> String {
    match mode {
        ReadMode::Ast => render_ast_text(&response.handles),
        ReadMode::Line => render_line_text(&response.handles),
    }
}

fn render_ast_text(handles: &[ReadHandle]) -> String {
    let mut grouped = BTreeMap::<String, Vec<&ReadHandle>>::new();
    for handle in handles {
        if let ReadHandle::Node { file, .. } = handle {
            grouped
                .entry(file.display().to_string())
                .or_default()
                .push(handle);
        }
    }

    if grouped.is_empty() {
        return "(no matches)".to_string();
    }

    let mut sections = Vec::with_capacity(grouped.len());
    for (file, file_handles) in grouped {
        let mut section = Vec::with_capacity(file_handles.len() + 1);
        section.push(format!("## {file}"));
        for handle in file_handles {
            if let ReadHandle::Node {
                span,
                kind,
                name,
                identity,
                text,
                ..
            } = handle
            {
                let name_text = name.as_deref().unwrap_or("-");
                section.push(format!(
                    "{identity} {kind} {name_text} [{}..{})",
                    span.start, span.end
                ));
                if let Some(body) = text {
                    for line in body.lines() {
                        section.push(format!("    {line}"));
                    }
                }
            }
        }
        sections.push(section.join("\n"));
    }

    sections.join("\n\n")
}

fn render_line_text(handles: &[ReadHandle]) -> String {
    let mut grouped = BTreeMap::<String, Vec<&ReadHandle>>::new();
    for handle in handles {
        if let ReadHandle::Line { file, .. } = handle {
            grouped
                .entry(file.display().to_string())
                .or_default()
                .push(handle);
        }
    }

    if grouped.is_empty() {
        return "(no matches)".to_string();
    }

    let include_headers = grouped.len() > 1;
    let mut sections = Vec::with_capacity(grouped.len());
    for (file, file_handles) in grouped {
        let mut lines = Vec::new();
        if include_headers {
            lines.push(format!("## {file}"));
        }
        for handle in file_handles {
            if let ReadHandle::Line {
                line, hash, text, ..
            } = handle
            {
                lines.push(format!("{line}:{hash}|{text}"));
            }
        }
        sections.push(lines.join("\n"));
    }

    sections.join("\n\n")
}

impl ReadHandle {
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

impl ReadResponse {
    fn from_read_select_response(response: super::read_select::ReadSelectResponse) -> Self {
        let handles = response
            .handles
            .into_iter()
            .map(ReadHandle::from_read_select_handle)
            .collect();
        let summary = ReadSummary {
            files_scanned: response.summary.files_scanned,
            matches: response.summary.matches,
        };
        let file_preconditions = response
            .file_preconditions
            .into_iter()
            .map(|item| FilePrecondition {
                file: item.file,
                expected_file_hash: item.expected_file_hash,
            })
            .collect();
        Self {
            handles,
            summary,
            file_preconditions,
        }
    }
}

impl ReadHandle {
    fn from_read_select_handle(handle: super::read_select::ReadSelectHandle) -> Self {
        let super::read_select::ReadSelectHandle {
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
            text,
        }
    }
}
