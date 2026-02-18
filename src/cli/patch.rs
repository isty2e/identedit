use std::io::Read;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::Deserialize;
use serde_json::Value;

use crate::apply::apply_multi_file_changeset;
use crate::changeset::{MultiFileChangeset, OpKind, TransformTarget, hash_text};
use crate::cli::apply::shape_apply_response;
use crate::error::IdenteditError;
use crate::handle::Span;
use crate::hash::{HASH_HEX_LEN, hash_bytes};
use crate::hashline::{HASHLINE_PUBLIC_HEX_LEN, parse_line_ref};
use crate::hashline::{HashlineEdit, InsertAfterEdit, ReplaceLinesEdit, SetLineEdit};
use crate::patch::config_path::{ConfigPathOperation, resolve_config_path_operation};
use crate::patch::engine::run_resolve_verify_apply;
use crate::patch::scoped_regex::rewrite_node_target_with_scoped_regex;
use crate::transform::{
    TransformInstruction, build_changeset, build_delete_changeset, build_insert_after_changeset,
    build_insert_before_changeset, build_replace_changeset, parse_handles_for_file,
};

use super::hashline::{HashlinePatchResponse, execute_hashline_patch};

#[derive(Debug, Args)]
pub struct PatchArgs {
    #[arg(long, help = "Read patch request JSON from stdin")]
    pub json: bool,
    #[arg(
        long,
        value_name = "TARGET",
        help = "Unified target selector: node identity (hex16), line anchor (line:hex12), or file-start/file-end"
    )]
    pub at: Option<String>,
    #[arg(
        long,
        value_name = "IDENTITY",
        hide = true,
        help = "Legacy target identity flag (use --at)"
    )]
    pub identity: Option<String>,
    #[arg(
        long,
        value_name = "LINE:HASH",
        hide = true,
        help = "Legacy line anchor flag (use --at)"
    )]
    pub anchor: Option<String>,
    #[arg(
        long,
        value_name = "LINE:HASH",
        help = "Optional end line anchor for --replace-range (line flag mode)"
    )]
    pub end_anchor: Option<String>,
    #[arg(
        long = "config-path",
        value_name = "PATH",
        help = "Config path target for JSON/YAML/TOML files (dot/bracket syntax)"
    )]
    pub config_path: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        help = "Replace target node with text (node flag mode)"
    )]
    pub replace: Option<String>,
    #[arg(
        long = "set-value",
        value_name = "TEXT",
        help = "Set config path value text (config path flag mode)"
    )]
    pub set_value: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        help = "Insert text for file-start/file-end targets"
    )]
    pub insert: Option<String>,
    #[arg(
        long = "scoped-regex",
        value_name = "PATTERN",
        help = "Regex pattern applied only inside the resolved node target (node flag mode)"
    )]
    pub scoped_regex: Option<String>,
    #[arg(
        long = "scoped-replacement",
        value_name = "TEXT",
        help = "Replacement text used with --scoped-regex (node flag mode)"
    )]
    pub scoped_replacement: Option<String>,
    #[arg(long, help = "Delete target node (node flag mode)")]
    pub delete: bool,
    #[arg(
        long,
        value_name = "TEXT",
        help = "Insert text immediately before target node (node flag mode)"
    )]
    pub insert_before: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        help = "Insert text immediately after target node (node flag mode)"
    )]
    pub insert_after: Option<String>,
    #[arg(
        long = "set-line",
        value_name = "TEXT",
        help = "Replace the anchored line with text (line flag mode)"
    )]
    pub set_line: Option<String>,
    #[arg(
        long = "replace-range",
        value_name = "TEXT",
        help = "Replace anchored line range with text (line flag mode)"
    )]
    pub replace_range: Option<String>,
    #[arg(
        long = "insert-after-line",
        value_name = "TEXT",
        help = "Insert text after anchored line (line flag mode)"
    )]
    pub insert_after_line: Option<String>,
    #[arg(
        long,
        help = "If line-mode strict check fails with deterministic remap candidates, run one repair retry"
    )]
    pub auto_repair: bool,
    #[arg(long, help = "Include per-file apply results in output (flag mode)")]
    pub verbose: bool,
    #[arg(value_name = "FILE", help = "Target file path in flag mode")]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinPatchRequest {
    command: String,
    file: PathBuf,
    target: StdinPatchTarget,
    op: Value,
    #[serde(default)]
    options: StdinPatchOptions,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct StdinPatchOptions {
    #[serde(default)]
    auto_repair: bool,
    #[serde(default)]
    verbose: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum StdinPatchTarget {
    Node {
        identity: String,
        kind: String,
        #[serde(default)]
        span_hint: Option<Span>,
        expected_old_hash: String,
    },
    FileStart {
        expected_file_hash: String,
    },
    FileEnd {
        expected_file_hash: String,
    },
    Line {
        anchor: String,
        #[serde(default)]
        end_anchor: Option<String>,
    },
    ConfigPath {
        path: String,
        #[serde(default)]
        expected_file_hash: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum NodePatchOp {
    Replace {
        new_text: String,
    },
    ScopedRegex {
        pattern: String,
        replacement: String,
    },
    Delete,
    InsertBefore {
        new_text: String,
    },
    InsertAfter {
        new_text: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum LinePatchOp {
    SetLine {
        new_text: String,
    },
    ReplaceLines {
        new_text: String,
    },
    #[serde(rename = "insert_after", alias = "line_insert_after")]
    InsertAfter {
        text: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum FilePatchOp {
    Insert { new_text: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ConfigPatchOp {
    Set { new_text: String },
    Delete,
}

pub fn run_patch(args: PatchArgs) -> Result<Value, IdenteditError> {
    if args.json {
        return run_patch_json_mode();
    }

    let file = args
        .file
        .clone()
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "FILE is required unless --json mode is enabled".to_string(),
        })?;

    match resolve_patch_flag_target(&args)? {
        PatchFlagTarget::NodeIdentity(identity) => run_patch_flag_node_mode(file, identity, args),
        PatchFlagTarget::LineAnchor(anchor) => run_patch_flag_line_mode(file, anchor, args),
        PatchFlagTarget::FileStart => run_patch_flag_file_mode(file, true, args),
        PatchFlagTarget::FileEnd => run_patch_flag_file_mode(file, false, args),
        PatchFlagTarget::ConfigPath(path) => run_patch_flag_config_mode(file, path, args),
    }
}

enum PatchFlagTarget {
    NodeIdentity(String),
    LineAnchor(String),
    FileStart,
    FileEnd,
    ConfigPath(String),
}

fn resolve_patch_flag_target(args: &PatchArgs) -> Result<PatchFlagTarget, IdenteditError> {
    if let Some(path) = args.config_path.clone() {
        if args.at.is_some() || args.identity.is_some() || args.anchor.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message: "--config-path cannot be combined with --at/--identity/--anchor"
                    .to_string(),
            });
        }
        return Ok(PatchFlagTarget::ConfigPath(path));
    }

    if let Some(at) = args.at.as_deref() {
        if args.identity.is_some() || args.anchor.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message: "--at cannot be combined with legacy --identity/--anchor flags"
                    .to_string(),
            });
        }
        return parse_patch_at_target(at);
    }

    match (args.identity.clone(), args.anchor.clone()) {
        (Some(identity), None) => Ok(PatchFlagTarget::NodeIdentity(identity)),
        (None, Some(anchor)) => Ok(PatchFlagTarget::LineAnchor(anchor)),
        _ => Err(IdenteditError::InvalidRequest {
            message: "Exactly one target is required in flag mode: use --at, or legacy --identity/--anchor".to_string(),
        }),
    }
}

fn parse_patch_at_target(raw: &str) -> Result<PatchFlagTarget, IdenteditError> {
    let normalized = raw.trim();
    if normalized.eq_ignore_ascii_case("file-start") {
        return Ok(PatchFlagTarget::FileStart);
    }
    if normalized.eq_ignore_ascii_case("file-end") {
        return Ok(PatchFlagTarget::FileEnd);
    }

    if is_hex_with_len(normalized, HASH_HEX_LEN) {
        return Ok(PatchFlagTarget::NodeIdentity(
            normalized.to_ascii_lowercase(),
        ));
    }

    if is_line_anchor_with_hash_len(normalized, HASHLINE_PUBLIC_HEX_LEN) {
        let parsed =
            parse_line_ref(normalized).map_err(|error| IdenteditError::InvalidRequest {
                message: error.to_string(),
            })?;
        return Ok(PatchFlagTarget::LineAnchor(format!(
            "{}:{}",
            parsed.line, parsed.hash
        )));
    }

    Err(IdenteditError::InvalidRequest {
        message: format!(
            "Invalid --at target '{}': expected hex{} identity, <line>:<hex{}> anchor, file-start, or file-end",
            raw, HASH_HEX_LEN, HASHLINE_PUBLIC_HEX_LEN
        ),
    })
}

fn is_hex_with_len(value: &str, len: usize) -> bool {
    value.len() == len && value.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

fn is_line_anchor_with_hash_len(value: &str, hash_len: usize) -> bool {
    let Some((line, hash)) = value.split_once(':') else {
        return false;
    };
    !line.is_empty()
        && line.as_bytes().iter().all(u8::is_ascii_digit)
        && hash.len() == hash_len
        && hash.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

fn run_patch_json_mode() -> Result<Value, IdenteditError> {
    let mut request_body = String::new();
    std::io::stdin()
        .read_to_string(&mut request_body)
        .map_err(|error| IdenteditError::StdinRead { source: error })?;

    let request: StdinPatchRequest = serde_json::from_str(&request_body)
        .map_err(|source| IdenteditError::InvalidJsonRequest { source })?;

    if request.command != "patch" {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Unsupported command '{}' in patch JSON mode; expected 'patch'",
                request.command
            ),
        });
    }

    match request.target {
        StdinPatchTarget::Node {
            identity,
            kind,
            span_hint,
            expected_old_hash,
        } => run_patch_json_node(
            request.file,
            identity,
            kind,
            span_hint,
            expected_old_hash,
            request.op,
            request.options.verbose,
        ),
        StdinPatchTarget::FileStart { expected_file_hash } => run_patch_json_file(
            request.file,
            TransformTarget::FileStart { expected_file_hash },
            request.op,
            request.options.verbose,
        ),
        StdinPatchTarget::FileEnd { expected_file_hash } => run_patch_json_file(
            request.file,
            TransformTarget::FileEnd { expected_file_hash },
            request.op,
            request.options.verbose,
        ),
        StdinPatchTarget::Line { anchor, end_anchor } => run_patch_json_line(
            request.file,
            anchor,
            end_anchor,
            request.op,
            request.options.auto_repair,
        ),
        StdinPatchTarget::ConfigPath {
            path,
            expected_file_hash,
        } => run_patch_json_config(
            request.file,
            path,
            expected_file_hash,
            request.op,
            request.options.verbose,
        ),
    }
}

