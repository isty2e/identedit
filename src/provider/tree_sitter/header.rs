use std::path::Path;

use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::provider::normalize_bare_cr_for_parser;

use super::catalog::{
    C_CPP_HEADER_PROVIDER_NAME, C_CPP_HEADER_SYNTAX_ERROR_MESSAGE, c_language_spec,
    cpp_language_spec,
};
use super::parser::{collect_nodes, parse_tree_from_source};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeaderDialect {
    C,
    Cpp,
}

pub(super) fn parse_c_cpp_header_with_dialect(
    path: &Path,
    source: &[u8],
) -> Result<(Vec<SelectionHandle>, HeaderDialect), IdenteditError> {
    let parse_source = normalize_bare_cr_for_parser(source);
    debug_assert_eq!(parse_source.len(), source.len());

    let cpp_spec = cpp_language_spec();
    let c_spec = c_language_spec();

    let cpp_tree = parse_tree_from_source(parse_source.as_ref(), &cpp_spec.source, cpp_spec.name)?;
    let c_tree = parse_tree_from_source(parse_source.as_ref(), &c_spec.source, c_spec.name)?;

    let cpp_has_error = cpp_tree.root_node().has_error();
    let c_has_error = c_tree.root_node().has_error();

    let (selected_tree, dialect) = match (cpp_has_error, c_has_error) {
        (false, true) => (&cpp_tree, HeaderDialect::Cpp),
        (true, false) => (&c_tree, HeaderDialect::C),
        (false, false) => {
            // TODO: Replace this with content-based heuristics for ambiguous headers.
            (&cpp_tree, HeaderDialect::Cpp)
        }
        (true, true) => {
            return Err(IdenteditError::ParseFailure {
                provider: C_CPP_HEADER_PROVIDER_NAME,
                message: C_CPP_HEADER_SYNTAX_ERROR_MESSAGE.to_string(),
            });
        }
    };

    let mut handles = Vec::new();
    collect_nodes(selected_tree.root_node(), path, source, &mut handles);
    Ok((handles, dialect))
}
