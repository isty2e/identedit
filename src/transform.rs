use crate::changeset::OpKind;
use crate::handle::Span;

pub(crate) mod build;
pub(crate) mod conflict;
pub(crate) mod parse;
pub(crate) mod resolve;

#[derive(Debug, Clone)]
pub struct MatchedChange {
    pub index: usize,
    pub op: OpKind,
    pub old_text: String,
    pub matched_span: Span,
    pub move_insert_at: Option<usize>,
    pub anchor_kind: String,
    pub anchor_span: Span,
}

#[cfg(test)]
mod tests;
