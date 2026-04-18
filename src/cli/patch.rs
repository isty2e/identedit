use std::io::Read;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::Deserialize;
use serde_json::Value;

use crate::apply::{apply_multi_file_changeset, dry_run_multi_file_changeset};
use crate::changeset::{MultiFileChangeset, OpKind, TransformTarget};
use crate::cli::apply::shape_apply_response;
use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::hash::{HASH_HEX_LEN, hash_bytes};
use crate::hashline::{HASHLINE_PUBLIC_HEX_LEN, parse_line_ref};
use crate::hashline::{HashlineEdit, InsertAfterEdit, ReplaceLinesEdit, SetLineEdit};
use crate::patch::config_path::{ConfigPathOperation, resolve_config_path_operation};
use crate::patch::engine::run_resolve_verify_apply;
use crate::patch::scoped_regex::rewrite_node_target_with_scoped_regex;
use crate::selector::Selector;
use crate::transform::{TransformInstruction, build_changeset, parse_handles_for_file};

use super::line_patch::{HashlinePatchResponse, execute_hashline_patch};

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
        value_name = "KIND",
        help = "Node kind for direct symbol-targeted patching (requires --name, node flag mode)"
    )]
    pub kind: Option<String>,
    #[arg(
        long,
        value_name = "GLOB",
        help = "Symbol name glob for direct symbol-targeted patching (requires --kind, node flag mode)"
    )]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace target node with text (node flag mode)"
    )]
    pub replace: Option<Option<String>>,
    #[arg(
        long = "text-file",
        value_name = "PATH",
        help = "Read text payload from file for the selected text-taking patch operation"
    )]
    pub text_file: Option<PathBuf>,
    #[arg(
        long = "stdin-text",
        help = "Read text payload from stdin for the selected text-taking patch operation"
    )]
    pub stdin_text: bool,
    #[arg(
        long = "set-value",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Set config path value text (config path flag mode)"
    )]
    pub set_value: Option<Option<String>>,
    #[arg(
        long = "append-value",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Append value text to target array at config path (config path flag mode)"
    )]
    pub append_value: Option<Option<String>>,
    #[arg(
        long = "create-missing",
        help = "Allow config path set to create missing map/table keys (not array indexes)"
    )]
    pub create_missing: bool,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text for file-start/file-end targets"
    )]
    pub insert: Option<Option<String>>,
    #[arg(
        long = "scoped-regex",
        value_name = "PATTERN",
        help = "Regex pattern applied only inside the resolved node target (node flag mode)"
    )]
    pub scoped_regex: Option<String>,
    #[arg(
        long = "scoped-replacement",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replacement text used with --scoped-regex (node flag mode)"
    )]
    pub scoped_replacement: Option<Option<String>>,
    #[arg(long, help = "Delete target node (node flag mode)")]
    pub delete: bool,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text immediately before target node (node flag mode)"
    )]
    pub insert_before: Option<Option<String>>,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text immediately after target node (node flag mode)"
    )]
    pub insert_after: Option<Option<String>>,
    #[arg(
        long = "set-line",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace the anchored line with text (line flag mode)"
    )]
    pub set_line: Option<Option<String>>,
    #[arg(
        long = "replace-range",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace anchored line range with text (line flag mode)"
    )]
    pub replace_range: Option<Option<String>>,
    #[arg(
        long = "insert-after-line",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text after anchored line (line flag mode)"
    )]
    pub insert_after_line: Option<Option<String>>,
    #[arg(
        long,
        help = "If line-mode strict check fails with deterministic remap candidates, run one repair retry"
    )]
    pub auto_repair: bool,
    #[arg(long, help = "Validate and preview without writing files")]
    pub dry_run: bool,
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
    dry_run: bool,
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
    Set {
        new_text: String,
        #[serde(default)]
        create_missing: bool,
    },
    Append {
        new_text: String,
    },
    Delete,
}

pub fn run_patch(args: PatchArgs) -> Result<Value, IdenteditError> {
    if args.json {
        if args.text_file.is_some() || args.stdin_text {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "--text-file and --stdin-text are only supported in flag mode; JSON patch mode already reads the request body from stdin."
                        .to_string(),
            });
        }
        return run_patch_json_mode(args.dry_run);
    }

    let request = parse_flag_patch_request(&args)?;
    execute_flag_patch_request(request)
}

#[derive(Debug, Clone)]
enum PatchTargetIngress {
    NodeIdentity(String),
    NodeSelector { kind: String, name_pattern: String },
    LineAnchor(String),
    FileStart,
    FileEnd,
    ConfigPath(String),
}

