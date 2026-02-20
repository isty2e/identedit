use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::Deserialize;
use serde_json::Value;

use crate::changeset::{FileChange, MultiFileChangeset, OpKind, TransformTarget, hash_text};
use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::handle::Span;
use crate::transform::{
    TransformInstruction, build_changeset, build_delete_changeset, build_replace_changeset,
    parse_handles_for_file, resolve_target_in_handles,
};

#[derive(Debug, Args)]
pub struct EditBuildArgs {
    #[arg(
        long,
        value_name = "IDENTITY",
        help = "Target identity from read output (flag mode only)"
    )]
    pub identity: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        help = "Replacement text for the target (--identity mode)"
    )]
    pub replace: Option<String>,
    #[arg(long, help = "Delete the target node (--identity mode)")]
    pub delete: bool,
    #[arg(long, help = "Read edit request JSON from stdin")]
    pub json: bool,
    #[arg(
        long,
        help = "Emit verbose preview fields (old_text) instead of compact fields"
    )]
    pub verbose: bool,
    #[arg(
        value_name = "FILE",
        help = "Input file in flag mode; omit when using --json"
    )]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinEditRequestWire {
    command: String,
    #[serde(default)]
    file: Option<PathBuf>,
    #[serde(default)]
    operations: Option<Vec<StdinEditOperationWire>>,
    #[serde(default)]
    handle_table: Option<StdinHandleTableWire>,
    #[serde(default)]
    files: Option<Vec<StdinEditFileWire>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinEditFileWire {
    file: PathBuf,
    operations: Vec<StdinEditOperationWire>,
    #[serde(default)]
    handle_table: Option<StdinHandleTableWire>,
}

#[derive(Debug)]
struct StdinEditFileRequest {
    file: PathBuf,
    operations: Vec<StdinEditOperationWire>,
    handle_table: Option<StdinHandleTableWire>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum StdinEditOp {
    Replace {
        new_text: String,
    },
    Delete,
    InsertBefore {
        new_text: String,
    },
    InsertAfter {
        new_text: String,
    },
    Insert {
        new_text: String,
    },
    SetLine {
        new_text: String,
    },
    #[serde(alias = "replace_range")]
    ReplaceLines {
        new_text: String,
    },
    #[serde(rename = "insert_after_line", alias = "line_insert_after")]
    InsertAfterLine {
        text: String,
    },
    MoveBefore {
        destination: Value,
    },
    MoveAfter {
        destination: Value,
    },
    MoveToBefore {
        destination_file: PathBuf,
        destination: Value,
    },
    MoveToAfter {
        destination_file: PathBuf,
        destination: Value,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinEditOperationWire {
    #[serde(default)]
    target: Option<Value>,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    span_hint: Option<Span>,
    #[serde(default)]
    expected_old_hash: Option<String>,
    op: StdinEditOp,
}

type StdinHandleTableWire = BTreeMap<String, StdinHandleTableEntryWire>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StdinHandleTableEntryWire {
    identity: String,
    kind: String,
    #[serde(default)]
    span_hint: Option<Span>,
    expected_old_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HandleRefTargetWire {
    #[serde(rename = "type")]
    target_type: String,
    r#ref: String,
}

#[derive(Debug)]
enum ParsedOperationKind {
    Canonical(OpKind),
    MoveToBefore {
        destination_file: PathBuf,
        destination: TransformTarget,
    },
    MoveToAfter {
        destination_file: PathBuf,
        destination: TransformTarget,
    },
}

#[derive(Debug)]
struct ParsedEditInstruction {
    target: TransformTarget,
    op: ParsedOperationKind,
}

#[derive(Debug)]
struct FileInstructionBucket {
    file: PathBuf,
    instructions: Vec<TransformInstruction>,
}

#[derive(Debug, Default)]
struct NormalizeState {
    buckets: Vec<FileInstructionBucket>,
    bucket_index: HashMap<PathBuf, usize>,
    handle_cache: HashMap<PathBuf, Vec<SelectionHandle>>,
}

impl NormalizeState {
    fn ensure_file_bucket(&mut self, file: PathBuf) {
        if self.bucket_index.contains_key(&file) {
            return;
        }

        let index = self.buckets.len();
        self.buckets.push(FileInstructionBucket {
            file: file.clone(),
            instructions: Vec::new(),
        });
        self.bucket_index.insert(file, index);
    }

    fn push_instruction_for_file(&mut self, file: PathBuf, instruction: TransformInstruction) {
        if let Some(index) = self.bucket_index.get(&file).copied() {
            self.buckets[index].instructions.push(instruction);
            return;
        }

        let index = self.buckets.len();
        self.buckets.push(FileInstructionBucket {
            file: file.clone(),
            instructions: vec![instruction],
        });
        self.bucket_index.insert(file, index);
    }

    fn resolve_move_endpoint(
        &mut self,
        file: &Path,
        target: &TransformTarget,
    ) -> Result<SelectionHandle, IdenteditError> {
        let key = file.to_path_buf();
        let handles = if let Some(existing) = self.handle_cache.get(&key) {
            existing.clone()
        } else {
            let parsed = parse_handles_for_file(file)?;
            self.handle_cache.insert(key.clone(), parsed.clone());
            parsed
        };
        resolve_target_in_handles(file, &handles, target)
    }

    fn normalize_cross_file_move_operation(
        &mut self,
        source_file: &Path,
        source_target: TransformTarget,
        destination_file: PathBuf,
        destination_target: TransformTarget,
        insert_before: bool,
    ) -> Result<(), IdenteditError> {
        if !matches!(source_target, TransformTarget::Node { .. }) {
            return Err(IdenteditError::InvalidRequest {
                message: "cross-file move source must be a node target".to_string(),
            });
        }
        if !matches!(destination_target, TransformTarget::Node { .. }) {
            return Err(IdenteditError::InvalidRequest {
                message: "cross-file move destination must be a node target".to_string(),
            });
        }
        if files_refer_to_same_target(source_file, destination_file.as_path())? {
            return Err(IdenteditError::InvalidRequest {
                message: "cross-file move source and destination resolve to the same file; use move_before or move_after for same-file reordering".to_string(),
            });
        }

        let source_handle = self.resolve_move_endpoint(source_file, &source_target)?;
        let moved_text = source_handle.text;

        let _ = self.resolve_move_endpoint(destination_file.as_path(), &destination_target)?;

        self.push_instruction_for_file(
            source_file.to_path_buf(),
            TransformInstruction {
                target: source_target,
                op: OpKind::Delete,
            },
        );
        self.push_instruction_for_file(
            destination_file,
            TransformInstruction {
                target: destination_target,
                op: if insert_before {
                    OpKind::InsertBefore {
                        new_text: moved_text,
                    }
                } else {
                    OpKind::InsertAfter {
                        new_text: moved_text,
                    }
                },
            },
        );

        Ok(())
    }
}

pub fn run_edit_build(args: EditBuildArgs) -> Result<MultiFileChangeset, IdenteditError> {
    if args.json {
        return run_edit_json_mode(args.verbose);
    }

    let file = args.file.ok_or_else(|| IdenteditError::InvalidRequest {
        message: "FILE is required unless --json mode is enabled".to_string(),
    })?;
    let identity = args
        .identity
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "--identity is required unless --json mode is enabled".to_string(),
        })?;

    if args.replace.is_some() && args.delete {
        return Err(IdenteditError::InvalidRequest {
            message: "--replace and --delete cannot be used together".to_string(),
        });
    }

    if let Some(replacement) = args.replace {
        let file_change = build_replace_changeset(&file, &identity, replacement)?;
        let mut changeset = wrap_single_file(file_change);
        apply_preview_mode(&mut changeset, args.verbose);
        return Ok(changeset);
    }

    if args.delete {
        let file_change = build_delete_changeset(&file, &identity)?;
        let mut changeset = wrap_single_file(file_change);
        apply_preview_mode(&mut changeset, args.verbose);
        return Ok(changeset);
    }

    Err(IdenteditError::InvalidRequest {
        message: "--replace or --delete is required unless --json mode is enabled".to_string(),
    })
}

fn run_edit_json_mode(verbose: bool) -> Result<MultiFileChangeset, IdenteditError> {
    let mut request_body = String::new();
    std::io::stdin()
        .read_to_string(&mut request_body)
        .map_err(|error| IdenteditError::StdinRead { source: error })?;

    let request: StdinEditRequestWire = serde_json::from_str(&request_body)
        .map_err(|error| IdenteditError::InvalidJsonRequest { source: error })?;

    if request.command != "edit" {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Unsupported command '{}' in stdin JSON mode; expected 'edit'",
                request.command
            ),
        });
    }

    let file_requests = parse_stdin_edit_shape(request)?;
    let normalized_buckets = normalize_edit_file_requests(file_requests)?;
    let mut files = Vec::with_capacity(normalized_buckets.len());
    for bucket in normalized_buckets {
        if bucket.instructions.is_empty() {
            files.push(FileChange {
                file: bucket.file,
                operations: Vec::new(),
            });
        } else {
            files.push(build_changeset(&bucket.file, bucket.instructions)?);
        }
    }

    let mut changeset = MultiFileChangeset {
        files,
        transaction: Default::default(),
    };
    apply_preview_mode(&mut changeset, verbose);
    Ok(changeset)
}