fn run_patch_json_file(
    file: PathBuf,
    target: TransformTarget,
    op: Value,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let file_op = serde_json::from_value::<FilePatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid file patch operation payload: {error}"),
        }
    })?;

    match file_op {
        FilePatchOp::Insert { new_text } => {
            run_patch_node_operation(file, target, OpKind::Insert { new_text }, verbose, None)
        }
    }
}

fn run_patch_json_node(
    file: PathBuf,
    identity: String,
    kind: String,
    span_hint: Option<Span>,
    expected_old_hash: String,
    op: Value,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let node_op = serde_json::from_value::<NodePatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid node patch operation payload: {error}"),
        }
    })?;
    let target = TransformTarget::node(identity, kind, span_hint, expected_old_hash);
    match node_op {
        NodePatchOp::Replace { new_text } => {
            run_patch_node_operation(file, target, OpKind::Replace { new_text }, verbose, None)
        }
        NodePatchOp::Delete => {
            run_patch_node_operation(file, target, OpKind::Delete, verbose, None)
        }
        NodePatchOp::InsertBefore { new_text } => run_patch_node_operation(
            file,
            target,
            OpKind::InsertBefore { new_text },
            verbose,
            None,
        ),
        NodePatchOp::InsertAfter { new_text } => run_patch_node_operation(
            file,
            target,
            OpKind::InsertAfter { new_text },
            verbose,
            None,
        ),
        NodePatchOp::ScopedRegex {
            pattern,
            replacement,
        } => run_patch_scoped_regex_node_operation(file, target, pattern, replacement, verbose),
    }
}