#[derive(Debug, Clone)]
enum PatchTextSource {
    File(PathBuf),
    Stdin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ApplyBackedExecution {
    dry_run: bool,
    verbose: bool,
}

impl ApplyBackedExecution {
    fn from_args(args: &PatchArgs) -> Self {
        Self {
            dry_run: args.dry_run,
            verbose: args.verbose,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineExecution {
    dry_run: bool,
    auto_repair: bool,
}

impl LineExecution {
    fn from_args(args: &PatchArgs) -> Self {
        Self {
            dry_run: args.dry_run,
            auto_repair: args.auto_repair,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NodeTargetSelector {
    Identity(String),
    Selector { kind: String, name_pattern: String },
}

impl NodeTargetSelector {
    fn resolve(self, file: &Path) -> Result<SelectionHandle, IdenteditError> {
        match self {
            Self::Identity(identity) => resolve_unique_identity_handle_for_patch(file, &identity),
            Self::Selector { kind, name_pattern } => {
                resolve_unique_selector_handle_for_patch(file, &kind, &name_pattern)
            }
        }
    }
}

#[derive(Debug, Clone)]
struct NodeFlagPatchRequest {
    file: PathBuf,
    selector: NodeTargetSelector,
    operation: PreparedNodePatchOperation,
    execution: ApplyBackedExecution,
}

#[derive(Debug, Clone)]
struct CanonicalFlagPatchRequest {
    file: PathBuf,
    target: TransformTarget,
    op: OpKind,
    execution: ApplyBackedExecution,
}

#[derive(Debug, Clone)]
struct LineFlagPatchRequest {
    file: PathBuf,
    edit: HashlineEdit,
    execution: LineExecution,
}

#[derive(Debug, Clone)]
enum FlagPatchRequest {
    Node(NodeFlagPatchRequest),
    Canonical(CanonicalFlagPatchRequest),
    Line(LineFlagPatchRequest),
}

const NODE_MODE_OPERATIONS: &str =
    "--replace, --delete, --insert-before, --insert-after, or --scoped-regex with --scoped-replacement";
const LINE_MODE_OPERATIONS: &str = "--set-line, --replace-range, or --insert-after-line";
const CONFIG_MODE_OPERATIONS: &str = "--set-value, --append-value, or --delete";

fn node_mode_guidance() -> String {
    format!(
        "Node target mode supports {NODE_MODE_OPERATIONS}. For line edits use --anchor or --at <line:hash>; for file insertion use --at file-start|file-end --insert; for config edits use --config-path with {CONFIG_MODE_OPERATIONS}."
    )
}

fn line_mode_guidance() -> String {
    format!(
        "Line target mode supports {LINE_MODE_OPERATIONS}. For node edits use --identity, --kind with --name, or --at <hex16>."
    )
}

fn file_mode_guidance() -> String {
    "File target mode supports only --insert. Use --at file-start or --at file-end with --insert <text>."
        .to_string()
}

fn config_mode_guidance() -> String {
    format!(
        "Config path mode supports {CONFIG_MODE_OPERATIONS}. Use --create-missing only with --set-value."
    )
}

fn resolve_patch_text_source(args: &PatchArgs) -> Result<Option<PatchTextSource>, IdenteditError> {
    match (args.text_file.clone(), args.stdin_text) {
        (Some(_), true) => Err(IdenteditError::InvalidRequest {
            message:
                "Choose exactly one external text source: --text-file <path> or --stdin-text."
                    .to_string(),
        }),
        (Some(path), false) => Ok(Some(PatchTextSource::File(path))),
        (None, true) => Ok(Some(PatchTextSource::Stdin)),
        (None, false) => Ok(None),
    }
}

fn read_patch_text_source(source: PatchTextSource) -> Result<String, IdenteditError> {
    match source {
        PatchTextSource::File(path) => {
            std::fs::read_to_string(&path).map_err(|error| IdenteditError::io(&path, error))
        }
        PatchTextSource::Stdin => {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|error| IdenteditError::StdinRead { source: error })?;
            Ok(buffer)
        }
    }
}

fn resolve_patch_text_payload(
    flag_name: &str,
    raw_value: Option<Option<String>>,
    text_source: Option<PatchTextSource>,
) -> Result<Option<String>, IdenteditError> {
    match raw_value {
        None => Ok(None),
        Some(Some(inline)) => {
            if text_source.is_some() {
                Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "{flag_name} accepts either inline text or one external text source. Use {flag_name} <text>, {flag_name} --text-file <path>, or {flag_name} --stdin-text."
                    ),
                })
            } else {
                Ok(Some(inline))
            }
        }
        Some(None) => {
            let source = text_source.ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "{flag_name} requires text. Provide {flag_name} <text>, {flag_name} --text-file <path>, or {flag_name} --stdin-text."
                ),
            })?;
            read_patch_text_source(source).map(Some)
        }
    }
}

