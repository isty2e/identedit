use std::path::Path;

use tree_sitter::Tree;

use crate::handle::SelectionHandle;

/// A provider observation, not necessarily a syntax-validation proof.
pub struct ParsedSource<'source> {
    representation: Representation<'source>,
}

enum Representation<'source> {
    TreeSitter { tree: Tree, source: &'source [u8] },
    Json { tree: Tree, source: &'source [u8] },
    Handles(Vec<SelectionHandle>),
}

impl<'source> ParsedSource<'source> {
    pub(super) fn tree_sitter(tree: Tree, source: &'source [u8]) -> Self {
        Self {
            representation: Representation::TreeSitter { tree, source },
        }
    }

    pub(super) fn json(tree: Tree, source: &'source [u8]) -> Self {
        Self {
            representation: Representation::Json { tree, source },
        }
    }

    pub(super) fn from_handles(handles: Vec<SelectionHandle>) -> Self {
        Self {
            representation: Representation::Handles(handles),
        }
    }

    pub fn into_handles(self, path: &Path) -> Vec<SelectionHandle> {
        match self.representation {
            Representation::TreeSitter { tree, source } => {
                super::tree_sitter::collect_handles(tree, path, source)
            }
            Representation::Json { tree, source } => {
                super::json::collect_handles(tree, path, source)
            }
            Representation::Handles(handles) => handles,
        }
    }
}

#[cfg(test)]
mod tests;