fn run_patch_node_operation(
    file: PathBuf,
    target: TransformTarget,
    op: OpKind,
    verbose: bool,
    regex_replacements: Option<usize>,
) -> Result<Value, IdenteditError> {
    let response = run_resolve_verify_apply(
        || {
            let file_change = build_changeset(&file, vec![TransformInstruction { target, op }])?;
            Ok(wrap_single_file(file_change))
        },
        verify_prepared_changeset,
        |changeset| apply_multi_file_changeset(&changeset),
    )?;

    serialize_node_patch_response(response, verbose, regex_replacements)
}

fn run_patch_scoped_regex_node_operation(
    file: PathBuf,
    target: TransformTarget,
    pattern: String,
    replacement: String,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let rewritten = rewrite_node_target_with_scoped_regex(&file, &target, &pattern, &replacement)?;
    run_patch_node_operation(
        file,
        target,
        OpKind::Replace {
            new_text: rewritten.new_text,
        },
        verbose,
        Some(rewritten.replacements),
    )
}

fn serialize_node_patch_response(
    response: crate::apply::ApplyResponse,
    verbose: bool,
    regex_replacements: Option<usize>,
) -> Result<Value, IdenteditError> {
    let mut value = serde_json::to_value(shape_apply_response(response, verbose))
        .map_err(|source| IdenteditError::ResponseSerialization { source })?;
    if let Some(replacements) = regex_replacements
        && let Some(object) = value.as_object_mut()
    {
        object.insert(
            "regex_replacements".to_string(),
            Value::Number(serde_json::Number::from(replacements)),
        );
    }
    Ok(value)
}