fn text_arg_present(raw_value: &Option<Option<String>>) -> bool {
    raw_value.is_some()
}

fn reject_unused_text_source(
    text_source: Option<PatchTextSource>,
    valid_operations: &str,
) -> Result<(), IdenteditError> {
    if text_source.is_some() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "External text sources require a text-taking operation. Use {valid_operations} with --text-file <path> or --stdin-text."
            ),
        });
    }
    Ok(())
}

fn parse_flag_patch_request(args: &PatchArgs) -> Result<FlagPatchRequest, IdenteditError> {
    let file = args
        .file
        .clone()
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "FILE is required unless --json mode is enabled".to_string(),
        })?;

    match resolve_patch_target_ingress(args)? {
        PatchTargetIngress::NodeIdentity(identity) => {
            parse_node_flag_patch_request(file, NodeTargetSelector::Identity(identity), args)
        }
        PatchTargetIngress::NodeSelector { kind, name_pattern } => parse_node_flag_patch_request(
            file,
            NodeTargetSelector::Selector { kind, name_pattern },
            args,
        ),
        PatchTargetIngress::LineAnchor(anchor) => parse_line_flag_patch_request(file, anchor, args),
        PatchTargetIngress::FileStart => parse_file_flag_patch_request(file, true, args),
        PatchTargetIngress::FileEnd => parse_file_flag_patch_request(file, false, args),
        PatchTargetIngress::ConfigPath(path) => parse_config_flag_patch_request(file, path, args),
    }
}

fn execute_flag_patch_request(request: FlagPatchRequest) -> Result<Value, IdenteditError> {
    match request {
        FlagPatchRequest::Node(request) => execute_node_flag_patch_request(request),
        FlagPatchRequest::Canonical(request) => run_patch_node_operation(
            request.file,
            request.target,
            request.op,
            request.execution.dry_run,
            request.execution.verbose,
            None,
        ),
        FlagPatchRequest::Line(request) => {
            let response = execute_hashline_patch(
                request.file,
                vec![request.edit],
                request.execution.auto_repair,
                request.execution.dry_run,
            )?;
            serialize_line_patch_response(response)
        }
    }
}

fn resolve_patch_target_ingress(args: &PatchArgs) -> Result<PatchTargetIngress, IdenteditError> {
    let selector_present = args.kind.is_some() || args.name.is_some();

    if let Some(path) = args.config_path.clone() {
        if args.at.is_some() || args.identity.is_some() || args.anchor.is_some() || selector_present
        {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "--config-path cannot be combined with --at, --identity, --anchor, --kind, or --name. {}",
                    config_mode_guidance()
                ),
            });
        }
        return Ok(PatchTargetIngress::ConfigPath(path));
    }

    if let Some(at) = args.at.as_deref() {
        if args.identity.is_some() || args.anchor.is_some() || selector_present {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "Choose exactly one target selector. Use --at <target> by itself, or use --identity, --anchor, or --kind with --name."
                        .to_string(),
            });
        }
        return parse_patch_at_target(at);
    }

    match (
        args.identity.clone(),
        args.anchor.clone(),
        args.kind.clone(),
        args.name.clone(),
    ) {
        (Some(identity), None, None, None) => Ok(PatchTargetIngress::NodeIdentity(identity)),
        (None, Some(anchor), None, None) => Ok(PatchTargetIngress::LineAnchor(anchor)),
        (None, None, Some(kind), Some(name_pattern)) => {
            Ok(PatchTargetIngress::NodeSelector { kind, name_pattern })
        }
        (None, None, Some(_), None) | (None, None, None, Some(_)) => {
            Err(IdenteditError::InvalidRequest {
                message:
                    "Direct symbol targeting requires both --kind and --name. Example: --kind function_definition --name process_*."
                        .to_string(),
            })
        }
        _ => Err(IdenteditError::InvalidRequest {
            message:
                "Choose exactly one target selector in flag mode: --at <target>, --identity <hex16>, --anchor <line:hash>, or --kind <kind> --name <glob>."
                    .to_string(),
            }),
    }
}

