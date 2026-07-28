use std::path::PathBuf;

use crate::changeset::{EditOperation, OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::hashline::HashlineEdit;
use crate::patch::scoped_regex::rewrite_node_target_with_scoped_regex;

use super::target::NodeTargetSelector;

#[derive(Debug, Clone)]
pub(crate) struct NodeEditIntent {
    pub(super) file: PathBuf,
    pub(super) selector: NodeTargetSelector,
    pub(super) operation: PreparedNodeEditOperation,
}

#[derive(Debug, Clone)]
pub(crate) struct CanonicalEditIntent {
    pub(crate) file: PathBuf,
    pub(crate) operation: EditOperation,
}

#[derive(Debug, Clone)]
pub(crate) struct LineEditIntent {
    pub(crate) file: PathBuf,
    pub(crate) edit: HashlineEdit,
}

#[derive(Debug, Clone)]
pub(crate) enum PreparedEditIntent {
    Node(NodeEditIntent),
    Canonical(CanonicalEditIntent),
    Line(LineEditIntent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PreparedNodeEditOperation {
    Standard(OpKind),
    ScopedRegex {
        pattern: String,
        replacement: String,
    },
}

#[derive(Debug)]
pub(crate) struct ResolvedNodeEditIntent {
    pub(crate) file: PathBuf,
    pub(crate) operation: EditOperation,
    pub(crate) regex_replacements: Option<usize>,
}

impl NodeEditIntent {
    pub(crate) fn resolve(self) -> Result<ResolvedNodeEditIntent, IdenteditError> {
        let handle = self.selector.resolve(&self.file)?;
        let target = TransformTarget::node(
            handle.identity,
            handle.kind,
            Some(handle.span),
            handle.expected_old_hash,
        );

        let (op, regex_replacements) = match self.operation {
            PreparedNodeEditOperation::Standard(op) => (op, None),
            PreparedNodeEditOperation::ScopedRegex {
                pattern,
                replacement,
            } => {
                let rewritten = rewrite_node_target_with_scoped_regex(
                    &self.file,
                    &target,
                    &pattern,
                    &replacement,
                )?;
                (
                    OpKind::Replace {
                        new_text: rewritten.new_text,
                    },
                    Some(rewritten.replacements),
                )
            }
        };

        Ok(ResolvedNodeEditIntent {
            file: self.file,
            operation: EditOperation::try_new(target, op)?,
            regex_replacements,
        })
    }
}

impl LineEditIntent {
    pub(crate) fn into_canonical(self) -> Result<CanonicalEditIntent, IdenteditError> {
        let (target, op) = match self.edit {
            HashlineEdit::SetLine { set_line } => (
                TransformTarget::Line {
                    anchor: set_line.anchor,
                    end_anchor: None,
                },
                OpKind::Replace {
                    new_text: set_line.new_text,
                },
            ),
            HashlineEdit::ReplaceLines { replace_lines } => (
                TransformTarget::Line {
                    anchor: replace_lines.start_anchor,
                    end_anchor: replace_lines.end_anchor,
                },
                OpKind::Replace {
                    new_text: replace_lines.new_text,
                },
            ),
            HashlineEdit::InsertAfter { insert_after } => (
                TransformTarget::Line {
                    anchor: insert_after.anchor,
                    end_anchor: None,
                },
                OpKind::InsertAfter {
                    new_text: insert_after.text,
                },
            ),
        };

        Ok(CanonicalEditIntent {
            file: self.file,
            operation: EditOperation::try_new(target, op)?,
        })
    }
}
