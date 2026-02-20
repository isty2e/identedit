use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::IdenteditError;
use crate::hash::hash_text;
use crate::hashline::{
    HashedLine, HashlineApplyError, HashlineApplyMode, HashlineApplyResult, HashlineCheckError,
    HashlineCheckResult, HashlineCheckSummary, HashlineEdit, HashlineMismatch,
    HashlineMismatchStatus, apply_hashline_edits_with_mode, check_hashline_edits,
    format_hashed_lines, show_hashed_lines,
};
use crate::patch::engine::run_resolve_verify_apply;

#[derive(Debug, Args)]
pub struct HashlineArgs {
    #[command(subcommand)]
    pub command: HashlineCommands,
}

#[derive(Debug, Subcommand)]
pub enum HashlineCommands {
    #[command(about = "Show hashed line anchors for a file")]
    Show(HashlineShowArgs),
    #[command(about = "Check whether edit anchors are still valid")]
    Check(HashlineCheckArgs),
    #[command(about = "Apply line-anchored edits in strict or repair mode")]
    Apply(HashlineApplyArgs),
    #[command(about = "One-shot hashline patch (strict check, optional single repair retry)")]
    Patch(HashlinePatchArgs),
}

#[derive(Debug, Args)]
pub struct HashlineShowArgs {
    #[arg(long, help = "Output structured JSON instead of plain hashline text")]
    pub json: bool,
    #[arg(value_name = "FILE", help = "File to inspect")]
    pub file: PathBuf,
}

#[derive(Debug, Args)]
pub struct HashlineCheckArgs {
    #[arg(value_name = "FILE", help = "File to validate edits against")]
    pub file: PathBuf,
    #[arg(
        long,
        value_name = "PATH_OR_DASH",
        help = "Edits JSON path, or '-' to read edits from stdin"
    )]
    pub edits: String,
    #[arg(long, help = "Include mismatch details even when check passes")]
    pub verbose: bool,
}

#[derive(Debug, Args)]
pub struct HashlineApplyArgs {
    #[arg(value_name = "FILE", help = "File to edit")]
    pub file: PathBuf,
    #[arg(
        long,
        value_name = "PATH_OR_DASH",
        help = "Edits JSON path, or '-' to read edits from stdin"
    )]
    pub edits: String,
    #[arg(long, help = "Do not write files; return preview result only")]
    pub dry_run: bool,
    #[arg(long, help = "Enable remap/repair when anchors are stale")]
    pub repair: bool,
    #[arg(long, help = "Include full output content in dry-run response")]
    pub include_content: bool,
}

#[derive(Debug, Args)]
pub struct HashlinePatchArgs {
    #[arg(value_name = "FILE", help = "File to edit")]
    pub file: PathBuf,
    #[arg(
        long,
        value_name = "PATH_OR_DASH",
        help = "Edits JSON path, or '-' to read edits from stdin"
    )]
    pub edits: String,
    #[arg(
        long,
        help = "If strict check fails with deterministic remap candidates, run one repair retry"
    )]
    pub auto_repair: bool,
}

pub enum HashlineCommandOutput {
    Text(String),
    Json(HashlineResponse),
}

#[derive(Debug, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum HashlineResponse {
    Show(HashlineShowResponse),
    Check(HashlineCheckResponse),
    Apply(HashlineApplyResponse),
    Patch(HashlinePatchResponse),
}

#[derive(Debug, Serialize)]
pub struct HashlineShowResponse {
    pub file: PathBuf,
    pub lines: Vec<HashedLine>,
    pub summary: HashlineShowSummary,
}

#[derive(Debug, Serialize)]
pub struct HashlineShowSummary {
    pub total_lines: usize,
}

#[derive(Debug, Serialize)]
pub struct HashlineCheckResponse {
    pub file: PathBuf,
    pub check: HashlineCheckPayload,
}

#[derive(Debug, Serialize)]
pub struct HashlineCheckPayload {
    pub ok: bool,
    pub summary: HashlineCheckSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mismatches: Option<Vec<HashlineMismatch>>,
}