fn run_patch_json_line(
    file: PathBuf,
    anchor: String,
    end_anchor: Option<String>,
    op: Value,
    auto_repair: bool,
) -> Result<Value, IdenteditError> {
    let line_op = serde_json::from_value::<LinePatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid line patch operation payload: {error}"),
        }
    })?;
    let edit = match line_op {
        LinePatchOp::SetLine { new_text } => HashlineEdit::SetLine {
            set_line: SetLineEdit { anchor, new_text },
        },
        LinePatchOp::ReplaceLines { new_text } => HashlineEdit::ReplaceLines {
            replace_lines: ReplaceLinesEdit {
                start_anchor: anchor,
                end_anchor,
                new_text,
            },
        },
        LinePatchOp::InsertAfter { text } => HashlineEdit::InsertAfter {
            insert_after: InsertAfterEdit { anchor, text },
        },
    };
    let patch_response = execute_hashline_patch(file, vec![edit], auto_repair)?;
    serialize_line_patch_response(patch_response)
}

fn run_patch_json_config(
    file: PathBuf,
    path: String,
    expected_file_hash: Option<String>,
    op: Value,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let config_op = serde_json::from_value::<ConfigPatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid config path operation payload: {error}"),
        }
    })?;

    let canonical = match config_op {
        ConfigPatchOp::Set { new_text } => resolve_config_path_operation(
            file.as_path(),
            &path,
            expected_file_hash.as_deref(),
            ConfigPathOperation::Set { new_text },
        )?,
        ConfigPatchOp::Delete => resolve_config_path_operation(
            file.as_path(),
            &path,
            expected_file_hash.as_deref(),
            ConfigPathOperation::Delete,
        )?,
    };

    run_patch_node_operation(file, canonical.target, canonical.op, verbose, None)
}

fn serialize_line_patch_response(response: HashlinePatchResponse) -> Result<Value, IdenteditError> {
    serde_json::to_value(response)
        .map_err(|source| IdenteditError::ResponseSerialization { source })
}