fn normalize_edit_file_requests(
    file_requests: Vec<StdinEditFileRequest>,
) -> Result<Vec<FileInstructionBucket>, IdenteditError> {
    let mut state = NormalizeState::default();

    for file_request in file_requests {
        let source_file = file_request.file;
        let handle_table = file_request.handle_table;
        if file_request.operations.is_empty() {
            validate_noop_file_path(source_file.as_path())?;
            state.ensure_file_bucket(source_file);
            continue;
        }
        for operation in file_request.operations {
            let parsed = parse_edit_operation(operation, handle_table.as_ref())?;
            match parsed.op {
                ParsedOperationKind::Canonical(op) => state.push_instruction_for_file(
                    source_file.clone(),
                    TransformInstruction {
                        target: parsed.target,
                        op,
                    },
                ),
                ParsedOperationKind::MoveToBefore {
                    destination_file,
                    destination,
                } => state.normalize_cross_file_move_operation(
                    source_file.as_path(),
                    parsed.target,
                    destination_file,
                    destination,
                    true,
                )?,
                ParsedOperationKind::MoveToAfter {
                    destination_file,
                    destination,
                } => state.normalize_cross_file_move_operation(
                    source_file.as_path(),
                    parsed.target,
                    destination_file,
                    destination,
                    false,
                )?,
            }
        }
    }

    Ok(state.buckets)
}