#[derive(Debug, Serialize)]
pub struct HashlineApplyResponse {
    pub file: PathBuf,
    pub mode: HashlineModeResponse,
    pub dry_run: bool,
    pub changed: bool,
    pub operations_total: usize,
    pub operations_applied: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct HashlinePatchResponse {
    pub file: PathBuf,
    pub auto_repair: bool,
    pub strict_check: HashlineCheckPayload,
    pub applied_mode: HashlineModeResponse,
    pub changed: bool,
    pub operations_total: usize,
    pub operations_applied: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HashlineModeResponse {
    Strict,
    Repair,
}

pub fn run_hashline(args: HashlineArgs) -> Result<HashlineCommandOutput, IdenteditError> {
    match args.command {
        HashlineCommands::Show(show_args) => run_hashline_show(show_args),
        HashlineCommands::Check(check_args) => run_hashline_check(check_args),
        HashlineCommands::Apply(apply_args) => run_hashline_apply(apply_args),
        HashlineCommands::Patch(patch_args) => run_hashline_patch(patch_args),
    }
}

fn run_hashline_show(args: HashlineShowArgs) -> Result<HashlineCommandOutput, IdenteditError> {
    let source =
        fs::read_to_string(&args.file).map_err(|error| IdenteditError::io(&args.file, error))?;
    if !args.json {
        return Ok(HashlineCommandOutput::Text(format_hashed_lines(&source)));
    }

    let lines = show_hashed_lines(&source);
    Ok(HashlineCommandOutput::Json(HashlineResponse::Show(
        HashlineShowResponse {
            file: args.file,
            summary: HashlineShowSummary {
                total_lines: lines.len(),
            },
            lines,
        },
    )))
}

fn run_hashline_check(args: HashlineCheckArgs) -> Result<HashlineCommandOutput, IdenteditError> {
    let source =
        fs::read_to_string(&args.file).map_err(|error| IdenteditError::io(&args.file, error))?;
    let edits = read_hashline_edits(&args.edits)?;
    let check_result = check_hashline_edits(&source, &edits).map_err(map_hashline_check_error)?;
    let check = build_hashline_check_payload(check_result, args.verbose);
    Ok(HashlineCommandOutput::Json(HashlineResponse::Check(
        HashlineCheckResponse {
            file: args.file,
            check,
        },
    )))
}

fn run_hashline_apply(args: HashlineApplyArgs) -> Result<HashlineCommandOutput, IdenteditError> {
    let source =
        fs::read_to_string(&args.file).map_err(|error| IdenteditError::io(&args.file, error))?;
    let edits = read_hashline_edits(&args.edits)?;
    let mode = if args.repair {
        HashlineApplyMode::Repair
    } else {
        HashlineApplyMode::Strict
    };
    let applied =
        apply_hashline_edits_with_mode(&source, &edits, mode).map_err(map_hashline_apply_error)?;

    let changed = source != applied.content;
    if changed && !args.dry_run {
        fs::write(&args.file, applied.content.as_bytes())
            .map_err(|error| IdenteditError::io(&args.file, error))?;
    }

    Ok(HashlineCommandOutput::Json(HashlineResponse::Apply(
        build_apply_response(
            args.file,
            args.dry_run,
            args.include_content,
            mode,
            changed,
            applied,
        ),
    )))
}

fn run_hashline_patch(args: HashlinePatchArgs) -> Result<HashlineCommandOutput, IdenteditError> {
    let edits = read_hashline_edits(&args.edits)?;
    let response = execute_hashline_patch(args.file, edits, args.auto_repair)?;
    Ok(HashlineCommandOutput::Json(HashlineResponse::Patch(
        response,
    )))
}

pub(crate) fn execute_hashline_patch(
    file: PathBuf,
    edits: Vec<HashlineEdit>,
    auto_repair: bool,
) -> Result<HashlinePatchResponse, IdenteditError> {
    run_resolve_verify_apply(
        || resolve_hashline_patch_request(file, edits, auto_repair),
        verify_hashline_patch_request,
        apply_hashline_patch_request,
    )
}

#[derive(Debug)]
struct ResolvedHashlinePatch {
    file: PathBuf,
    source: String,
    edits: Vec<HashlineEdit>,
    auto_repair: bool,
}

#[derive(Debug)]
struct VerifiedHashlinePatch {
    file: PathBuf,
    source: String,
    edits: Vec<HashlineEdit>,
    auto_repair: bool,
    strict_check_result: HashlineCheckResult,
    applied_mode: HashlineApplyMode,
}

fn resolve_hashline_patch_request(
    file: PathBuf,
    edits: Vec<HashlineEdit>,
    auto_repair: bool,
) -> Result<ResolvedHashlinePatch, IdenteditError> {
    let source = fs::read_to_string(&file).map_err(|error| IdenteditError::io(&file, error))?;
    Ok(ResolvedHashlinePatch {
        file,
        source,
        edits,
        auto_repair,
    })
}

fn verify_hashline_patch_request(
    resolved: ResolvedHashlinePatch,
) -> Result<VerifiedHashlinePatch, IdenteditError> {
    let strict_check_result = check_hashline_edits(&resolved.source, &resolved.edits)
        .map_err(map_hashline_check_error)?;

    let applied_mode = if strict_check_result.ok {
        HashlineApplyMode::Strict
    } else if resolved.auto_repair && can_retry_with_repair(&strict_check_result) {
        HashlineApplyMode::Repair
    } else {
        return Err(hashline_precondition_failed_error(strict_check_result));
    };

    Ok(VerifiedHashlinePatch {
        file: resolved.file,
        source: resolved.source,
        edits: resolved.edits,
        auto_repair: resolved.auto_repair,
        strict_check_result,
        applied_mode,
    })
}

fn apply_hashline_patch_request(
    verified: VerifiedHashlinePatch,
) -> Result<HashlinePatchResponse, IdenteditError> {
    let strict_check = build_hashline_check_payload(
        verified.strict_check_result.clone(),
        verified.auto_repair || !verified.strict_check_result.ok,
    );
    let applied =
        apply_hashline_edits_with_mode(&verified.source, &verified.edits, verified.applied_mode)
            .map_err(map_hashline_apply_error)?;
    let changed = verified.source != applied.content;

    if changed {
        fs::write(&verified.file, applied.content.as_bytes())
            .map_err(|error| IdenteditError::io(&verified.file, error))?;
    }

    Ok(HashlinePatchResponse {
        file: verified.file,
        auto_repair: verified.auto_repair,
        strict_check,
        applied_mode: match verified.applied_mode {
            HashlineApplyMode::Strict => HashlineModeResponse::Strict,
            HashlineApplyMode::Repair => HashlineModeResponse::Repair,
        },
        changed,
        operations_total: applied.operations_total,
        operations_applied: applied.operations_applied,
    })
}

fn build_hashline_check_payload(check: HashlineCheckResult, verbose: bool) -> HashlineCheckPayload {
    let HashlineCheckResult {
        ok,
        summary,
        mismatches,
    } = check;
    let include_mismatches = verbose || !ok;
    HashlineCheckPayload {
        ok,
        summary,
        mismatches: include_mismatches.then_some(mismatches),
    }
}

fn build_apply_response(
    file: PathBuf,
    dry_run: bool,
    include_content: bool,
    mode: HashlineApplyMode,
    changed: bool,
    applied: HashlineApplyResult,
) -> HashlineApplyResponse {
    let compact_output_hash = (dry_run && !include_content).then(|| hash_text(&applied.content));
    let compact_output_bytes = (dry_run && !include_content).then_some(applied.content.len());
    HashlineApplyResponse {
        file,
        mode: match mode {
            HashlineApplyMode::Strict => HashlineModeResponse::Strict,
            HashlineApplyMode::Repair => HashlineModeResponse::Repair,
        },
        dry_run,
        changed,
        operations_total: applied.operations_total,
        operations_applied: applied.operations_applied,
        content: if dry_run && include_content {
            Some(applied.content)
        } else {
            None
        },
        output_hash: compact_output_hash,
        output_bytes: compact_output_bytes,
    }
}

fn read_hashline_edits(path_or_dash: &str) -> Result<Vec<HashlineEdit>, IdenteditError> {
    let body = if path_or_dash == "-" {
        read_stdin_text()?
    } else {
        let path = PathBuf::from(path_or_dash);
        fs::read_to_string(&path).map_err(|error| IdenteditError::io(&path, error))?
    };

    parse_hashline_edits(&body)
}

fn read_stdin_text() -> Result<String, IdenteditError> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| IdenteditError::StdinRead { source: error })?;
    Ok(input)
}

