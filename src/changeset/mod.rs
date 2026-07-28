use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::handle::Span;
use crate::hash::ContentHash;
use crate::hashline::LineAnchor;

mod wire;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformTarget {
    Node {
        identity: String,
        kind: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        span_hint: Option<Span>,
        expected_old_hash: ContentHash,
    },
    FileStart {
        expected_file_hash: ContentHash,
    },
    FileEnd {
        expected_file_hash: ContentHash,
    },
    File {
        expected_file_hash: ContentHash,
    },
    Line {
        anchor: LineAnchor,
        #[serde(skip_serializing_if = "Option::is_none")]
        end_anchor: Option<LineAnchor>,
    },
}

impl TransformTarget {
    pub fn node(
        identity: String,
        kind: String,
        span_hint: Option<Span>,
        expected_old_hash: ContentHash,
    ) -> Self {
        Self::Node {
            identity,
            kind,
            span_hint,
            expected_old_hash,
        }
    }

    pub fn requires_node_resolution(&self) -> bool {
        matches!(self, Self::Node { .. })
    }

    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Self::Node { .. } => "node",
            Self::FileStart { .. } => "file_start",
            Self::FileEnd { .. } => "file_end",
            Self::File { .. } => "file",
            Self::Line { .. } => "line",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    pub file: PathBuf,
    pub operations: Vec<ChangeOp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiFileChangeset {
    pub files: Vec<FileChange>,
    #[serde(default)]
    pub transaction: TransactionSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct TransactionSpec {
    pub mode: TransactionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransactionMode {
    #[default]
    AllOrNothing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpKind {
    Replace { new_text: String },
    Delete,
    InsertBefore { new_text: String },
    InsertAfter { new_text: String },
    Insert { new_text: String },
    MoveBefore { destination: Box<TransformTarget> },
    MoveAfter { destination: Box<TransformTarget> },
    Move { to: PathBuf },
}

impl OpKind {
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Self::Replace { .. } => "replace",
            Self::Delete => "delete",
            Self::InsertBefore { .. } => "insert_before",
            Self::InsertAfter { .. } => "insert_after",
            Self::Insert { .. } => "insert",
            Self::MoveBefore { .. } => "move_before",
            Self::MoveAfter { .. } => "move_after",
            Self::Move { .. } => "move",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct EditOperation {
    target: TransformTarget,
    op: OpKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeOp {
    #[serde(flatten)]
    operation: EditOperation,
    preview: ChangePreview,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum OperationModelError {
    #[error(
        "unsupported target/op combination: '{target}' target cannot be used with '{operation}' operation"
    )]
    UnsupportedTargetOperation {
        target: &'static str,
        operation: &'static str,
    },
    #[error("invalid same-file move destination for '{operation}': {message}")]
    InvalidMoveDestination {
        operation: &'static str,
        message: &'static str,
    },
    #[error("invalid '{target}' target for '{operation}' operation: {message}")]
    InvalidTargetOperationDetails {
        target: &'static str,
        operation: &'static str,
        message: &'static str,
    },
    #[error("invalid preview for '{operation}' operation: expected {expected} preview fields")]
    InvalidPreviewFamily {
        operation: &'static str,
        expected: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ChangePreview {
    Text(TextChangePreview),
    Move(MoveChangePreview),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextChangePreview {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_hash: Option<ContentHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_len: Option<usize>,
    pub new_text: String,
    pub matched_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveChangePreview {
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "move")]
    pub move_preview: Option<MovePreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovePreview {
    pub from: PathBuf,
    pub to: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FileMoveOperationRef<'a> {
    pub(crate) expected_file_hash: &'a ContentHash,
    pub(crate) destination: &'a std::path::Path,
    pub(crate) preview: &'a MoveChangePreview,
}

impl EditOperation {
    pub(crate) fn try_new(
        target: TransformTarget,
        op: OpKind,
    ) -> Result<Self, OperationModelError> {
        if !valid_target_operation_pair(&target, &op) {
            return Err(OperationModelError::UnsupportedTargetOperation {
                target: target.kind_name(),
                operation: op.kind_name(),
            });
        }
        validate_target_operation_details(&target, &op)?;
        validate_same_file_move_destination(&op)?;

        Ok(Self { target, op })
    }

    pub(crate) fn target(&self) -> &TransformTarget {
        &self.target
    }

    pub(crate) fn op(&self) -> &OpKind {
        &self.op
    }
}

impl ChangeOp {
    pub(crate) fn try_new(
        operation: EditOperation,
        preview: ChangePreview,
    ) -> Result<Self, OperationModelError> {
        let preview = normalize_preview_family(operation.op(), preview)?;
        Ok(Self { operation, preview })
    }

    pub(crate) fn from_parts(
        target: TransformTarget,
        op: OpKind,
        preview: ChangePreview,
    ) -> Result<Self, OperationModelError> {
        Self::try_new(EditOperation::try_new(target, op)?, preview)
    }

    pub(crate) fn target(&self) -> &TransformTarget {
        self.operation.target()
    }

    pub(crate) fn operation(&self) -> &EditOperation {
        &self.operation
    }

    pub(crate) fn op(&self) -> &OpKind {
        self.operation.op()
    }

    pub(crate) fn preview(&self) -> &ChangePreview {
        &self.preview
    }

    pub(crate) fn text_preview(&self) -> Option<&TextChangePreview> {
        self.preview.as_text()
    }

    pub(crate) fn text_preview_mut(&mut self) -> Option<&mut TextChangePreview> {
        self.preview.as_text_mut()
    }

    pub(crate) fn as_file_move(&self) -> Option<FileMoveOperationRef<'_>> {
        match (self.target(), self.op(), self.preview()) {
            (
                TransformTarget::File { expected_file_hash },
                OpKind::Move { to },
                ChangePreview::Move(preview),
            ) => Some(FileMoveOperationRef {
                expected_file_hash,
                destination: to.as_path(),
                preview,
            }),
            _ => None,
        }
    }

    pub(crate) fn replace_target(
        &mut self,
        target: TransformTarget,
    ) -> Result<(), OperationModelError> {
        self.operation = EditOperation::try_new(target, self.operation.op.clone())?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn replace_preview(
        &mut self,
        preview: ChangePreview,
    ) -> Result<(), OperationModelError> {
        self.preview = normalize_preview_family(self.op(), preview)?;
        Ok(())
    }
}

impl ChangePreview {
    pub(crate) fn text(
        old_text: Option<String>,
        old_hash: Option<ContentHash>,
        old_len: Option<usize>,
        new_text: String,
        matched_span: Span,
    ) -> Self {
        Self::Text(TextChangePreview {
            old_text,
            old_hash,
            old_len,
            new_text,
            matched_span,
        })
    }

    #[cfg(test)]
    pub(crate) fn move_operation(move_preview: Option<MovePreview>) -> Self {
        Self::Move(MoveChangePreview { move_preview })
    }

    fn as_text(&self) -> Option<&TextChangePreview> {
        match self {
            Self::Text(preview) => Some(preview),
            Self::Move(_) => None,
        }
    }

    fn as_text_mut(&mut self) -> Option<&mut TextChangePreview> {
        match self {
            Self::Text(preview) => Some(preview),
            Self::Move(_) => None,
        }
    }
}

fn valid_target_operation_pair(target: &TransformTarget, op: &OpKind) -> bool {
    match target {
        TransformTarget::Node { .. } => matches!(
            op,
            OpKind::Replace { .. }
                | OpKind::Delete
                | OpKind::InsertBefore { .. }
                | OpKind::InsertAfter { .. }
                | OpKind::MoveBefore { .. }
                | OpKind::MoveAfter { .. }
        ),
        TransformTarget::FileStart { .. } | TransformTarget::FileEnd { .. } => {
            matches!(op, OpKind::Insert { .. })
        }
        TransformTarget::File { .. } => matches!(op, OpKind::Move { .. }),
        TransformTarget::Line { .. } => {
            matches!(op, OpKind::Replace { .. } | OpKind::InsertAfter { .. })
        }
    }
}

fn validate_same_file_move_destination(op: &OpKind) -> Result<(), OperationModelError> {
    let (operation, destination) = match op {
        OpKind::MoveBefore { destination } => ("move_before", destination.as_ref()),
        OpKind::MoveAfter { destination } => ("move_after", destination.as_ref()),
        _ => return Ok(()),
    };

    match destination {
        TransformTarget::Node { .. }
        | TransformTarget::FileStart { .. }
        | TransformTarget::FileEnd { .. }
        | TransformTarget::Line {
            end_anchor: None, ..
        } => Ok(()),
        TransformTarget::File { .. } => Err(OperationModelError::InvalidMoveDestination {
            operation,
            message: "whole-file targets cannot identify an in-file position",
        }),
        TransformTarget::Line {
            end_anchor: Some(_),
            ..
        } => Err(OperationModelError::InvalidMoveDestination {
            operation,
            message: "line destinations do not accept end_anchor",
        }),
    }
}

fn validate_target_operation_details(
    target: &TransformTarget,
    op: &OpKind,
) -> Result<(), OperationModelError> {
    if matches!(
        (target, op),
        (
            TransformTarget::Line {
                end_anchor: Some(_),
                ..
            },
            OpKind::InsertAfter { .. }
        )
    ) {
        return Err(OperationModelError::InvalidTargetOperationDetails {
            target: "line",
            operation: "insert_after",
            message: "end_anchor is only valid for replace operations",
        });
    }

    Ok(())
}

fn normalize_preview_family(
    op: &OpKind,
    preview: ChangePreview,
) -> Result<ChangePreview, OperationModelError> {
    match (op, preview) {
        (OpKind::Move { .. }, ChangePreview::Move(preview)) => Ok(ChangePreview::Move(preview)),
        (OpKind::Move { .. }, ChangePreview::Text(_)) => {
            Err(OperationModelError::InvalidPreviewFamily {
                operation: "move",
                expected: "move",
            })
        }
        (_, ChangePreview::Text(preview)) => Ok(ChangePreview::Text(preview)),
        (op, ChangePreview::Move(_)) => Err(OperationModelError::InvalidPreviewFamily {
            operation: op.kind_name(),
            expected: "text",
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::{Value, json};

    use super::{
        ChangeOp, ChangePreview, MovePreview, MultiFileChangeset, OpKind, TextChangePreview,
        TransactionMode, TransformTarget,
    };
    use crate::handle::Span;
    use crate::hash::{ContentHash, HASH_HEX_LEN, hash_text};

    #[test]
    fn multi_file_changeset_defaults_transaction_mode_to_all_or_nothing() {
        let payload = r#"{
            "files": [
                {
                    "file": "fixture.py",
                    "operations": []
                }
            ]
        }"#;

        let parsed: MultiFileChangeset =
            serde_json::from_str(payload).expect("v2 changeset should deserialize");

        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.transaction.mode, TransactionMode::AllOrNothing);
    }

    #[test]
    fn multi_file_changeset_rejects_unknown_transaction_field() {
        let payload = r#"{
            "files": [
                {
                    "file": "fixture.py",
                    "operations": []
                }
            ],
            "transaction": {
                "mode": "all_or_nothing",
                "unknown": true
            }
        }"#;

        let error = serde_json::from_str::<MultiFileChangeset>(payload)
            .expect_err("unknown fields must be rejected by strict mode");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn hash_text_uses_fixed_hex_prefix_length() {
        let hash = hash_text("def process_data(value):\n    return value + 1\n");
        assert_eq!(hash.len(), HASH_HEX_LEN);
        assert!(hash.chars().all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn multi_file_changeset_rejects_non_object_transaction_values() {
        let payloads = [
            r#"{"files":[{"file":"fixture.py","operations":[]}],"transaction":null}"#,
            r#"{"files":[{"file":"fixture.py","operations":[]}],"transaction":[]}"#,
            r#"{"files":[{"file":"fixture.py","operations":[]}],"transaction":1}"#,
        ];

        for payload in payloads {
            let error = serde_json::from_str::<MultiFileChangeset>(payload)
                .expect_err("transaction must deserialize from an object");
            assert!(error.to_string().contains("invalid type"));
        }
    }

    #[test]
    fn change_op_deserializes_move_kind() {
        let payload = r#"{
            "target": {
                "type": "file",
                "expected_file_hash": "0123456789abcdef"
            },
            "op": {
                "type": "move",
                "to": "renamed.py"
            },
            "preview": {
                "old_text": "",
                "new_text": "",
                "matched_span": {
                    "start": 0,
                    "end": 0
                },
                "move": {
                    "from": "fixture.py",
                    "to": "renamed.py"
                }
            }
        }"#;

        let parsed: ChangeOp = serde_json::from_str(payload).expect("move op should deserialize");
        assert_eq!(
            parsed.target(),
            &TransformTarget::File {
                expected_file_hash: ContentHash::parse("0123456789abcdef")
                    .expect("test hash should be valid"),
            }
        );
        match parsed.op() {
            OpKind::Move { to } => assert_eq!(to.as_os_str(), "renamed.py"),
            other => panic!("expected move op, got {other:?}"),
        }
        assert_eq!(
            parsed.preview(),
            &ChangePreview::move_operation(Some(MovePreview {
                from: PathBuf::from("fixture.py"),
                to: PathBuf::from("renamed.py"),
            }))
        );
    }

    #[test]
    fn transform_target_file_requires_expected_file_hash() {
        let payload = r#"{"type":"file"}"#;

        let error = serde_json::from_str::<TransformTarget>(payload)
            .expect_err("file target without a precondition must be rejected");
        assert!(
            error.to_string().contains("expected_file_hash"),
            "missing precondition diagnostic should name the field: {error}"
        );
    }

    #[test]
    fn change_op_rejects_move_target_with_node_fields_on_file_target() {
        let payload = r#"{
            "target": {
                "type": "file",
                "identity": "id-1",
                "expected_file_hash": "0123456789abcdef"
            },
            "op": {
                "type": "move",
                "to": "renamed.py"
            },
            "preview": {
                "old_text": "",
                "new_text": "",
                "matched_span": {
                    "start": 0,
                    "end": 0
                },
                "move": {
                    "from": "fixture.py",
                    "to": "renamed.py"
                }
            }
        }"#;

        let error = serde_json::from_str::<ChangeOp>(payload)
            .expect_err("file target must reject node-only fields");
        assert!(
            error.to_string().contains("identity"),
            "error should identify the invalid field: {error}"
        );
    }

    #[test]
    fn transform_target_deserializes_legacy_node_shape_without_type() {
        let payload = r#"{
            "identity": "id-1",
            "kind": "function_definition",
            "expected_old_hash": "0123456789abcdef"
        }"#;

        let parsed: TransformTarget =
            serde_json::from_str(payload).expect("legacy node target should deserialize");

        match parsed {
            TransformTarget::Node {
                identity,
                kind,
                expected_old_hash,
                span_hint,
            } => {
                assert_eq!(identity, "id-1");
                assert_eq!(kind, "function_definition");
                assert_eq!(expected_old_hash.as_str(), "0123456789abcdef");
                assert!(span_hint.is_none());
            }
            other => panic!("expected node target, got {other:?}"),
        }
    }

    #[test]
    fn transform_target_deserializes_file_end_shape() {
        let payload = r#"{
            "type": "file_end",
            "expected_file_hash": "0123456789abcdef"
        }"#;

        let parsed: TransformTarget =
            serde_json::from_str(payload).expect("file_end target should deserialize");

        match parsed {
            TransformTarget::FileEnd { expected_file_hash } => {
                assert_eq!(expected_file_hash.as_str(), "0123456789abcdef");
            }
            other => panic!("expected file_end target, got {other:?}"),
        }
    }

    #[test]
    fn change_op_deserializes_insert_kind() {
        let payload = r##"{
            "target": {
                "type": "file_start",
                "expected_file_hash": "1111111111111111"
            },
            "op": {
                "type": "insert",
                "new_text": "# header\n"
            },
            "preview": {
                "old_text": "",
                "new_text": "# header\n",
                "matched_span": {
                    "start": 0,
                    "end": 0
                }
            }
        }"##;

        let parsed: ChangeOp = serde_json::from_str(payload).expect("insert op should deserialize");
        match parsed.op() {
            OpKind::Insert { new_text } => assert_eq!(new_text, "# header\n"),
            other => panic!("expected insert op, got {other:?}"),
        }
        assert_eq!(
            parsed.preview(),
            &ChangePreview::Text(TextChangePreview {
                old_text: Some(String::new()),
                old_hash: None,
                old_len: None,
                new_text: "# header\n".to_string(),
                matched_span: Span { start: 0, end: 0 },
            })
        );
    }

    #[test]
    fn transform_target_rejects_file_start_with_identity_field_name_in_message() {
        let payload = r#"{
            "type": "file_start",
            "expected_file_hash": "2222222222222222",
            "identity": "id-1"
        }"#;

        let error = serde_json::from_str::<TransformTarget>(payload)
            .expect_err("file-level target should reject node-only fields");
        let message = error.to_string();
        assert!(message.contains("file-level targets do not accept node-only fields"));
        assert!(message.contains("identity"));
    }

    #[test]
    fn transform_target_rejects_file_end_with_multiple_node_fields_in_message() {
        let payload = r#"{
            "type": "file_end",
            "expected_file_hash": "3333333333333333",
            "kind": "function_definition",
            "span_hint": { "start": 1, "end": 2 }
        }"#;

        let error = serde_json::from_str::<TransformTarget>(payload)
            .expect_err("file-level target should reject multiple node-only fields");
        let message = error.to_string();
        assert!(message.contains("file-level targets do not accept node-only fields"));
        assert!(message.contains("kind"));
        assert!(message.contains("span_hint"));
    }

    fn wire_target(target_type: &str) -> Value {
        match target_type {
            "node" => json!({
                "type": "node",
                "identity": "0123456789abcdef",
                "kind": "function_definition",
                "span_hint": { "start": 0, "end": 8 },
                "expected_old_hash": "fedcba9876543210"
            }),
            "file_start" | "file_end" | "file" => json!({
                "type": target_type,
                "expected_file_hash": "0123456789abcdef"
            }),
            "line" => json!({
                "type": "line",
                "anchor": "1:0123456789ab"
            }),
            other => panic!("unsupported test target type: {other}"),
        }
    }

    fn wire_op(op_type: &str) -> Value {
        match op_type {
            "replace" => json!({ "type": "replace", "new_text": "replacement" }),
            "delete" => json!({ "type": "delete" }),
            "insert_before" => json!({ "type": "insert_before", "new_text": "before" }),
            "insert_after" => json!({ "type": "insert_after", "new_text": "after" }),
            "insert" => json!({ "type": "insert", "new_text": "inserted" }),
            "move_before" | "move_after" => json!({
                "type": op_type,
                "destination": wire_target("node")
            }),
            "move" => json!({ "type": "move", "to": "renamed.py" }),
            other => panic!("unsupported test operation type: {other}"),
        }
    }

    fn text_preview(op_type: &str) -> Value {
        let new_text = match op_type {
            "replace" => "replacement",
            "insert_before" => "before",
            "insert_after" => "after",
            "insert" => "inserted",
            "delete" | "move_before" | "move_after" => "",
            other => panic!("operation does not use text preview: {other}"),
        };
        json!({
            "old_text": "",
            "new_text": new_text,
            "matched_span": { "start": 0, "end": 0 }
        })
    }

    fn move_preview() -> Value {
        json!({
            "move": {
                "from": "fixture.py",
                "to": "renamed.py"
            }
        })
    }

    fn wire_change_op(target_type: &str, op_type: &str) -> Value {
        json!({
            "target": wire_target(target_type),
            "op": wire_op(op_type),
            "preview": if op_type == "move" {
                move_preview()
            } else {
                text_preview(op_type)
            }
        })
    }

    fn valid_target_op_pair(target_type: &str, op_type: &str) -> bool {
        match target_type {
            "node" => matches!(
                op_type,
                "replace"
                    | "delete"
                    | "insert_before"
                    | "insert_after"
                    | "move_before"
                    | "move_after"
            ),
            "file_start" | "file_end" => op_type == "insert",
            "file" => op_type == "move",
            "line" => matches!(op_type, "replace" | "insert_after"),
            _ => false,
        }
    }

    #[test]
    fn change_op_round_trips_every_valid_target_operation_variant() {
        const VALID_PAIRS: &[(&str, &str)] = &[
            ("node", "replace"),
            ("node", "delete"),
            ("node", "insert_before"),
            ("node", "insert_after"),
            ("node", "move_before"),
            ("node", "move_after"),
            ("file_start", "insert"),
            ("file_end", "insert"),
            ("file", "move"),
            ("line", "replace"),
            ("line", "insert_after"),
        ];

        for &(target_type, op_type) in VALID_PAIRS {
            let wire = wire_change_op(target_type, op_type);
            let parsed: ChangeOp = serde_json::from_value(wire)
                .unwrap_or_else(|error| panic!("{target_type}/{op_type} must parse: {error}"));
            let serialized = serde_json::to_value(&parsed)
                .unwrap_or_else(|error| panic!("{target_type}/{op_type} must serialize: {error}"));
            assert_eq!(serialized["target"]["type"], target_type);
            assert_eq!(serialized["op"]["type"], op_type);
            assert!(
                serialized.get("operation").is_none(),
                "canonical internals must not leak into the wire schema"
            );
            let reparsed: ChangeOp = serde_json::from_value(serialized).unwrap_or_else(|error| {
                panic!("{target_type}/{op_type} must reparse after serialization: {error}")
            });

            assert_eq!(parsed, reparsed, "{target_type}/{op_type}");
        }
    }

    #[test]
    fn change_op_rejects_every_invalid_target_operation_pair_at_ingress() {
        const TARGET_TYPES: &[&str] = &["node", "file_start", "file_end", "file", "line"];
        const OP_TYPES: &[&str] = &[
            "replace",
            "delete",
            "insert_before",
            "insert_after",
            "insert",
            "move_before",
            "move_after",
            "move",
        ];

        for &target_type in TARGET_TYPES {
            for &op_type in OP_TYPES {
                if valid_target_op_pair(target_type, op_type) {
                    continue;
                }

                let error =
                    serde_json::from_value::<ChangeOp>(wire_change_op(target_type, op_type))
                        .unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains("unsupported target/op combination"),
                    "{target_type}/{op_type} produced an unexpected diagnostic: {error}"
                );
            }
        }
    }

    #[test]
    fn change_op_rejects_preview_family_mismatches_at_ingress() {
        let text_operation_with_move_preview = json!({
            "target": wire_target("node"),
            "op": wire_op("replace"),
            "preview": move_preview()
        });
        let move_operation_with_text_preview = json!({
            "target": wire_target("file"),
            "op": wire_op("move"),
            "preview": {
                "old_text": "not a compatibility placeholder",
                "new_text": "",
                "matched_span": { "start": 0, "end": 0 }
            }
        });

        for wire in [
            text_operation_with_move_preview,
            move_operation_with_text_preview,
        ] {
            let error = serde_json::from_value::<ChangeOp>(wire).unwrap_err();
            assert!(
                error.to_string().contains("preview"),
                "preview mismatch diagnostic should name the preview: {error}"
            );
        }
    }

    #[test]
    fn change_op_normalizes_legacy_empty_text_move_preview() {
        let wire = json!({
            "target": wire_target("file"),
            "op": wire_op("move"),
            "preview": {
                "old_text": "",
                "new_text": "",
                "matched_span": { "start": 0, "end": 0 }
            }
        });

        let parsed: ChangeOp =
            serde_json::from_value(wire).expect("legacy empty move preview should remain accepted");
        let serialized = serde_json::to_value(parsed).expect("normalized move should serialize");

        assert_eq!(serialized["preview"], json!({}));
        serde_json::from_value::<ChangeOp>(serialized)
            .expect("canonical empty move preview should reparse");
    }

    #[test]
    fn change_op_rejects_legacy_empty_text_move_preview_inside_canonical_model() {
        let error = ChangeOp::from_parts(
            TransformTarget::File {
                expected_file_hash: "0123456789abcdef".to_string(),
            },
            OpKind::Move {
                to: PathBuf::from("renamed.py"),
            },
            ChangePreview::text(
                Some(String::new()),
                None,
                None,
                String::new(),
                Span { start: 0, end: 0 },
            ),
        )
        .unwrap_err();

        assert!(error.to_string().contains("expected move preview fields"));
    }

    #[test]
    fn change_op_rejects_incomplete_legacy_empty_move_preview() {
        let wire = json!({
            "target": wire_target("file"),
            "op": wire_op("move"),
            "preview": {
                "old_text": ""
            }
        });

        let error = serde_json::from_value::<ChangeOp>(wire).unwrap_err();
        assert!(error.to_string().contains("missing field `new_text`"));
    }

    #[test]
    fn invalid_target_operation_pair_is_diagnosed_before_preview_shape() {
        let wire = json!({
            "target": wire_target("node"),
            "op": wire_op("insert"),
            "preview": {}
        });

        let error = serde_json::from_value::<ChangeOp>(wire).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported target/op combination"),
            "canonical target/op validation should not depend on preview shape: {error}"
        );
    }

    #[test]
    fn change_op_rejects_malformed_precondition_hashes_at_ingress() {
        let malformed_hashes = [
            "",
            "0123456789abcde",
            "0123456789abcdef0",
            "0123456789abcdeg",
            "éééééééé",
        ];

        for malformed_hash in malformed_hashes {
            for target_type in ["node", "file_start", "file_end", "file"] {
                let mut wire = wire_change_op(
                    target_type,
                    if target_type == "node" {
                        "replace"
                    } else if target_type == "file" {
                        "move"
                    } else {
                        "insert"
                    },
                );
                let field = if target_type == "node" {
                    "expected_old_hash"
                } else {
                    "expected_file_hash"
                };
                wire["target"][field] = json!(malformed_hash);

                let error = serde_json::from_value::<ChangeOp>(wire).unwrap_err();
                assert!(
                    error.to_string().contains(field),
                    "{target_type} malformed hash diagnostic should name {field}: {error}"
                );
            }
        }
    }

    #[test]
    fn change_op_rejects_malformed_line_anchors_at_ingress() {
        let malformed_anchors = [
            "0:0123456789ab",
            "1:0123456789a",
            "1:0123456789abc",
            "1:0123456789ag",
            "1:éééééé",
            "1:0123456789ab:tail",
        ];

        for anchor in malformed_anchors {
            let mut wire = wire_change_op("line", "replace");
            wire["target"]["anchor"] = json!(anchor);

            let error = serde_json::from_value::<ChangeOp>(wire).unwrap_err();
            assert!(
                error.to_string().contains("anchor"),
                "malformed line anchor should fail at ingress: {anchor}: {error}"
            );
        }
    }

    #[test]
    fn change_op_normalizes_hash_and_line_anchor_case_at_ingress() {
        let mut node_wire = wire_change_op("node", "replace");
        node_wire["target"]["expected_old_hash"] = json!("FEDCBA9876543210");
        node_wire["preview"]["old_hash"] = json!("ABCDEF0123456789");
        let node: ChangeOp =
            serde_json::from_value(node_wire).expect("uppercase hashes should parse");
        let serialized_node = serde_json::to_value(node).expect("node operation should serialize");
        assert_eq!(
            serialized_node["target"]["expected_old_hash"],
            "fedcba9876543210"
        );
        assert_eq!(serialized_node["preview"]["old_hash"], "abcdef0123456789");

        let mut line_wire = wire_change_op("line", "replace");
        line_wire["target"]["anchor"] = json!(" 7:ABCDEF012345|display text ");
        let line: ChangeOp =
            serde_json::from_value(line_wire).expect("display-form line anchor should parse");
        let serialized_line = serde_json::to_value(line).expect("line operation should serialize");
        assert_eq!(serialized_line["target"]["anchor"], "7:abcdef012345");
    }

    #[test]
    fn change_op_rejects_malformed_compact_preview_hash_at_ingress() {
        let mut wire = wire_change_op("node", "replace");
        wire["preview"]["old_hash"] = json!("deadbeef");

        let error = serde_json::from_value::<ChangeOp>(wire).unwrap_err();
        assert!(
            error.to_string().contains("old_hash"),
            "compact preview hash diagnostic should name old_hash: {error}"
        );
    }

    #[test]
    fn same_file_move_rejects_non_position_destinations_at_ingress() {
        let invalid_destinations = [
            wire_target("file"),
            json!({
                "type": "line",
                "anchor": "1:0123456789ab",
                "end_anchor": "2:abcdef012345"
            }),
        ];

        for op_type in ["move_before", "move_after"] {
            for destination in invalid_destinations.clone() {
                let mut operation = wire_op(op_type);
                operation["destination"] = destination;
                let wire = json!({
                    "target": wire_target("node"),
                    "op": operation,
                    "preview": text_preview(op_type)
                });

                let error = serde_json::from_value::<ChangeOp>(wire).unwrap_err();
                assert!(
                    error.to_string().contains("destination"),
                    "invalid {op_type} destination should be diagnosed: {error}"
                );
            }
        }
    }

    #[test]
    fn same_file_move_accepts_each_position_destination_variant() {
        let valid_destinations = [
            wire_target("node"),
            wire_target("file_start"),
            wire_target("file_end"),
            wire_target("line"),
        ];

        for op_type in ["move_before", "move_after"] {
            for destination in valid_destinations.clone() {
                let mut operation = wire_op(op_type);
                operation["destination"] = destination;
                let wire = json!({
                    "target": wire_target("node"),
                    "op": operation,
                    "preview": text_preview(op_type)
                });

                serde_json::from_value::<ChangeOp>(wire)
                    .unwrap_or_else(|error| panic!("valid {op_type} destination failed: {error}"));
            }
        }
    }

    #[test]
    fn line_range_target_rejects_insert_after_at_ingress() {
        let wire = json!({
            "target": {
                "type": "line",
                "anchor": "1:0123456789ab",
                "end_anchor": "2:abcdef012345"
            },
            "op": wire_op("insert_after"),
            "preview": text_preview("insert_after")
        });

        let error = serde_json::from_value::<ChangeOp>(wire).unwrap_err();
        assert!(
            error.to_string().contains("end_anchor"),
            "line range diagnostic should identify end_anchor: {error}"
        );
    }

    #[test]
    fn canonical_mutators_preserve_the_existing_operation_on_validation_failure() {
        let original_target = TransformTarget::node(
            "0123456789abcdef".to_string(),
            "function_definition".to_string(),
            Some(Span { start: 0, end: 8 }),
            ContentHash::parse("fedcba9876543210").expect("test hash should be valid"),
        );
        let original_preview = ChangePreview::text(
            Some("old".to_string()),
            None,
            None,
            "new".to_string(),
            Span { start: 0, end: 8 },
        );
        let mut operation = ChangeOp::from_parts(
            original_target.clone(),
            OpKind::Replace {
                new_text: "new".to_string(),
            },
            original_preview.clone(),
        )
        .expect("fixture operation should be canonical");

        operation
            .replace_target(TransformTarget::FileEnd {
                expected_file_hash: ContentHash::parse("0123456789abcdef")
                    .expect("test hash should be valid"),
            })
            .expect_err("replace cannot target file_end");
        operation
            .replace_preview(ChangePreview::move_operation(None))
            .expect_err("replace cannot use a move preview");

        assert_eq!(operation.target(), &original_target);
        assert_eq!(operation.preview(), &original_preview);
    }
}