fn validate_noop_file_path(file: &Path) -> Result<(), IdenteditError> {
    std::fs::read(file)
        .map(|_| ())
        .map_err(|error| IdenteditError::io(file, error))
}

fn files_refer_to_same_target(source: &Path, destination: &Path) -> Result<bool, IdenteditError> {
    if source == destination {
        return Ok(true);
    }
    let source_canonical =
        std::fs::canonicalize(source).map_err(|error| IdenteditError::io(source, error))?;
    let destination_canonical = match std::fs::canonicalize(destination) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(IdenteditError::io(destination, error)),
    };
    Ok(source_canonical == destination_canonical)
}

fn parse_stdin_edit_shape(
    request: StdinEditRequestWire,
) -> Result<Vec<StdinEditFileRequest>, IdenteditError> {
    let has_single =
        request.file.is_some() || request.operations.is_some() || request.handle_table.is_some();
    let has_batch = request.files.is_some();

    if has_single && has_batch {
        return Err(IdenteditError::InvalidRequest {
            message: "edit JSON request cannot include both 'file' and 'files' shapes; batch field 'files' cannot be combined with single-file fields ('file', 'operations', 'handle_table')".to_string(),
        });
    }

    if has_batch {
        let files = request.files.expect("checked has_batch");
        if files.is_empty() {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "edit JSON request field 'files' must contain at least one file entry"
                        .to_string(),
            });
        }
        return Ok(files
            .into_iter()
            .map(|entry| StdinEditFileRequest {
                file: entry.file,
                operations: entry.operations,
                handle_table: entry.handle_table,
            })
            .collect());
    }

    if request.file.is_none() && request.operations.is_none() {
        return Err(IdenteditError::InvalidRequest {
            message:
                "edit JSON request must include either single-file ('file' + 'operations') or batch ('files') shape"
                    .to_string(),
        });
    }

    let file = request.file.ok_or_else(|| IdenteditError::InvalidRequest {
        message: "edit JSON single-file shape is missing field 'file'".to_string(),
    })?;
    let operations = request
        .operations
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "edit JSON single-file shape is missing field 'operations'".to_string(),
        })?;
    Ok(vec![StdinEditFileRequest {
        file,
        operations,
        handle_table: request.handle_table,
    }])
}

fn wrap_single_file(file_change: FileChange) -> MultiFileChangeset {
    MultiFileChangeset {
        files: vec![file_change],
        transaction: Default::default(),
    }
}

fn parse_edit_operation(
    operation: StdinEditOperationWire,
    handle_table: Option<&StdinHandleTableWire>,
) -> Result<ParsedEditInstruction, IdenteditError> {
    if let Some(target_wire) = operation.target {
        if operation.identity.is_some()
            || operation.kind.is_some()
            || operation.span_hint.is_some()
            || operation.expected_old_hash.is_some()
        {
            return Err(IdenteditError::InvalidRequest {
                message: "operation.target cannot be combined with legacy identity/kind/span_hint/expected_old_hash fields".to_string(),
            });
        }

        let target = parse_edit_target_from_wire(target_wire, handle_table)?;
        return Ok(ParsedEditInstruction {
            target,
            op: parse_stdin_operation_kind(operation.op, handle_table)?,
        });
    }

    let identity = operation
        .identity
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "missing field `identity`".to_string(),
        })?;
    let kind = operation
        .kind
        .ok_or_else(|| IdenteditError::InvalidRequest {
            message: "missing field `kind`".to_string(),
        })?;
    let expected_old_hash =
        operation
            .expected_old_hash
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: "missing field `expected_old_hash`".to_string(),
            })?;

    Ok(ParsedEditInstruction {
        target: TransformTarget::node(identity, kind, operation.span_hint, expected_old_hash),
        op: parse_stdin_operation_kind(operation.op, handle_table)?,
    })
}