fn run_patch_flag_node_mode(
    file: PathBuf,
    identity: String,
    args: PatchArgs,
) -> Result<Value, IdenteditError> {
    if args.anchor.is_some()
        || args.end_anchor.is_some()
        || args.insert.is_some()
        || args.set_line.is_some()
        || args.replace_range.is_some()
        || args.insert_after_line.is_some()
        || args.auto_repair
    {
        return Err(IdenteditError::InvalidRequest {
            message: "Node flag mode does not allow line/file-target options (--at line/file-start/file-end, --anchor/--end-anchor/--insert/--set-line/--replace-range/--insert-after-line/--auto-repair)".to_string(),
        });
    }

    let scoped_regex_present = args.scoped_regex.is_some() || args.scoped_replacement.is_some();
    if scoped_regex_present && (args.scoped_regex.is_none() || args.scoped_replacement.is_none()) {
        return Err(IdenteditError::InvalidRequest {
            message: "--scoped-regex and --scoped-replacement must be provided together"
                .to_string(),
        });
    }
    let operation_count = usize::from(args.replace.is_some())
        + usize::from(args.delete)
        + usize::from(args.insert_before.is_some())
        + usize::from(args.insert_after.is_some())
        + usize::from(scoped_regex_present);

    if operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message: "Exactly one node operation is required: choose one of --replace, --delete, --insert-before, --insert-after, --scoped-regex+--scoped-replacement".to_string(),
        });
    }
    if let Some(pattern) = args.scoped_regex {
        let replacement =
            args.scoped_replacement
                .ok_or_else(|| IdenteditError::InvalidRequest {
                    message: "missing payload for --scoped-replacement".to_string(),
                })?;
        return run_patch_flag_scoped_regex(file, &identity, pattern, replacement, args.verbose);
    }

    let file_change = if let Some(new_text) = args.replace {
        build_replace_changeset(&file, &identity, new_text)?
    } else if args.delete {
        build_delete_changeset(&file, &identity)?
    } else if let Some(new_text) = args.insert_before {
        build_insert_before_changeset(&file, &identity, new_text)?
    } else {
        let new_text = args
            .insert_after
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: "missing operation payload for --insert-after".to_string(),
            })?;
        build_insert_after_changeset(&file, &identity, new_text)?
    };

    let response = run_resolve_verify_apply(
        || Ok(wrap_single_file(file_change)),
        verify_prepared_changeset,
        |changeset| apply_multi_file_changeset(&changeset),
    )?;
    serialize_node_patch_response(response, args.verbose, None)
}

fn run_patch_flag_scoped_regex(
    file: PathBuf,
    identity: &str,
    pattern: String,
    replacement: String,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let handle = resolve_unique_identity_handle_for_patch(&file, identity)?;
    let target = TransformTarget::node(
        handle.identity,
        handle.kind,
        Some(handle.span),
        hash_text(&handle.text),
    );
    let rewritten = rewrite_node_target_with_scoped_regex(&file, &target, &pattern, &replacement)?;
    run_patch_node_operation(
        file,
        target,
        OpKind::Replace {
            new_text: rewritten.new_text,
        },
        verbose,
        Some(rewritten.replacements),
    )
}

fn run_patch_flag_file_mode(
    file: PathBuf,
    at_file_start: bool,
    args: PatchArgs,
) -> Result<Value, IdenteditError> {
    if args.identity.is_some()
        || args.anchor.is_some()
        || args.replace.is_some()
        || args.scoped_regex.is_some()
        || args.scoped_replacement.is_some()
        || args.delete
        || args.insert_before.is_some()
        || args.insert_after.is_some()
        || args.set_line.is_some()
        || args.replace_range.is_some()
        || args.insert_after_line.is_some()
        || args.auto_repair
    {
        return Err(IdenteditError::InvalidRequest {
            message: "File target mode accepts only --insert (plus optional --verbose)".to_string(),
        });
    }

    let insert_text = args.insert.ok_or_else(|| IdenteditError::InvalidRequest {
        message: "File target mode requires --insert payload".to_string(),
    })?;

    let source = std::fs::read(&file).map_err(|error| IdenteditError::io(&file, error))?;
    let expected_file_hash = hash_bytes(&source);
    let target = if at_file_start {
        TransformTarget::FileStart { expected_file_hash }
    } else {
        TransformTarget::FileEnd { expected_file_hash }
    };
    run_patch_node_operation(
        file,
        target,
        OpKind::Insert {
            new_text: insert_text,
        },
        args.verbose,
        None,
    )
}

