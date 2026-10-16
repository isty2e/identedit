use std::borrow::Cow;
use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::named_structure::NamedNode;
use crate::provider::{NameProjectionError, node_text, normalize_bare_cr_for_parser};

use super::catalog::{LanguageSource, LanguageSpec};

pub(super) fn parse_with_spec(
    spec: &'static LanguageSpec,
    source: &[u8],
) -> Result<tree_sitter::Tree, IdenteditError> {
    let parse_source = if spec.normalize_bare_cr {
        normalize_bare_cr_for_parser(source)
    } else {
        Cow::Borrowed(source)
    };
    debug_assert_eq!(parse_source.len(), source.len());
    let tree = parse_tree_from_source(parse_source.as_ref(), &spec.source, spec.name)?;

    if tree.root_node().has_error() {
        return Err(IdenteditError::ParseFailure {
            provider: spec.name,
            message: spec.syntax_error_message.to_string(),
        });
    }

    Ok(tree)
}

pub(super) fn parse_tree_from_source(
    source: &[u8],
    language_source: &LanguageSource,
    provider_name: &'static str,
) -> Result<tree_sitter::Tree, IdenteditError> {
    let mut parser = Parser::new();
    let language = language_source.load()?;

    parser
        .set_language(&language)
        .map_err(|error| IdenteditError::LanguageSetup {
            message: error.to_string(),
        })?;

    parser
        .parse(source, None)
        .ok_or_else(|| IdenteditError::ParseFailure {
            provider: provider_name,
            message: "Tree-sitter returned no syntax tree".to_string(),
        })
}

pub(super) fn collect_nodes(
    node: Node<'_>,
    path: &Path,
    source: &[u8],
    handles: &mut Vec<SelectionHandle>,
) {
    visit_nodes(node, source, &mut |node, name| {
        if let Some(text) = node_text(node, source) {
            handles.push(SelectionHandle::from_parts(
                path.to_path_buf(),
                Span {
                    start: node.start_byte(),
                    end: node.end_byte(),
                },
                node.kind().to_string(),
                name,
                text,
            ));
        }
    });
}

pub(super) fn collect_named_nodes(
    node: Node<'_>,
    source: &[u8],
) -> Result<Vec<NamedNode>, NameProjectionError> {
    let mut names = Vec::new();
    let valid = visit_nodes(node, source, &mut |node, name| {
        if let Some(name) = name {
            names.push(NamedNode {
                span: Span {
                    start: node.start_byte(),
                    end: node.end_byte(),
                },
                kind: node.kind().to_string(),
                name,
            });
        }
    });
    if valid {
        Ok(names)
    } else {
        Err(NameProjectionError::InvalidSpan)
    }
}

fn visit_nodes<'tree>(
    node: Node<'tree>,
    source: &[u8],
    visit: &mut impl FnMut(Node<'tree>, Option<String>),
) -> bool {
    let mut valid = true;
    if node.is_named() {
        if node.start_byte() <= node.end_byte() && node.end_byte() <= source.len() {
            visit(node, extract_node_name(node, source));
        } else {
            valid = false;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        valid &= visit_nodes(child, source, visit);
    }
    valid
}

fn extract_node_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|name_node| node_text(name_node, source))
        .filter(|name| !name.is_empty())
}
