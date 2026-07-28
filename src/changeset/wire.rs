use std::{fmt, result};

use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::handle::Span;

use super::{
    ChangeOp, ChangePreview, EditOperation, MoveChangePreview, MovePreview, OpKind,
    TextChangePreview, TransactionMode, TransactionSpec, TransformTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransformTargetType {
    Node,
    FileStart,
    FileEnd,
    File,
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
            TransformTargetType::File => {
                reject_node_or_line_fields_for_file_target(&wire)?;
                let expected_file_hash = wire
                    .expected_file_hash
                    .ok_or_else(|| de::Error::missing_field("expected_file_hash"))?;
                Ok(TransformTarget::File { expected_file_hash })
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangePreviewWire {
    #[serde(default)]
    old_text: Option<String>,
    #[serde(default)]
    old_hash: Option<String>,
    #[serde(default)]
    old_len: Option<usize>,
    #[serde(default)]
    new_text: Option<String>,
    #[serde(default)]
    matched_span: Option<Span>,
    #[serde(default, rename = "move")]
    move_preview: Option<MovePreview>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChangeOpWire {
    target: TransformTarget,
    op: OpKind,
    preview: ChangePreviewWire,
}

impl<'de> Deserialize<'de> for ChangeOp {
    fn deserialize<D>(deserializer: D) -> result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ChangeOpWire::deserialize(deserializer)?;
        let operation = EditOperation::try_new(wire.target, wire.op).map_err(de::Error::custom)?;
        let preview = wire
            .preview
            .into_preview_for_op::<D::Error>(operation.op())?;
        ChangeOp::try_new(operation, preview).map_err(de::Error::custom)
    }
}

impl ChangePreviewWire {
    fn into_preview_for_op<E>(self, op: &OpKind) -> result::Result<ChangePreview, E>
    where
        E: de::Error,
    {
        if matches!(op, OpKind::Move { .. }) {
            if let Some(move_preview) = self.move_preview.clone() {
                reject_legacy_move_placeholder_fields(&self)?;
                return Ok(ChangePreview::Move(MoveChangePreview {
                    move_preview: Some(move_preview),
                }));
            }

            if self.old_text.is_none()
                && self.old_hash.is_none()
                && self.old_len.is_none()
                && self.new_text.is_none()
                && self.matched_span.is_none()
            {
                return Ok(ChangePreview::Move(MoveChangePreview {
                    move_preview: None,
                }));
            }
        } else if let Some(move_preview) = self.move_preview {
            return Ok(ChangePreview::Move(MoveChangePreview {
                move_preview: Some(move_preview),
            }));
        }

        Ok(ChangePreview::Text(TextChangePreview {
            old_text: self.old_text,
            old_hash: self.old_hash,
            old_len: self.old_len,
            new_text: self
                .new_text
                .ok_or_else(|| de::Error::missing_field("new_text"))?,
            matched_span: self
                .matched_span
                .ok_or_else(|| de::Error::missing_field("matched_span"))?,
        }))
    }
}

fn reject_legacy_move_placeholder_fields<E>(wire: &ChangePreviewWire) -> result::Result<(), E>
where
    E: de::Error,
{
    if wire.old_hash.is_some() || wire.old_len.is_some() {
        return Err(E::custom(
            "move preview does not accept old_hash/old_len placeholder fields",
        ));
    }

    if wire
        .old_text
        .as_deref()
        .is_some_and(|text| !text.is_empty())
    {
        return Err(E::custom(
            "move preview compatibility old_text field must be empty when provided",
        ));
    }

    if wire
        .new_text
        .as_deref()
        .is_some_and(|text| !text.is_empty())
    {
        return Err(E::custom(
            "move preview compatibility new_text field must be empty when provided",
        ));
    }

    if wire
        .matched_span
        .is_some_and(|span| span.start != 0 || span.end != 0)
    {
        return Err(E::custom(
            "move preview compatibility matched_span field must be [0, 0) when provided",
        ));
    }

    Ok(())
}