fn parse_hashline_edits(body: &str) -> Result<Vec<HashlineEdit>, IdenteditError> {
    let parsed: Value = serde_json::from_str(body)
        .map_err(|source| IdenteditError::InvalidJsonRequest { source })?;

    match parsed {
        Value::Array(items) => parse_hashline_edit_array(items),
        Value::Object(map) => parse_hashline_wrapper(map),
        _ => Err(IdenteditError::InvalidRequest {
            message:
                "Invalid hashline stdin request: expected either an edit array or an object with 'command' and 'edits' fields"
                    .to_string(),
        }),
    }
}

fn parse_hashline_wrapper(map: Map<String, Value>) -> Result<Vec<HashlineEdit>, IdenteditError> {
    const COMMAND_FIELD: &str = "command";
    const EDITS_FIELD: &str = "edits";
    const ANCHORS_FIELD: &str = "anchors";
    const WRAPPER_EXPECTED_FIELDS: &str = "'command', 'edits', 'anchors'";

    for key in map.keys() {
        if key != COMMAND_FIELD && key != EDITS_FIELD && key != ANCHORS_FIELD {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Invalid hashline stdin request: unknown field '{key}', expected {WRAPPER_EXPECTED_FIELDS}"
                ),
            });
        }
    }

    let command_value = map
        .get(COMMAND_FIELD)
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "Invalid hashline stdin request: missing field 'command'".to_string(),
        })?;
    let command = command_value
        .as_str()
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: format!(
                "Invalid hashline stdin request: field 'command' must be a string, got {}",
                json_type_name(command_value)
            ),
        })?;

    if command != "hashline" {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Unsupported command '{}' for hashline stdin request; expected 'hashline'",
                command
            ),
        });
    }

    let edits_value = map
        .get(EDITS_FIELD)
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "Invalid hashline stdin request: missing field 'edits'".to_string(),
        })?;
    let edits_array =
        edits_value
            .as_array()
            .cloned()
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Invalid hashline stdin request: field 'edits' must be an array, got {}",
                    json_type_name(edits_value)
                ),
            })?;

    let anchors = parse_anchor_table(map.get(ANCHORS_FIELD))?;
    parse_hashline_edit_array_with_anchors(edits_array, anchors.as_ref())
}