fn parse_patch_at_target(raw: &str) -> Result<PatchTargetIngress, IdenteditError> {
    let normalized = raw.trim();
    if normalized.eq_ignore_ascii_case("file-start") {
        return Ok(PatchTargetIngress::FileStart);
    }
    if normalized.eq_ignore_ascii_case("file-end") {
        return Ok(PatchTargetIngress::FileEnd);
    }

    if is_hex_with_len(normalized, HASH_HEX_LEN) {
        return Ok(PatchTargetIngress::NodeIdentity(
            normalized.to_ascii_lowercase(),
        ));
    }

    if is_line_anchor_with_hash_len(normalized, HASHLINE_PUBLIC_HEX_LEN) {
        let parsed =
            parse_line_ref(normalized).map_err(|error| IdenteditError::InvalidRequest {
                message: error.to_string(),
            })?;
        return Ok(PatchTargetIngress::LineAnchor(format!(
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

fn run_patch_json_mode(cli_dry_run: bool) -> Result<Value, IdenteditError> {
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

    let dry_run = cli_dry_run || request.options.dry_run;

    match request.target {
        StdinPatchTarget::Node {
            identity,
            kind,
            span_hint,
            expected_old_hash,
        } => run_patch_json_node(
            request.file,
            TransformTarget::node(identity, kind, span_hint, expected_old_hash),
            request.op,
            dry_run,
            request.options.verbose,
        ),
        StdinPatchTarget::FileStart { expected_file_hash } => run_patch_json_file(
            request.file,
            TransformTarget::FileStart { expected_file_hash },
            request.op,
            dry_run,
            request.options.verbose,
        ),
        StdinPatchTarget::FileEnd { expected_file_hash } => run_patch_json_file(
            request.file,
            TransformTarget::FileEnd { expected_file_hash },
            request.op,
            dry_run,
            request.options.verbose,
        ),
        StdinPatchTarget::Line { anchor, end_anchor } => run_patch_json_line(
            request.file,
            anchor,
            end_anchor,
            request.op,
            request.options.auto_repair,
            dry_run,
        ),
        StdinPatchTarget::ConfigPath {
            path,
            expected_file_hash,
        } => run_patch_json_config(
            request.file,
            path,
            expected_file_hash,
            request.op,
            dry_run,
            request.options.verbose,
        ),
    }
}

fn run_patch_json_file(
    file: PathBuf,
    target: TransformTarget,
    op: Value,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let file_op = serde_json::from_value::<FilePatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid file patch operation payload: {error}"),
        }
    })?;

    match file_op {
        FilePatchOp::Insert { new_text } => {
            run_patch_node_operation(
                file,
                target,
                OpKind::Insert { new_text },
                dry_run,
                verbose,
                None,
            )
        }
    }
}

fn run_patch_json_node(
    file: PathBuf,
    target: TransformTarget,
    op: Value,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let node_op = serde_json::from_value::<NodePatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid node patch operation payload: {error}"),
        }
    })?;
    match node_op {
        NodePatchOp::Replace { new_text } => {
            run_patch_node_operation(
                file,
                target,
                OpKind::Replace { new_text },
                dry_run,
                verbose,
                None,
            )
        }
        NodePatchOp::Delete => run_patch_node_operation(file, target, OpKind::Delete, dry_run, verbose, None),
        NodePatchOp::InsertBefore { new_text } => run_patch_node_operation(
            file,
            target,
            OpKind::InsertBefore { new_text },
            dry_run,
            verbose,
            None,
        ),
        NodePatchOp::InsertAfter { new_text } => run_patch_node_operation(
            file,
            target,
            OpKind::InsertAfter { new_text },
            dry_run,
            verbose,
            None,
        ),
        NodePatchOp::ScopedRegex { pattern, replacement } => {
            run_patch_scoped_regex_node_operation(file, target, pattern, replacement, dry_run, verbose)
        }
    }
}

fn run_patch_node_operation(
    file: PathBuf,
    target: TransformTarget,
    op: OpKind,
    dry_run: bool,
    verbose: bool,
    regex_replacements: Option<usize>,
) -> Result<Value, IdenteditError> {
    let response = run_resolve_verify_apply(
        || {
            let file_change = build_changeset(&file, vec![TransformInstruction { target, op }])?;
            Ok(wrap_single_file(file_change))
        },
        verify_prepared_changeset,
        |changeset| {
            if dry_run {
                dry_run_multi_file_changeset(&changeset)
            } else {
                apply_multi_file_changeset(&changeset)
            }
        },
    )?;

    serialize_node_patch_response(response, verbose, regex_replacements)
}

