use std::path::Path;

use tree_sitter::Tree;

use crate::handle::SelectionHandle;
use crate::named_structure::NamedNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameProjectionError {
    Unsupported,
    InvalidSpan,
}

/// A provider observation, not necessarily a syntax-validation proof.
pub struct ParsedSource<'source> {
    representation: Representation<'source>,
}

enum Representation<'source> {
    TreeSitter {
        path: &'source Path,
        tree: Tree,
        source: &'source [u8],
    },
    Json {
        path: &'source Path,
        tree: Tree,
        source: &'source [u8],
    },
    Handles(Vec<SelectionHandle>),
}

impl<'source> ParsedSource<'source> {
    pub(super) fn tree_sitter(path: &'source Path, tree: Tree, source: &'source [u8]) -> Self {
        Self {
            representation: Representation::TreeSitter { path, tree, source },
        }
    }

    pub(super) fn json(path: &'source Path, tree: Tree, source: &'source [u8]) -> Self {
        Self {
            representation: Representation::Json { path, tree, source },
        }
    }

    pub(super) fn from_handles(handles: Vec<SelectionHandle>) -> Self {
        Self {
            representation: Representation::Handles(handles),
        }
    }

    pub fn into_handles(self) -> Vec<SelectionHandle> {
        match self.representation {
            Representation::TreeSitter { path, tree, source } => {
                super::tree_sitter::collect_handles(tree, path, source)
            }
            Representation::Json { path, tree, source } => {
                super::json::collect_handles(tree, path, source)
            }
            Representation::Handles(handles) => handles,
        }
    }

    pub(crate) fn named_nodes(&self) -> Result<Vec<NamedNode>, NameProjectionError> {
        match &self.representation {
            Representation::TreeSitter { tree, source, .. } => {
                super::tree_sitter::collect_named_nodes(tree, source)
            }
            Representation::Json { tree, source, .. } => {
                super::json::collect_named_nodes(tree, source)
            }
            Representation::Handles(_) => Err(NameProjectionError::Unsupported),
        }
    }
}

#[cfg(test)]
mod tests;