fn parse_hashline_edit_array(items: Vec<Value>) -> Result<Vec<HashlineEdit>, IdenteditError> {
    parse_hashline_edit_array_with_anchors(items, None)
}

fn parse_hashline_edit_array_with_anchors(
    items: Vec<Value>,
    anchors: Option<&BTreeMap<String, String>>,
) -> Result<Vec<HashlineEdit>, IdenteditError> {
    let mut edits = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        let parsed = parse_hashline_edit(item, anchors).map_err(|message| {
            IdenteditError::InvalidRequest {
                message: format!("Invalid hashline edit at index {index}: {message}"),
            }
        })?;
        edits.push(parsed);
    }
    Ok(edits)
}

fn parse_hashline_edit(
    item: Value,
    anchors: Option<&BTreeMap<String, String>>,
) -> Result<HashlineEdit, String> {
    let object = item.as_object().ok_or_else(|| {
        format!(
            "expected an object with exactly one operation key, got {}",
            json_type_name(&item)
        )
    })?;

    if object.len() != 1 {
        return Err(format!(
            "expected exactly one operation key ('set_line', 'replace_lines', 'insert_after'), got {} keys",
            object.len()
        ));
    }

    let (operation, payload) = object.iter().next().expect("one key is required");
    match operation.as_str() {
        "set_line" => parse_set_line_edit(payload.clone(), anchors)
            .map(|set_line| HashlineEdit::SetLine { set_line }),
        "replace_lines" => parse_replace_lines_edit(payload.clone(), anchors)
            .map(|replace_lines| HashlineEdit::ReplaceLines { replace_lines }),
        "insert_after" => parse_insert_after_edit(payload.clone(), anchors)
            .map(|insert_after| HashlineEdit::InsertAfter { insert_after }),
        other => Err(format!(
            "unknown operation key '{other}', expected one of 'set_line', 'replace_lines', 'insert_after'"
        )),
    }
}