fn run_patch_scoped_regex_node_operation(
    file: PathBuf,
    target: TransformTarget,
    pattern: String,
    replacement: String,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    let rewritten = rewrite_node_target_with_scoped_regex(&file, &target, &pattern, &replacement)?;
    run_patch_node_operation(
        file,
        target,
        OpKind::Replace {
            new_text: rewritten.new_text,
        },
        dry_run,
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
    dry_run: bool,
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
    let patch_response = execute_hashline_patch(file, vec![edit], auto_repair, dry_run)?;
    serialize_line_patch_response(patch_response)
}

fn run_patch_json_config(
    file: PathBuf,
    path: String,
    expected_file_hash: Option<String>,
    op: Value,
    dry_run: bool,
    verbose: bool,
) -> Result<Value, IdenteditError> {
    if let Some(object) = op.as_object()
        && object
            .get("type")
            .and_then(|value| value.as_str())
            .is_some_and(|value| value == "delete")
        && object.contains_key("create_missing")
    {
        return Err(IdenteditError::InvalidRequest {
            message: "Config path delete operation does not accept create_missing".to_string(),
        });
    }

    let config_op = serde_json::from_value::<ConfigPatchOp>(op).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("Invalid config path operation payload: {error}"),
        }
    })?;

    let canonical = match config_op {
        ConfigPatchOp::Set {
            new_text,
            create_missing,
        } => resolve_config_path_operation(
            file.as_path(),
            &path,
            expected_file_hash.as_deref(),
            ConfigPathOperation::Set {
                new_text,
                create_missing,
            },
        )?,
        ConfigPatchOp::Append { new_text } => resolve_config_path_operation(
            file.as_path(),
            &path,
            expected_file_hash.as_deref(),
            ConfigPathOperation::Append { new_text },
        )?,
        ConfigPatchOp::Delete => resolve_config_path_operation(
            file.as_path(),
            &path,
            expected_file_hash.as_deref(),
            ConfigPathOperation::Delete,
        )?,
    };

    run_patch_node_operation(file, canonical.target, canonical.op, dry_run, verbose, None)
}

fn serialize_line_patch_response(response: HashlinePatchResponse) -> Result<Value, IdenteditError> {
    serde_json::to_value(response)
        .map_err(|source| IdenteditError::ResponseSerialization { source })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreparedNodePatchOperation {
    Standard(OpKind),
    ScopedRegex {
        pattern: String,
        replacement: String,
    },
}

fn parse_node_flag_patch_request(
    file: PathBuf,
    selector: NodeTargetSelector,
    args: &PatchArgs,
) -> Result<FlagPatchRequest, IdenteditError> {
    let operation = prepare_patch_flag_node_operation(args)?;
    Ok(FlagPatchRequest::Node(NodeFlagPatchRequest {
        file,
        selector,
        operation,
        execution: ApplyBackedExecution::from_args(args),
    }))
}

fn prepare_patch_flag_node_operation(
    args: &PatchArgs,
) -> Result<PreparedNodePatchOperation, IdenteditError> {
    let text_source = resolve_patch_text_source(args)?;

    if args.anchor.is_some()
        || args.end_anchor.is_some()
        || args.insert.is_some()
        || args.set_value.is_some()
        || args.append_value.is_some()
        || args.create_missing
        || args.set_line.is_some()
        || args.replace_range.is_some()
        || args.insert_after_line.is_some()
        || args.auto_repair
    {
        return Err(IdenteditError::InvalidRequest {
            message: node_mode_guidance(),
        });
    }

    let scoped_regex_present = args.scoped_regex.is_some() || args.scoped_replacement.is_some();
    if scoped_regex_present && (args.scoped_regex.is_none() || args.scoped_replacement.is_none()) {
        return Err(IdenteditError::InvalidRequest {
            message: "--scoped-regex and --scoped-replacement must be provided together"
                .to_string(),
        });
    }

    let operation_count = usize::from(text_arg_present(&args.replace))
        + usize::from(args.delete)
        + usize::from(text_arg_present(&args.insert_before))
        + usize::from(text_arg_present(&args.insert_after))
        + usize::from(scoped_regex_present);
    if operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Choose exactly one node operation. Node target mode supports {NODE_MODE_OPERATIONS}."
            ),
        });
    }

    if let Some(pattern) = args.scoped_regex.clone() {
        let replacement = resolve_patch_text_payload(
            "--scoped-replacement",
            args.scoped_replacement.clone(),
            text_source,
        )?
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "missing payload for --scoped-replacement".to_string(),
        })?;
        return Ok(PreparedNodePatchOperation::ScopedRegex {
            pattern,
            replacement,
        });
    }

    if let Some(new_text) =
        resolve_patch_text_payload("--replace", args.replace.clone(), text_source.clone())?
    {
        return Ok(PreparedNodePatchOperation::Standard(OpKind::Replace {
            new_text,
        }));
    }

    if args.delete {
        reject_unused_text_source(text_source, NODE_MODE_OPERATIONS)?;
        return Ok(PreparedNodePatchOperation::Standard(OpKind::Delete));
    }

    if let Some(new_text) = resolve_patch_text_payload(
        "--insert-before",
        args.insert_before.clone(),
        text_source.clone(),
    )? {
        return Ok(PreparedNodePatchOperation::Standard(OpKind::InsertBefore {
            new_text,
        }));
    }

    let new_text = resolve_patch_text_payload(
        "--insert-after",
        args.insert_after.clone(),
        text_source,
    )?
    .ok_or_else(|| IdenteditError::InvalidRequest {
        message: "missing operation payload for --insert-after".to_string(),
    })?;
    Ok(PreparedNodePatchOperation::Standard(OpKind::InsertAfter {
        new_text,
    }))
}

