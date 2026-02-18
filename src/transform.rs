use std::path::Path;

use crate::changeset::{FileChange, OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::provider::ProviderRegistry;

mod build;
mod conflict;
mod parse;
mod resolve;

#[derive(Debug, Clone)]
pub struct TransformInstruction {
    pub target: TransformTarget,
    pub op: OpKind,
}

#[derive(Debug, Clone)]
pub struct MatchedChange {
    pub index: usize,
    pub target: TransformTarget,
    pub op: OpKind,
    pub expected_hash: String,
    pub old_text: String,
    pub matched_span: Span,
    pub move_insert_at: Option<usize>,
    pub anchor_kind: String,
    pub anchor_span: Span,
}

pub fn build_replace_changeset(
    file: &Path,
    identity: &str,
    replacement: String,
) -> Result<FileChange, IdenteditError> {
    build::build_replace_changeset(file, identity, replacement)
}

pub fn build_delete_changeset(file: &Path, identity: &str) -> Result<FileChange, IdenteditError> {
    build::build_delete_changeset(file, identity)
}

pub fn build_insert_before_changeset(
    file: &Path,
    identity: &str,
    new_text: String,
) -> Result<FileChange, IdenteditError> {
    build::build_insert_before_changeset(file, identity, new_text)
}

pub fn build_insert_after_changeset(
    file: &Path,
    identity: &str,
    new_text: String,
) -> Result<FileChange, IdenteditError> {
    build::build_insert_after_changeset(file, identity, new_text)
}

pub fn build_changeset(
    file: &Path,
    instructions: Vec<TransformInstruction>,
) -> Result<FileChange, IdenteditError> {
    build::build_changeset(file, instructions)
}

pub fn resolve_changeset_targets(
    changeset: &FileChange,
) -> Result<Vec<MatchedChange>, IdenteditError> {
    build::resolve_changeset_targets(changeset)
}

pub fn resolve_changeset_targets_in_handles(
    changeset: &FileChange,
    source_text: &str,
    handles: &[SelectionHandle],
) -> Result<Vec<MatchedChange>, IdenteditError> {
    build::resolve_changeset_targets_in_handles(changeset, source_text, handles)
}

pub fn parse_handles_for_file(file: &Path) -> Result<Vec<SelectionHandle>, IdenteditError> {
    parse::parse_handles_for_file(file)
}

pub fn parse_handles_for_source(
    file: &Path,
    source: &[u8],
) -> Result<Vec<SelectionHandle>, IdenteditError> {
    parse::parse_handles_for_source(file, source)
}

pub fn parse_handles_for_source_with_registry(
    file: &Path,
    source: &[u8],
    registry: &ProviderRegistry,
) -> Result<Vec<SelectionHandle>, IdenteditError> {
    parse::parse_handles_for_source_with_registry(file, source, registry)
}

#[cfg(test)]
fn reject_move_operation(op: &OpKind, index: usize) -> Result<(), IdenteditError> {
    conflict::reject_move_operation(op, index)
}

pub fn validate_change_conflicts(matched_changes: &[MatchedChange]) -> Result<(), IdenteditError> {
    conflict::validate_change_conflicts(matched_changes)
}

pub fn resolve_target_in_handles(
    file: &Path,
    handles: &[SelectionHandle],
    target: &TransformTarget,
) -> Result<SelectionHandle, IdenteditError> {
    resolve::resolve_target_in_handles(file, handles, target)
}

#[cfg(test)]
mod tests;
