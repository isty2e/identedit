use std::borrow::Cow;
use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::provider::{node_text, normalize_bare_cr_for_parser};

use super::catalog::{LanguageSource, LanguageSpec};

pub(super) fn parse_with_spec(
    spec: &'static LanguageSpec,
    path: &Path,
    source: &[u8],
) -> Result<Vec<SelectionHandle>, IdenteditError> {
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

    let mut handles = Vec::new();
    collect_nodes(tree.root_node(), path, source, &mut handles);
    Ok(handles)
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
    if node.is_named()
        && let Some(text) = node_text(node, source)
    {
        let kind = node.kind().to_string();
        let name = extract_node_name(node, source);
        let span = Span {
            start: node.start_byte(),
            end: node.end_byte(),
        };

        handles.push(SelectionHandle::from_parts(
            path.to_path_buf(),
            span,
            kind,
            name,
            text,
        ));
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_nodes(child, path, source, handles);
    }
}

fn extract_node_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|name_node| node_text(name_node, source))
        .filter(|name| !name.is_empty())
}