fn execute_node_flag_patch_request(request: NodeFlagPatchRequest) -> Result<Value, IdenteditError> {
    let handle = request.selector.resolve(&request.file)?;
    execute_patch_flag_node_operation(request.file, handle, request.operation, request.execution)
}

fn execute_patch_flag_node_operation(
    file: PathBuf,
    handle: SelectionHandle,
    operation: PreparedNodePatchOperation,
    execution: ApplyBackedExecution,
) -> Result<Value, IdenteditError> {
    let target = TransformTarget::node(
        handle.identity,
        handle.kind,
        Some(handle.span),
        handle.expected_old_hash,
    );

    match operation {
        PreparedNodePatchOperation::Standard(op) => {
            run_patch_node_operation(
                file,
                target,
                op,
                execution.dry_run,
                execution.verbose,
                None,
            )
        }
        PreparedNodePatchOperation::ScopedRegex {
            pattern,
            replacement,
        } => {
            let rewritten =
                rewrite_node_target_with_scoped_regex(&file, &target, &pattern, &replacement)?;
            run_patch_node_operation(
                file,
                target,
                OpKind::Replace {
                    new_text: rewritten.new_text,
                },
                execution.dry_run,
                execution.verbose,
                Some(rewritten.replacements),
            )
        }
    }
}

fn parse_file_flag_patch_request(
    file: PathBuf,
    at_file_start: bool,
    args: &PatchArgs,
) -> Result<FlagPatchRequest, IdenteditError> {
    let text_source = resolve_patch_text_source(args)?;

    if args.identity.is_some()
        || args.anchor.is_some()
        || args.replace.is_some()
        || args.set_value.is_some()
        || args.append_value.is_some()
        || args.scoped_regex.is_some()
        || args.scoped_replacement.is_some()
        || args.delete
        || args.insert_before.is_some()
        || args.insert_after.is_some()
        || args.create_missing
        || args.set_line.is_some()
        || args.replace_range.is_some()
        || args.insert_after_line.is_some()
        || args.auto_repair
    {
        return Err(IdenteditError::InvalidRequest {
            message: file_mode_guidance(),
        });
    }

    let insert_text = resolve_patch_text_payload("--insert", args.insert.clone(), text_source)?
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: file_mode_guidance(),
        })?;

    let source = std::fs::read(&file).map_err(|error| IdenteditError::io(&file, error))?;
    let expected_file_hash = hash_bytes(&source);
    let target = if at_file_start {
        TransformTarget::FileStart { expected_file_hash }
    } else {
        TransformTarget::FileEnd { expected_file_hash }
    };

    Ok(FlagPatchRequest::Canonical(CanonicalFlagPatchRequest {
        file,
        target,
        op: OpKind::Insert {
            new_text: insert_text,
        },
        execution: ApplyBackedExecution::from_args(args),
    }))
}

