use std::path::PathBuf;
use std::{fmt, result};

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::handle::Span;
pub use crate::hash::HASH_HEX_LEN;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransformTargetType {
    Node,
    FileStart,
    FileEnd,
    Line,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransformTargetWire {
    #[serde(default, rename = "type")]
    target_type: Option<TransformTargetType>,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    span_hint: Option<Span>,
    #[serde(default)]
    expected_old_hash: Option<String>,
    #[serde(default)]
    expected_file_hash: Option<String>,
    #[serde(default)]
    anchor: Option<String>,
    #[serde(default)]
    end_anchor: Option<String>,
}

impl<'de> Deserialize<'de> for TransformTarget {
    fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TransformTargetWire::deserialize(deserializer)?;
        match wire.target_type.unwrap_or(TransformTargetType::Node) {
            TransformTargetType::Node => {
                if wire.expected_file_hash.is_some()
                    || wire.anchor.is_some()
                    || wire.end_anchor.is_some()
                {
                    return Err(de::Error::custom(
                        "node target does not accept expected_file_hash/anchor/end_anchor",
                    ));
                }
                let identity = wire
                    .identity
                    .ok_or_else(|| de::Error::missing_field("identity"))?;
                let kind = wire.kind.ok_or_else(|| de::Error::missing_field("kind"))?;
                let expected_old_hash = wire
                    .expected_old_hash
                    .ok_or_else(|| de::Error::missing_field("expected_old_hash"))?;
                Ok(TransformTarget::Node {
                    identity,
                    kind,
                    span_hint: wire.span_hint,
                    expected_old_hash,
                })
            }
            TransformTargetType::FileStart => {
                reject_node_or_line_fields_for_file_target(&wire)?;
                let expected_file_hash = wire
                    .expected_file_hash
                    .ok_or_else(|| de::Error::missing_field("expected_file_hash"))?;
                Ok(TransformTarget::FileStart { expected_file_hash })
            }
            TransformTargetType::FileEnd => {
                reject_node_or_line_fields_for_file_target(&wire)?;
                let expected_file_hash = wire
                    .expected_file_hash
                    .ok_or_else(|| de::Error::missing_field("expected_file_hash"))?;
                Ok(TransformTarget::FileEnd { expected_file_hash })
            }
            TransformTargetType::Line => {
                reject_node_or_file_fields_for_line_target(&wire)?;
                let anchor = wire
                    .anchor
                    .ok_or_else(|| de::Error::missing_field("anchor"))?;
                Ok(TransformTarget::Line {
                    anchor,
                    end_anchor: wire.end_anchor,
                })
            }
        }
    }
}

fn reject_node_or_line_fields_for_file_target<E>(
    wire: &TransformTargetWire,
) -> result::Result<(), E>
where
    E: de::Error,
{
    let mut invalid_fields = Vec::new();
    if wire.identity.is_some() {
        invalid_fields.push("identity");
    }
    if wire.kind.is_some() {
        invalid_fields.push("kind");
    }
    if wire.span_hint.is_some() {
        invalid_fields.push("span_hint");
    }
    if wire.expected_old_hash.is_some() {
        invalid_fields.push("expected_old_hash");
    }
    if wire.anchor.is_some() {
        invalid_fields.push("anchor");
    }
    if wire.end_anchor.is_some() {
        invalid_fields.push("end_anchor");
    }

    if !invalid_fields.is_empty() {
        return Err(E::custom(format!(
            "file-level targets do not accept node-only fields: {}",
            invalid_fields.join(", ")
        )));
    }
    Ok(())
}

fn reject_node_or_file_fields_for_line_target<E>(
    wire: &TransformTargetWire,
) -> result::Result<(), E>
where
    E: de::Error,
{
    let mut invalid_fields = Vec::new();
    if wire.identity.is_some() {
        invalid_fields.push("identity");
    }
    if wire.kind.is_some() {
        invalid_fields.push("kind");
    }
    if wire.span_hint.is_some() {
        invalid_fields.push("span_hint");
    }
    if wire.expected_old_hash.is_some() {
        invalid_fields.push("expected_old_hash");
    }
    if wire.expected_file_hash.is_some() {
        invalid_fields.push("expected_file_hash");
    }

    if !invalid_fields.is_empty() {
        return Err(E::custom(format!(
            "line targets do not accept non-line fields: {}",
            invalid_fields.join(", ")
        )));
    }
    Ok(())
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

impl<'de> Deserialize<'de> for TransactionSpec {
    fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TransactionSpecVisitor;

        impl<'de> Visitor<'de> for TransactionSpecVisitor {
            type Value = TransactionSpec;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object for transaction settings")
            }

            fn visit_map<M>(self, map: M) -> result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct TransactionSpecWire {
                    #[serde(default)]
                    mode: TransactionMode,
                }

                let wire =
                    TransactionSpecWire::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(TransactionSpec { mode: wire.mode })
            }
        }

        deserializer.deserialize_map(TransactionSpecVisitor)
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePreview {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_len: Option<usize>,
    pub new_text: String,
    pub matched_span: Span,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "move")]
    pub move_preview: Option<MovePreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovePreview {
    pub from: PathBuf,
    pub to: PathBuf,
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    crate::hash::hash_bytes(bytes)
}

pub fn hash_text(text: &str) -> String {
    crate::hash::hash_text(text)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        ChangeOp, MovePreview, MultiFileChangeset, OpKind, TransactionMode, TransformTarget,
        hash_text,
    };

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
            parsed.preview.move_preview,
            Some(MovePreview {
                from: PathBuf::from("fixture.py"),
                to: PathBuf::from("renamed.py"),
            })
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
