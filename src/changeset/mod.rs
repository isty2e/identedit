use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::handle::Span;

mod wire;

pub use crate::hash::{HASH_HEX_LEN, hash_bytes, hash_text};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformTarget {
    Node {
        identity: String,
        kind: String,
        #[serde(skip_serializing_if = "Option::is_none")]
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
        #[serde(skip_serializing_if = "Option::is_none")]
        end_anchor: Option<String>,
    },
}

impl TransformTarget {
    pub fn node(
        identity: String,
        kind: String,
        span_hint: Option<Span>,
        expected_old_hash: String,
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

    pub fn precondition_hash(&self) -> &str {
        match self {
            Self::Node {
                expected_old_hash, ..
            } => expected_old_hash,
            Self::FileStart { expected_file_hash } | Self::FileEnd { expected_file_hash } => {
                expected_file_hash
            }
            Self::Line { anchor, .. } => anchor
                .split_once(':')
                .map(|(_, hash)| hash)
                .unwrap_or_default(),
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
#[serde(deny_unknown_fields)]
pub struct ChangeOp {
    pub target: TransformTarget,
    pub op: OpKind,
    pub preview: ChangePreview,
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
    pub old_hash: Option<String>,
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

impl ChangePreview {
    pub fn text(
        old_text: Option<String>,
        old_hash: Option<String>,
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

    pub fn move_operation(move_preview: Option<MovePreview>) -> Self {
        Self::Move(MoveChangePreview { move_preview })
    }

    pub fn as_text(&self) -> Option<&TextChangePreview> {
        match self {
            Self::Text(preview) => Some(preview),
            Self::Move(_) => None,
        }
    }

    pub fn as_text_mut(&mut self) -> Option<&mut TextChangePreview> {
        match self {
            Self::Text(preview) => Some(preview),
            Self::Move(_) => None,
        }
    }

    pub fn as_move(&self) -> Option<&MoveChangePreview> {
        match self {
            Self::Text(_) => None,
            Self::Move(preview) => Some(preview),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        ChangeOp, ChangePreview, MovePreview, MultiFileChangeset, OpKind, TextChangePreview,
        TransactionMode, TransformTarget, hash_text,
    };
    use crate::handle::Span;

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
        assert_eq!(hash.len(), super::HASH_HEX_LEN);
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
                "identity": "id-1",
                "kind": "function_definition",
                "expected_old_hash": "hash-1"
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
        match parsed.op {
            OpKind::Move { to } => assert_eq!(to.as_os_str(), "renamed.py"),
            other => panic!("expected move op, got {other:?}"),
        }
        assert_eq!(
            parsed.preview,
            ChangePreview::move_operation(Some(MovePreview {
                from: PathBuf::from("fixture.py"),
                to: PathBuf::from("renamed.py"),
            }))
        );
    }

    #[test]
    fn change_op_deserializes_legacy_move_preview_placeholder_shape() {
        let payload = r#"{
            "target": {
                "identity": "id-1",
                "kind": "function_definition",
                "expected_old_hash": "hash-1"
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

        let parsed: ChangeOp =
            serde_json::from_str(payload).expect("legacy move preview should deserialize");
        assert_eq!(
            parsed.preview,
            ChangePreview::move_operation(Some(MovePreview {
                from: PathBuf::from("fixture.py"),
                to: PathBuf::from("renamed.py"),
            }))
        );
    }

    #[test]
    fn transform_target_deserializes_legacy_node_shape_without_type() {
        let payload = r#"{
            "identity": "id-1",
            "kind": "function_definition",
            "expected_old_hash": "hash-1"
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
                assert_eq!(expected_old_hash, "hash-1");
                assert!(span_hint.is_none());
            }
            other => panic!("expected node target, got {other:?}"),
        }
    }

    #[test]
    fn transform_target_deserializes_file_end_shape() {
        let payload = r#"{
            "type": "file_end",
            "expected_file_hash": "file-hash-1"
        }"#;

        let parsed: TransformTarget =
            serde_json::from_str(payload).expect("file_end target should deserialize");

        match parsed {
            TransformTarget::FileEnd { expected_file_hash } => {
                assert_eq!(expected_file_hash, "file-hash-1");
            }
            other => panic!("expected file_end target, got {other:?}"),
        }
    }

    #[test]
    fn change_op_deserializes_insert_kind() {
        let payload = r##"{
            "target": {
                "type": "file_start",
                "expected_file_hash": "file-hash-2"
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
        match parsed.op {
            OpKind::Insert { new_text } => assert_eq!(new_text, "# header\n"),
            other => panic!("expected insert op, got {other:?}"),
        }
        assert_eq!(
            parsed.preview,
            ChangePreview::Text(TextChangePreview {
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
            "expected_file_hash": "file-hash-3",
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
            "expected_file_hash": "file-hash-4",
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
}