fn parse_anchor_table(
    value: Option<&Value>,
) -> Result<Option<BTreeMap<String, String>>, IdenteditError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: format!(
                "Invalid hashline stdin request: field 'anchors' must be an object, got {}",
                json_type_name(value)
            ),
        })?;

    let mut anchors = BTreeMap::new();
    for (key, raw_anchor) in object {
        let anchor = raw_anchor
            .as_str()
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "Invalid hashline stdin request: anchors['{key}'] must be a string, got {}",
                    json_type_name(raw_anchor)
                ),
            })?;
        anchors.insert(key.clone(), anchor.to_string());
    }
    Ok(Some(anchors))
}

fn parse_set_line_edit(
    payload: Value,
    anchors: Option<&BTreeMap<String, String>>,
) -> Result<crate::hashline::SetLineEdit, String> {
    let object = payload.as_object().ok_or_else(|| {
        format!(
            "set_line payload must be an object, got {}",
            json_type_name(&payload)
        )
    })?;

    let anchor = resolve_anchor_field(object, "set_line", "anchor", "anchor_ref", anchors)?;
    let new_text = object
        .get("new_text")
        .and_then(Value::as_str)
        .ok_or_else(|| "set_line requires string field 'new_text'".to_string())?;

    for key in object.keys() {
        if key != "anchor" && key != "anchor_ref" && key != "new_text" {
            return Err(format!("set_line unknown field '{key}'"));
        }
    }

    Ok(crate::hashline::SetLineEdit {
        anchor,
        new_text: new_text.to_string(),
    })
}

fn parse_replace_lines_edit(
    payload: Value,
    anchors: Option<&BTreeMap<String, String>>,
) -> Result<crate::hashline::ReplaceLinesEdit, String> {
    let object = payload.as_object().ok_or_else(|| {
        format!(
            "replace_lines payload must be an object, got {}",
            json_type_name(&payload)
        )
    })?;

    let start_anchor = resolve_anchor_field(
        object,
        "replace_lines",
        "start_anchor",
        "start_anchor_ref",
        anchors,
    )?;
    let end_anchor = resolve_optional_anchor_field(
        object,
        "replace_lines",
        "end_anchor",
        "end_anchor_ref",
        anchors,
    )?;
    let new_text = object
        .get("new_text")
        .and_then(Value::as_str)
        .ok_or_else(|| "replace_lines requires string field 'new_text'".to_string())?;

    for key in object.keys() {
        if key != "start_anchor"
            && key != "start_anchor_ref"
            && key != "end_anchor"
            && key != "end_anchor_ref"
            && key != "new_text"
        {
            return Err(format!("replace_lines unknown field '{key}'"));
        }
    }

    Ok(crate::hashline::ReplaceLinesEdit {
        start_anchor,
        end_anchor,
        new_text: new_text.to_string(),
    })
}

fn parse_insert_after_edit(
    payload: Value,
    anchors: Option<&BTreeMap<String, String>>,
) -> Result<crate::hashline::InsertAfterEdit, String> {
    let object = payload.as_object().ok_or_else(|| {
        format!(
            "insert_after payload must be an object, got {}",
            json_type_name(&payload)
        )
    })?;

    let anchor = resolve_anchor_field(object, "insert_after", "anchor", "anchor_ref", anchors)?;
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "insert_after requires string field 'text'".to_string())?;

    for key in object.keys() {
        if key != "anchor" && key != "anchor_ref" && key != "text" {
            return Err(format!("insert_after unknown field '{key}'"));
        }
    }

    Ok(crate::hashline::InsertAfterEdit {
        anchor,
        text: text.to_string(),
    })
}

