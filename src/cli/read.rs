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
use crate::hash::{ContentHash, hash_bytes};
use crate::hashline::{LineAnchor, LineHash, show_hashed_lines};
use crate::provider::ProviderRegistry;

mod window;

use window::ReadWindow;

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
        long,
        value_name = "NAME",
        help = "Read one exact symbol (e.g. Class.method), with node identity and line anchors"
    )]
    pub symbol: Option<String>,
    #[arg(
        long,
        value_name = "N",
        help = "Context lines on each side of --symbol (default: 0); never truncates the symbol"
    )]
    pub context: Option<usize>,
    #[arg(
        long,
        value_name = "N",
        help = "First line to read, 1-based (line mode only)"
    )]
    pub offset: Option<usize>,
    #[arg(
        long,
        value_name = "N",
        help = "Maximum lines to read (positive, line mode only)"
    )]
    pub limit: Option<usize>,
    #[arg(
        long = "exclude-kind",
        value_name = "KIND",
        help = "Exclude a node kind (repeatable, ast mode only)"
    )]
    pub exclude_kinds: Vec<String>,
    #[arg(long, help = "Emit structured JSON output")]
    pub json: bool,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    windows: Vec<ReadWindow>,
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
        expected_old_hash: ContentHash,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    Line {
        file: PathBuf,
        line: usize,
        anchor: LineAnchor,
        hash: LineHash,
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
    pub expected_file_hash: ContentHash,
}

pub enum ReadCommandOutput {
    Text(String),
    Json(ReadResponse),
}

pub fn run_read(args: ReadArgs) -> Result<ReadCommandOutput, IdenteditError> {
    validate_read_options(&args)?;

    if args.files.is_empty() {
        if !args.json {
            return Err(IdenteditError::InvalidRequest {
                message: "At least one FILE is required".to_string(),
            });
        }
        if args.mode != ReadMode::Ast {
            return Err(IdenteditError::InvalidRequest {
                message: "--json stdin mode currently supports only --mode ast".to_string(),
            });
        }

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
        let response = super::read_select::run_read_select_from_stdin(args.verbose)?;
        return Ok(ReadCommandOutput::Json(
            ReadResponse::from_read_select_response(response),
        ));
    }

    let provider_registry = ProviderRegistry::default();
    let mut handles = Vec::new();
    let mut file_preconditions = Vec::new();
    let mut windows = Vec::new();
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
                if let Some(symbol) = args.symbol.as_deref() {
                    let source_text = source_utf8(file, &source)?;
                    let handle = super::node_selection::resolve_symbol(
                        file,
                        source_text,
                        &parsed_handles,
                        symbol,
                    )?;
                    windows.push(ReadWindow::symbol(
                        file.clone(),
                        source_text,
                        handle.span,
                        args.context.unwrap_or(0),
                    )?);
                    handles.push(ReadHandle::from_selection_handle(handle, args.verbose));
                } else {
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
            }
            ReadMode::Line => {
                let source_text = source_utf8(file, &source)?;
                let mut lines = show_hashed_lines(source_text);
                if args.offset.is_some() || args.limit.is_some() {
                    let (window, selected) =
                        ReadWindow::page(file.clone(), lines, args.offset.unwrap_or(1), args.limit);
                    windows.push(window);
                    lines = selected;
                }
                handles.extend(lines.into_iter().map(|line| {
                    ReadHandle::Line {
                        file: file.clone(),
                        line: line.line,
                        anchor: LineAnchor::try_new(line.line, line.hash.clone())
                            .expect("hashed source lines always use positive line numbers"),
                        hash: line.hash,
                        text: line.content,
                    }
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
        windows,
    };

    if args.json {
        return Ok(ReadCommandOutput::Json(response));
    }

    Ok(ReadCommandOutput::Text(render_human_readable(
        &response, args.mode,
    )))
}

fn source_utf8<'a>(file: &std::path::Path, source: &'a [u8]) -> Result<&'a str, IdenteditError> {
    std::str::from_utf8(source).map_err(|error| {
        IdenteditError::io(
            file,
            std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        )
    })
}

fn validate_read_options(args: &ReadArgs) -> Result<(), IdenteditError> {
    let filters = args.kind.is_some() || args.name.is_some() || !args.exclude_kinds.is_empty();
    let paging = args.offset.is_some() || args.limit.is_some();
    let message = if args.files.is_empty()
        && (paging || args.symbol.is_some() || args.context.is_some())
    {
        Some(
            "Bounded reads require FILE arguments; --symbol/--context/--offset/--limit are not supported in --json stdin selector mode",
        )
    } else if args.mode == ReadMode::Line
        && (filters || args.symbol.is_some() || args.context.is_some())
    {
        Some(
            "--mode line accepts --offset/--limit, not --kind/--name/--exclude-kind/--symbol/--context; use --mode ast for symbols",
        )
    } else if args.mode == ReadMode::Ast && paging {
        Some(
            "--offset/--limit require --mode line; use --symbol NAME --context N for a complete AST target with context",
        )
    } else if args.offset == Some(0) || args.limit == Some(0) {
        Some("--offset and --limit must be positive; --offset is a 1-based original line number")
    } else if args.symbol.is_some() && filters {
        Some(
            "--symbol cannot be combined with --kind/--name/--exclude-kind; choose one selection method",
        )
    } else if args.context.is_some() && args.symbol.is_none() {
        Some("--context requires --symbol NAME; use --mode line --offset/--limit for a line range")
    } else if args
        .symbol
        .as_deref()
        .is_some_and(|symbol| symbol.trim().is_empty())
    {
        Some("--symbol must not be empty")
    } else {
        None
    };

    if let Some(message) = message {
        return Err(IdenteditError::InvalidRequest {
            message: message.to_string(),
        });
    }
    Ok(())
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
    if !response.windows.is_empty() {
        return window::render_windows(response);
    }
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
            windows: Vec::new(),
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