fn resolve_unique_identity_handle_for_patch(
    file: &Path,
    identity: &str,
) -> Result<crate::handle::SelectionHandle, IdenteditError> {
    let handles = parse_handles_for_file(file)?;
    let matches = handles
        .into_iter()
        .filter(|handle| handle.identity == identity)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => Err(IdenteditError::TargetMissing {
            identity: identity.to_string(),
            file: file.display().to_string(),
        }),
        [single] => Ok(single.clone()),
        candidates => Err(IdenteditError::AmbiguousTarget {
            identity: identity.to_string(),
            file: file.display().to_string(),
            candidates: candidates.len(),
        }),
    }
}

fn run_patch_flag_line_mode(
    file: PathBuf,
    anchor: String,
    args: PatchArgs,
) -> Result<Value, IdenteditError> {
    if args.identity.is_some()
        || args.replace.is_some()
        || args.insert.is_some()
        || args.scoped_regex.is_some()
        || args.scoped_replacement.is_some()
        || args.delete
        || args.insert_before.is_some()
        || args.insert_after.is_some()
        || args.verbose
    {
        return Err(IdenteditError::InvalidRequest {
            message: "Line flag mode does not allow node/file-target options (--identity/--replace/--insert/--scoped-regex/--scoped-replacement/--delete/--insert-before/--insert-after/--verbose)".to_string(),
        });
    }
    let line_operation_count = usize::from(args.set_line.is_some())
        + usize::from(args.replace_range.is_some())
        + usize::from(args.insert_after_line.is_some());
    if line_operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message: "Exactly one line operation is required: choose one of --set-line, --replace-range, --insert-after-line".to_string(),
        });
    }

    let edit = if let Some(new_text) = args.set_line {
        if args.end_anchor.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message: "--end-anchor is only valid with --replace-range in line flag mode"
                    .to_string(),
            });
        }
        HashlineEdit::SetLine {
            set_line: SetLineEdit { anchor, new_text },
        }
    } else if let Some(new_text) = args.replace_range {
        HashlineEdit::ReplaceLines {
            replace_lines: ReplaceLinesEdit {
                start_anchor: anchor,
                end_anchor: args.end_anchor,
                new_text,
            },
        }
    } else {
        if args.end_anchor.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message: "--end-anchor is only valid with --replace-range in line flag mode"
                    .to_string(),
            });
        }
        let text = args
            .insert_after_line
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: "missing operation payload for --insert-after-line".to_string(),
            })?;
        HashlineEdit::InsertAfter {
            insert_after: InsertAfterEdit { anchor, text },
        }
    };

    let patch_response = execute_hashline_patch(file, vec![edit], args.auto_repair)?;
    serialize_line_patch_response(patch_response)
}

fn run_patch_flag_config_mode(
    file: PathBuf,
    path: String,
    args: PatchArgs,
) -> Result<Value, IdenteditError> {
    if args.at.is_some()
        || args.identity.is_some()
        || args.anchor.is_some()
        || args.end_anchor.is_some()
        || args.replace.is_some()
        || args.insert.is_some()
        || args.scoped_regex.is_some()
        || args.scoped_replacement.is_some()
        || args.insert_before.is_some()
        || args.insert_after.is_some()
        || args.set_line.is_some()
        || args.replace_range.is_some()
        || args.insert_after_line.is_some()
        || args.auto_repair
    {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path flag mode supports only --set-value or --delete (plus optional --verbose)".to_string(),
        });
    }

    let operation_count = usize::from(args.set_value.is_some()) + usize::from(args.delete);
    if operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message:
                "Exactly one config path operation is required: choose one of --set-value or --delete"
                    .to_string(),
        });
    }

    let canonical = if let Some(new_text) = args.set_value {
        resolve_config_path_operation(
            file.as_path(),
            &path,
            None,
            ConfigPathOperation::Set { new_text },
        )?
    } else {
        resolve_config_path_operation(file.as_path(), &path, None, ConfigPathOperation::Delete)?
    };

    run_patch_node_operation(file, canonical.target, canonical.op, args.verbose, None)
}

fn wrap_single_file(file_change: crate::changeset::FileChange) -> MultiFileChangeset {
    MultiFileChangeset {
        files: vec![file_change],
        transaction: Default::default(),
    }
}

fn verify_prepared_changeset(
    changeset: MultiFileChangeset,
) -> Result<MultiFileChangeset, IdenteditError> {
    Ok(changeset)
}