fn resolve_anchor_field(
    object: &Map<String, Value>,
    operation: &str,
    raw_field: &str,
    ref_field: &str,
    anchors: Option<&BTreeMap<String, String>>,
) -> Result<String, String> {
    let raw_value = object.get(raw_field).and_then(Value::as_str);
    let ref_value = object.get(ref_field).and_then(Value::as_str);

    if raw_value.is_some() && ref_value.is_some() {
        return Err(format!(
            "{operation} cannot contain both '{raw_field}' and '{ref_field}'"
        ));
    }
    if let Some(anchor) = raw_value {
        return Ok(anchor.to_string());
    }
    if let Some(anchor_ref) = ref_value {
        let table = anchors.ok_or_else(|| {
            format!("{operation} field '{ref_field}' requires top-level 'anchors' table")
        })?;
        let resolved = table
            .get(anchor_ref)
            .ok_or_else(|| format!("unknown anchor_ref '{anchor_ref}' in field '{ref_field}'"))?;
        return Ok(resolved.clone());
    }

    Err(format!(
        "{operation} requires one of '{raw_field}' or '{ref_field}'"
    ))
}

fn resolve_optional_anchor_field(
    object: &Map<String, Value>,
    operation: &str,
    raw_field: &str,
    ref_field: &str,
    anchors: Option<&BTreeMap<String, String>>,
) -> Result<Option<String>, String> {
    let raw_value = object.get(raw_field).and_then(Value::as_str);
    let ref_value = object.get(ref_field).and_then(Value::as_str);

    if raw_value.is_some() && ref_value.is_some() {
        return Err(format!(
            "{operation} cannot contain both '{raw_field}' and '{ref_field}'"
        ));
    }
    if let Some(anchor) = raw_value {
        return Ok(Some(anchor.to_string()));
    }
    if let Some(anchor_ref) = ref_value {
        let table = anchors.ok_or_else(|| {
            format!("{operation} field '{ref_field}' requires top-level 'anchors' table")
        })?;
        let resolved = table
            .get(anchor_ref)
            .ok_or_else(|| format!("unknown anchor_ref '{anchor_ref}' in field '{ref_field}'"))?;
        return Ok(Some(resolved.clone()));
    }

    Ok(None)
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn map_hashline_check_error(error: HashlineCheckError) -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: error.to_string(),
    }
}

fn map_hashline_apply_error(error: HashlineApplyError) -> IdenteditError {
    match error {
        HashlineApplyError::Check(check_error) => map_hashline_check_error(check_error),
        HashlineApplyError::Overlap { .. } => IdenteditError::InvalidRequest {
            message: error.to_string(),
        },
        HashlineApplyError::PreconditionFailed { check } => {
            hashline_precondition_failed_error(check)
        }
    }
}

fn hashline_precondition_failed_error(check: HashlineCheckResult) -> IdenteditError {
    let diagnostic_check = canonicalize_check_for_diagnostics(check);
    let serialized_check = serde_json::to_string_pretty(&diagnostic_check)
        .unwrap_or_else(|_| "{\"ok\":false}".to_string());
    IdenteditError::InvalidRequest {
        message: format!(
            "Hashline preconditions failed; refresh anchors with 'identedit read --mode line --json <file>' and retry.\n{serialized_check}"
        ),
    }
}

fn can_retry_with_repair(check: &HashlineCheckResult) -> bool {
    check.summary.remappable > 0
        && check.summary.ambiguous == 0
        && check.summary.mismatched == check.summary.remappable
}

fn canonicalize_check_for_diagnostics(mut check: HashlineCheckResult) -> HashlineCheckResult {
    for mismatch in &mut check.mismatches {
        mismatch.remaps.sort_by(|left, right| {
            left.line
                .cmp(&right.line)
                .then_with(|| left.hash.cmp(&right.hash))
        });
    }

    check.mismatches.sort_by(|left, right| {
        left.edit_index
            .cmp(&right.edit_index)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| {
                mismatch_status_rank(&left.status).cmp(&mismatch_status_rank(&right.status))
            })
            .then_with(|| left.anchor.cmp(&right.anchor))
            .then_with(|| left.expected_hash.cmp(&right.expected_hash))
            .then_with(|| left.actual_hash.cmp(&right.actual_hash))
            .then_with(|| left.remaps.len().cmp(&right.remaps.len()))
    });

    check
}

fn mismatch_status_rank(status: &HashlineMismatchStatus) -> u8 {
    match status {
        HashlineMismatchStatus::Mismatch => 0,
        HashlineMismatchStatus::Remappable => 1,
        HashlineMismatchStatus::Ambiguous => 2,
    }
}
