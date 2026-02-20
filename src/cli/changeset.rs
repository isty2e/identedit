use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::changeset::{ChangeOp, FileChange, MultiFileChangeset, OpKind};
use crate::error::IdenteditError;

pub fn run_changeset_merge_inputs(
    inputs: Vec<PathBuf>,
) -> Result<MultiFileChangeset, IdenteditError> {
    let mut merged_by_file = BTreeMap::<String, FileChange>::new();

    for input in &inputs {
        let content =
            std::fs::read_to_string(input).map_err(|error| IdenteditError::io(input, error))?;
        let changeset: MultiFileChangeset = serde_json::from_str(&content)
            .map_err(|source| IdenteditError::InvalidJsonRequest { source })?;

        for file_change in changeset.files {
            let file_key = normalize_file_key(&file_change.file)?;
            let entry = merged_by_file
                .entry(file_key)
                .or_insert_with(|| FileChange {
                    file: file_change.file.clone(),
                    operations: Vec::new(),
                });
            entry.operations.extend(file_change.operations);
        }
    }

    let mut files = Vec::with_capacity(merged_by_file.len());
    for mut file_change in merged_by_file.into_values() {
        validate_file_merge_constraints(&file_change.file, &file_change.operations)?;
        if file_change.operations.is_empty() {
            continue;
        }
        files.push(FileChange {
            file: file_change.file.clone(),
            operations: std::mem::take(&mut file_change.operations),
        });
    }

    if files.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "Merged changeset must contain at least one operation".to_string(),
        });
    }

    Ok(MultiFileChangeset {
        files,
        transaction: Default::default(),
    })
}

#[derive(Debug, Clone, Copy)]
struct SpanOp {
    index: usize,
    start: usize,
    end: usize,
    is_insert: bool,
}

fn validate_file_merge_constraints(
    file: &Path,
    operations: &[ChangeOp],
) -> Result<(), IdenteditError> {
    let move_count = operations
        .iter()
        .filter(|operation| matches!(operation.op, OpKind::Move { .. }))
        .count();
    let has_non_move = operations
        .iter()
        .any(|operation| !matches!(operation.op, OpKind::Move { .. }));

    if move_count > 1 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Strict merge rejected file '{}': multiple move operations cannot be merged for one file",
                file.display()
            ),
        });
    }

    if move_count == 1 && has_non_move {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Strict merge rejected file '{}': move cannot be merged with content-edit operations for the same file",
                file.display()
            ),
        });
    }

    let mut spans = Vec::<SpanOp>::new();
    for (index, operation) in operations.iter().enumerate() {
        if matches!(operation.op, OpKind::Move { .. }) {
            continue;
        }

        let span = operation.preview.matched_span;
        if span.start > span.end {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Strict merge rejected file '{}': operation {index} has invalid preview span [{}, {})",
                    file.display(),
                    span.start,
                    span.end,
                ),
            });
        }

        let is_insert = matches!(
            operation.op,
            OpKind::InsertBefore { .. } | OpKind::InsertAfter { .. } | OpKind::Insert { .. }
        );
        spans.push(SpanOp {
            index,
            start: span.start,
            end: span.end,
            is_insert,
        });
    }

    spans.sort_by_key(|entry| (entry.start, entry.end, entry.index));

    for window in spans.windows(2) {
        let first = window[0];
        let second = window[1];
        let has_overlap = first.end > second.start
            || (first.end == second.start && (first.is_insert || second.is_insert));
        if has_overlap {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "Strict merge rejected file '{}': conflicting operations {} [{}, {}) and {} [{}, {})",
                    file.display(),
                    first.index,
                    first.start,
                    first.end,
                    second.index,
                    second.start,
                    second.end,
                ),
            });
        }
    }

    Ok(())
}

fn normalize_file_key(path: &Path) -> Result<String, IdenteditError> {
    match std::fs::canonicalize(path) {
        Ok(canonical) => Ok(canonical.to_string_lossy().into_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()
                    .map_err(|current_dir_error| {
                        IdenteditError::io(Path::new("."), current_dir_error)
                    })?
                    .join(path)
            };
            Ok(normalize_lexical_path(&absolute)
                .to_string_lossy()
                .into_owned())
        }
        Err(error) => Err(IdenteditError::io(path, error)),
    }
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    if normalized.as_os_str().is_empty() {
        path.to_path_buf()
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::validate_file_merge_constraints;
    use crate::changeset::{ChangeOp, ChangePreview, OpKind, TransformTarget};
    use crate::error::IdenteditError;
    use crate::handle::Span;
    use std::path::Path;

    fn op(kind: OpKind, span: Span) -> ChangeOp {
        ChangeOp {
            target: TransformTarget::node(
                "id".to_string(),
                "function_definition".to_string(),
                Some(span),
                "hash".to_string(),
            ),
            op: kind,
            preview: ChangePreview {
                old_text: Some(String::new()),
                old_hash: None,
                old_len: None,
                new_text: String::new(),
                matched_span: span,
                move_preview: None,
            },
        }
    }

    #[test]
    fn strict_merge_allows_adjacent_non_insert_ranges() {
        let operations = vec![
            op(OpKind::Delete, Span { start: 0, end: 10 }),
            op(
                OpKind::Replace {
                    new_text: "x".to_string(),
                },
                Span { start: 10, end: 20 },
            ),
        ];

        validate_file_merge_constraints(Path::new("file.py"), &operations)
            .expect("adjacent non-insert ranges should be mergeable");
    }

    #[test]
    fn strict_merge_rejects_insert_touching_replace_boundary() {
        let operations = vec![
            op(
                OpKind::InsertAfter {
                    new_text: "x".to_string(),
                },
                Span { start: 10, end: 10 },
            ),
            op(OpKind::Delete, Span { start: 10, end: 20 }),
        ];

        let error = validate_file_merge_constraints(Path::new("file.py"), &operations)
            .expect_err("insert+replace boundary touch should be rejected");
        match error {
            IdenteditError::InvalidRequest { message } => {
                assert!(message.contains("conflicting operations"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn strict_merge_rejects_move_plus_edit() {
        let operations = vec![
            op(
                OpKind::Move {
                    to: "moved.py".into(),
                },
                Span { start: 0, end: 0 },
            ),
            op(OpKind::Delete, Span { start: 0, end: 10 }),
        ];

        let error = validate_file_merge_constraints(Path::new("file.py"), &operations)
            .expect_err("move+edit should be rejected");
        match error {
            IdenteditError::InvalidRequest { message } => {
                assert!(message.contains("move cannot be merged"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