fn parse_edit_target_from_wire(
    target_wire: Value,
    handle_table: Option<&StdinHandleTableWire>,
) -> Result<TransformTarget, IdenteditError> {
    let target_type = target_wire.get("type").and_then(Value::as_str);
    if target_type == Some("handle_ref") {
        let handle_ref: HandleRefTargetWire =
            serde_json::from_value(target_wire).map_err(|error| {
                IdenteditError::InvalidRequest {
                    message: format!("invalid handle_ref target payload: {error}"),
                }
            })?;
        if handle_ref.target_type != "handle_ref" {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "unsupported operation target type '{}'",
                    handle_ref.target_type
                ),
            });
        }
        let table = handle_table.ok_or_else(|| IdenteditError::InvalidRequest {
            message:
                "handle_ref target requires file-scoped 'handle_table' in the same request shape"
                    .to_string(),
        })?;
        let entry = table
            .get(&handle_ref.r#ref)
            .ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "unknown handle_ref '{}'; add it to this file's handle_table",
                    handle_ref.r#ref
                ),
            })?;
        return Ok(TransformTarget::node(
            entry.identity.clone(),
            entry.kind.clone(),
            entry.span_hint,
            entry.expected_old_hash.clone(),
        ));
    }

    serde_json::from_value::<TransformTarget>(target_wire).map_err(|error| {
        IdenteditError::InvalidRequest {
            message: format!("invalid operation target: {error}"),
        }
    })
}

fn parse_stdin_operation_kind(
    operation: StdinEditOp,
    handle_table: Option<&StdinHandleTableWire>,
) -> Result<ParsedOperationKind, IdenteditError> {
    let parsed = match operation {
        StdinEditOp::Replace { new_text } => {
            ParsedOperationKind::Canonical(OpKind::Replace { new_text })
        }
        StdinEditOp::Delete => ParsedOperationKind::Canonical(OpKind::Delete),
        StdinEditOp::InsertBefore { new_text } => {
            ParsedOperationKind::Canonical(OpKind::InsertBefore { new_text })
        }
        StdinEditOp::InsertAfter { new_text } => {
            ParsedOperationKind::Canonical(OpKind::InsertAfter { new_text })
        }
        StdinEditOp::Insert { new_text } => {
            ParsedOperationKind::Canonical(OpKind::Insert { new_text })
        }
        StdinEditOp::SetLine { new_text } => {
            ParsedOperationKind::Canonical(OpKind::Replace { new_text })
        }
        StdinEditOp::ReplaceLines { new_text } => {
            ParsedOperationKind::Canonical(OpKind::Replace { new_text })
        }
        StdinEditOp::InsertAfterLine { text } => {
            ParsedOperationKind::Canonical(OpKind::InsertAfter { new_text: text })
        }
        StdinEditOp::MoveBefore { destination } => {
            ParsedOperationKind::Canonical(OpKind::MoveBefore {
                destination: Box::new(parse_edit_target_from_wire(destination, handle_table)?),
            })
        }
        StdinEditOp::MoveAfter { destination } => {
            ParsedOperationKind::Canonical(OpKind::MoveAfter {
                destination: Box::new(parse_edit_target_from_wire(destination, handle_table)?),
            })
        }
        StdinEditOp::MoveToBefore {
            destination_file,
            destination,
        } => ParsedOperationKind::MoveToBefore {
            destination_file,
            destination: parse_edit_target_from_wire(destination, handle_table)?,
        },
        StdinEditOp::MoveToAfter {
            destination_file,
            destination,
        } => ParsedOperationKind::MoveToAfter {
            destination_file,
            destination: parse_edit_target_from_wire(destination, handle_table)?,
        },
    };
    Ok(parsed)
}

fn apply_preview_mode(changeset: &mut MultiFileChangeset, verbose: bool) {
    for file in &mut changeset.files {
        for operation in &mut file.operations {
            if verbose {
                operation.preview.old_hash = None;
                operation.preview.old_len = None;
                if operation.preview.old_text.is_none() {
                    operation.preview.old_text = Some(String::new());
                }
            } else {
                let old_text = operation.preview.old_text.clone().unwrap_or_default();
                operation.preview.old_hash = Some(hash_text(&old_text));
                operation.preview.old_len = Some(old_text.len());
                operation.preview.old_text = None;
            }
        }
    }
}