fn resolve_unique_identity_handle_for_patch(
    file: &Path,
    identity: &str,
) -> Result<SelectionHandle, IdenteditError> {
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

fn resolve_unique_selector_handle_for_patch(
    file: &Path,
    kind: &str,
    name_pattern: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let selector = Selector {
        kind: kind.to_string(),
        name_pattern: Some(name_pattern.to_string()),
        exclude_kinds: vec![],
    };
    let selector_description = format!("kind='{kind}', name='{name_pattern}'");
    let matches = selector.filter(parse_handles_for_file(file)?)?;

    match matches.as_slice() {
        [] => Err(IdenteditError::TargetMissingSelector {
            selector: selector_description,
            file: file.display().to_string(),
        }),
        [single] => Ok(single.clone()),
        candidates => Err(IdenteditError::AmbiguousTargetSelector {
            selector: selector_description,
            file: file.display().to_string(),
            candidates: candidates.len(),
        }),
    }
}

fn parse_line_flag_patch_request(
    file: PathBuf,
    anchor: String,
    args: &PatchArgs,
) -> Result<FlagPatchRequest, IdenteditError> {
    let text_source = resolve_patch_text_source(args)?;

    if args.identity.is_some()
        || args.replace.is_some()
        || args.insert.is_some()
        || args.set_value.is_some()
        || args.append_value.is_some()
        || args.scoped_regex.is_some()
        || args.scoped_replacement.is_some()
        || args.delete
        || args.insert_before.is_some()
        || args.insert_after.is_some()
        || args.create_missing
        || args.verbose
    {
        return Err(IdenteditError::InvalidRequest {
            message: line_mode_guidance(),
        });
    }
    let line_operation_count = usize::from(text_arg_present(&args.set_line))
        + usize::from(text_arg_present(&args.replace_range))
        + usize::from(text_arg_present(&args.insert_after_line));
    if line_operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Choose exactly one line operation. Line target mode supports {LINE_MODE_OPERATIONS}."
            ),
        });
    }

    let edit = if let Some(new_text) =
        resolve_patch_text_payload("--set-line", args.set_line.clone(), text_source.clone())?
    {
        if args.end_anchor.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "Use --end-anchor only with --replace-range in line target mode."
                        .to_string(),
            });
        }
        HashlineEdit::SetLine {
            set_line: SetLineEdit { anchor, new_text },
        }
    } else if let Some(new_text) = resolve_patch_text_payload(
        "--replace-range",
        args.replace_range.clone(),
        text_source.clone(),
    )? {
        HashlineEdit::ReplaceLines {
            replace_lines: ReplaceLinesEdit {
                start_anchor: anchor,
                end_anchor: args.end_anchor.clone(),
                new_text,
            },
        }
    } else {
        if args.end_anchor.is_some() {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "Use --end-anchor only with --replace-range in line target mode."
                        .to_string(),
            });
        }
        let text = resolve_patch_text_payload(
            "--insert-after-line",
            args.insert_after_line.clone(),
            text_source,
        )?
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "missing operation payload for --insert-after-line".to_string(),
        })?;
        HashlineEdit::InsertAfter {
            insert_after: InsertAfterEdit { anchor, text },
        }
    };

    Ok(FlagPatchRequest::Line(LineFlagPatchRequest {
        file,
        edit,
        execution: LineExecution::from_args(args),
    }))
}

fn parse_config_flag_patch_request(
    file: PathBuf,
    path: String,
    args: &PatchArgs,
) -> Result<FlagPatchRequest, IdenteditError> {
    let text_source = resolve_patch_text_source(args)?;

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
            message: config_mode_guidance(),
        });
    }

    if args.create_missing && (args.delete || args.append_value.is_some()) {
        return Err(IdenteditError::InvalidRequest {
            message: config_mode_guidance(),
        });
    }

    let operation_count = usize::from(text_arg_present(&args.set_value))
        + usize::from(text_arg_present(&args.append_value))
        + usize::from(args.delete);
    if operation_count != 1 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Choose exactly one config path operation. Config path mode supports {CONFIG_MODE_OPERATIONS}."
            ),
        });
    }

    let canonical = if let Some(new_text) =
        resolve_patch_text_payload("--set-value", args.set_value.clone(), text_source.clone())?
    {
        resolve_config_path_operation(
            file.as_path(),
            &path,
            None,
            ConfigPathOperation::Set {
                new_text,
                create_missing: args.create_missing,
            },
        )?
    } else if let Some(new_text) =
        resolve_patch_text_payload(
            "--append-value",
            args.append_value.clone(),
            text_source.clone(),
        )?
    {
        resolve_config_path_operation(
            file.as_path(),
            &path,
            None,
            ConfigPathOperation::Append { new_text },
        )?
    } else {
        reject_unused_text_source(text_source, CONFIG_MODE_OPERATIONS)?;
        resolve_config_path_operation(file.as_path(), &path, None, ConfigPathOperation::Delete)?
    };

    Ok(FlagPatchRequest::Canonical(CanonicalFlagPatchRequest {
        file,
        target: canonical.target,
        op: canonical.op,
        execution: ApplyBackedExecution::from_args(args),
    }))
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::NamedTempFile;

    use super::{
        ApplyBackedExecution, FlagPatchRequest, HashlineEdit, LineExecution, NodeFlagPatchRequest,
        NodeTargetSelector, PatchArgs, PreparedNodePatchOperation, parse_flag_patch_request,
    };
    use crate::changeset::{OpKind, TransformTarget};

    fn base_args(file: PathBuf) -> PatchArgs {
        PatchArgs {
            json: false,
            at: None,
            identity: None,
            anchor: None,
            end_anchor: None,
            config_path: None,
            kind: None,
            name: None,
            replace: None,
            text_file: None,
            stdin_text: false,
            set_value: None,
            append_value: None,
            create_missing: false,
            insert: None,
            scoped_regex: None,
            scoped_replacement: None,
            delete: false,
            insert_before: None,
            insert_after: None,
            set_line: None,
            replace_range: None,
            insert_after_line: None,
            auto_repair: false,
            dry_run: false,
            verbose: false,
            file: Some(file),
        }
    }

    #[test]
    fn parse_flag_patch_request_builds_node_request_for_direct_symbol_targeting() {
        let mut args = base_args(PathBuf::from("fixture.py"));
        args.kind = Some("function_definition".to_string());
        args.name = Some("process_*".to_string());
        args.replace = Some(Some("def process_data():\n    return 1\n".to_string()));
        args.dry_run = true;
        args.verbose = true;

        let request = parse_flag_patch_request(&args).expect("node request should parse");
        let FlagPatchRequest::Node(NodeFlagPatchRequest {
            selector,
            operation,
            execution,
            ..
        }) = request
        else {
            panic!("expected node request");
        };

        assert_eq!(
            selector,
            NodeTargetSelector::Selector {
                kind: "function_definition".to_string(),
                name_pattern: "process_*".to_string(),
            }
        );
        assert_eq!(
            operation,
            PreparedNodePatchOperation::Standard(OpKind::Replace {
                new_text: "def process_data():\n    return 1\n".to_string(),
            })
        );
        assert_eq!(
            execution,
            ApplyBackedExecution {
                dry_run: true,
                verbose: true,
            }
        );
    }

    #[test]
    fn parse_flag_patch_request_builds_line_request_with_line_execution_options() {
        let mut args = base_args(PathBuf::from("fixture.py"));
        args.at = Some("12:0123456789ab".to_string());
        args.set_line = Some(Some("replacement".to_string()));
        args.auto_repair = true;
        args.dry_run = true;

        let request = parse_flag_patch_request(&args).expect("line request should parse");
        let FlagPatchRequest::Line(line_request) = request else {
            panic!("expected line request");
        };

        match line_request.edit {
            HashlineEdit::SetLine { set_line } => {
                assert_eq!(set_line.anchor, "12:0123456789ab");
                assert_eq!(set_line.new_text, "replacement");
            }
            other => panic!("expected set-line edit, got {other:?}"),
        }
        assert_eq!(
            line_request.execution,
            LineExecution {
                dry_run: true,
                auto_repair: true,
            }
        );
    }

    #[test]
    fn parse_flag_patch_request_builds_canonical_request_for_file_insert() {
        let temp = NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), "body\n").expect("write source");

        let mut args = base_args(temp.path().to_path_buf());
        args.at = Some("file-end".to_string());
        args.insert = Some(Some("\n# tail\n".to_string()));
        args.verbose = true;

        let request = parse_flag_patch_request(&args).expect("file request should parse");
        let FlagPatchRequest::Canonical(request) = request else {
            panic!("expected canonical request");
        };

        assert_eq!(request.file, temp.path());
        assert_eq!(
            request.op,
            OpKind::Insert {
                new_text: "\n# tail\n".to_string(),
            }
        );
        assert_eq!(
            request.execution,
            ApplyBackedExecution {
                dry_run: false,
                verbose: true,
            }
        );
        match request.target {
            TransformTarget::FileEnd { expected_file_hash } => {
                assert_eq!(expected_file_hash, crate::hash::hash_bytes(b"body\n"));
            }
            other => panic!("expected file-end target, got {other:?}"),
        }
    }
}
