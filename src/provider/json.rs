use std::borrow::Cow;
use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::named_structure::NamedNode;
use crate::provider::{NameProjectionError, node_source};
use crate::provider::{ParsedSource, StructureProvider};

pub struct JsonProvider;

impl StructureProvider for JsonProvider {
    fn parse_source<'source>(
        &self,
        path: &'source Path,
        source: &'source [u8],
    ) -> Result<ParsedSource<'source>, IdenteditError> {
        let mut parser = Parser::new();
        let language = tree_sitter_json::LANGUAGE;

        parser
            .set_language(&language.into())
            .map_err(|error| IdenteditError::LanguageSetup {
                message: error.to_string(),
            })?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| IdenteditError::ParseFailure {
                provider: self.name(),
                message: "Tree-sitter returned no syntax tree".to_string(),
            })?;

        if tree.root_node().has_error() {
            return Err(IdenteditError::ParseFailure {
                provider: self.name(),
                message: "Syntax errors detected in JSON input".to_string(),
            });
        }

        Ok(ParsedSource::json(path, tree, source))
    }

    fn supports_syntax_validation(&self) -> bool {
        true
    }

    fn can_handle(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    }

    fn name(&self) -> &'static str {
        "json"
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &["json"]
    }
}

pub(super) fn collect_handles(
    tree: tree_sitter::Tree,
    path: &Path,
    source: &[u8],
) -> Vec<SelectionHandle> {
    let mut handles = Vec::new();
    visit_json_nodes(
        tree.root_node(),
        source,
        None,
        &mut |span, kind, name, text| {
            handles.push(SelectionHandle::from_parts(
                path.to_path_buf(),
                span,
                kind.to_string(),
                name,
                text.into_owned(),
            ));
        },
    );
    handles
}

pub(super) fn collect_named_nodes(
    tree: &tree_sitter::Tree,
    source: &[u8],
) -> Result<Vec<NamedNode>, NameProjectionError> {
    let mut names = Vec::new();
    let valid = visit_json_nodes(
        tree.root_node(),
        source,
        None,
        &mut |span, kind, name, _| {
            if let Some(name) = name {
                names.push(NamedNode {
                    span,
                    kind: kind.to_string(),
                    name,
                });
            }
        },
    );
    if valid {
        Ok(names)
    } else {
        Err(NameProjectionError::InvalidSpan)
    }
}

fn visit_json_nodes<'source>(
    node: Node<'_>,
    source: &'source [u8],
    inherited_name: Option<String>,
    visit: &mut impl FnMut(Span, &'static str, Option<String>, Cow<'source, str>),
) -> bool {
    if node.kind() == "pair" {
        return visit_pair(node, source, visit);
    }

    let mut valid = true;
    if let Some(kind) = normalized_kind(node.kind()) {
        let span = Span {
            start: node.start_byte(),
            end: node.end_byte(),
        };
        if let Some(text) = node_source(node, source) {
            visit(span, kind, inherited_name.clone(), text);
        } else {
            valid = false;
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        valid &= visit_json_nodes(child, source, inherited_name.clone(), visit);
    }
    valid
}

fn visit_pair<'source>(
    node: Node<'_>,
    source: &'source [u8],
    visit: &mut impl FnMut(Span, &'static str, Option<String>, Cow<'source, str>),
) -> bool {
    let key_node = match node.child_by_field_name("key") {
        Some(node) => node,
        None => return false,
    };

    let value_node = match node.child_by_field_name("value") {
        Some(node) => node,
        None => return false,
    };

    let key_raw = match node_source(key_node, source) {
        Some(text) => text,
        None => return false,
    };

    let key_name = decode_json_string(&key_raw).unwrap_or(key_raw.trim_matches('"').to_string());
    let key_span = Span {
        start: key_node.start_byte(),
        end: key_node.end_byte(),
    };

    visit(key_span, "key", Some(key_name.clone()), key_raw);

    visit_json_nodes(value_node, source, Some(key_name), visit)
}

fn normalized_kind(node_kind: &str) -> Option<&'static str> {
    match node_kind {
        "object" => Some("object"),
        "array" => Some("array"),
        "string" => Some("string"),
        "number" => Some("number"),
        "true" | "false" => Some("boolean"),
        "null" => Some("null"),
        _ => None,
    }
}

fn decode_json_string(text: &str) -> Option<String> {
    serde_json::from_str::<String>(text).ok()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::JsonProvider;
    use crate::error::IdenteditError;
    use crate::provider::StructureProvider;

    #[test]
    fn parse_preserves_raw_key_and_string_text_while_decoding_names() {
        let provider = JsonProvider;
        let source = br#"{"na\u006de":"va\u004Cue","quote\"key":"line\nvalue"}"#;

        let handles = provider
            .parse(Path::new("fixture.json"), source)
            .expect("json parse should succeed");

        let name_key = handles
            .iter()
            .find(|handle| handle.kind == "key" && handle.name.as_deref() == Some("name"))
            .expect("decoded key handle should exist");
        assert_eq!(name_key.text, r#""na\u006de""#);

        let decoded_value = handles
            .iter()
            .find(|handle| handle.kind == "string" && handle.name.as_deref() == Some("name"))
            .expect("decoded string value handle should exist");
        assert_eq!(decoded_value.text, r#""va\u004Cue""#);

        let quoted_key = handles
            .iter()
            .find(|handle| handle.kind == "key" && handle.name.as_deref() == Some("quote\"key"))
            .expect("escaped-quote key handle should exist");
        assert_eq!(quoted_key.text, r#""quote\"key""#);

        let multiline_value = handles
            .iter()
            .find(|handle| handle.kind == "string" && handle.name.as_deref() == Some("quote\"key"))
            .expect("escaped newline value handle should exist");
        assert_eq!(multiline_value.text, r#""line\nvalue""#);
    }

    #[test]
    fn parse_normalizes_kind_mapping_for_json_value_nodes() {
        let provider = JsonProvider;
        let source = br#"{"b1":true,"b2":false,"n":null,"num":42,"arr":[1],"obj":{"x":1},"s":"x"}"#;

        let handles = provider
            .parse(Path::new("fixture.json"), source)
            .expect("json parse should succeed");

        assert!(
            handles
                .iter()
                .any(|handle| handle.kind == "boolean" && handle.text == "true")
        );
        assert!(
            handles
                .iter()
                .any(|handle| handle.kind == "boolean" && handle.text == "false")
        );
        assert!(
            handles
                .iter()
                .any(|handle| handle.kind == "null" && handle.text == "null")
        );
        assert!(
            handles
                .iter()
                .any(|handle| handle.kind == "number" && handle.text == "42")
        );
        assert!(
            handles
                .iter()
                .any(|handle| handle.kind == "array" && handle.name.as_deref() == Some("arr"))
        );
        assert!(
            handles
                .iter()
                .any(|handle| handle.kind == "object" && handle.name.as_deref() == Some("obj"))
        );
        assert!(
            handles
                .iter()
                .any(|handle| handle.kind == "string" && handle.name.as_deref() == Some("s"))
        );
    }

    #[test]
    fn parse_propagates_immediate_pair_key_as_value_name() {
        let provider = JsonProvider;
        let source = br#"{"outer":{"inner":1}}"#;

        let handles = provider
            .parse(Path::new("fixture.json"), source)
            .expect("json parse should succeed");

        let nested_number = handles
            .iter()
            .find(|handle| handle.kind == "number" && handle.text == "1")
            .expect("nested number handle should exist");
        assert_eq!(nested_number.name.as_deref(), Some("inner"));

        let inner_object = handles
            .iter()
            .find(|handle| {
                handle.kind == "object"
                    && handle.name.as_deref() == Some("outer")
                    && handle.text.contains("\"inner\":1")
            })
            .expect("inner object should inherit outer key name");
        assert_eq!(inner_object.name.as_deref(), Some("outer"));
    }

    #[test]
    fn parse_rejects_invalid_json_syntax() {
        let provider = JsonProvider;
        let source = br#"{"broken": [1,}"#;

        let error = provider
            .parse(Path::new("fixture.json"), source)
            .expect_err("invalid json should fail parse");

        match error {
            IdenteditError::ParseFailure { provider, message } => {
                assert_eq!(provider, "json");
                assert!(message.contains("Syntax errors"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn parse_decodes_surrogate_pair_unicode_escapes() {
        let provider = JsonProvider;
        let source = br#"{"\uD83D\uDE03":"\uD83D\uDE80"}"#;

        let handles = provider
            .parse(Path::new("fixture.json"), source)
            .expect("json parse should succeed");

        let emoji_key = handles
            .iter()
            .find(|handle| handle.kind == "key" && handle.name.as_deref() == Some("😃"))
            .expect("emoji key handle should exist");
        assert_eq!(emoji_key.text, r#""\uD83D\uDE03""#);

        let emoji_value = handles
            .iter()
            .find(|handle| handle.kind == "string" && handle.name.as_deref() == Some("😃"))
            .expect("emoji value handle should exist");
        assert_eq!(emoji_value.text, r#""\uD83D\uDE80""#);
    }

    #[test]
    fn parse_decodes_control_escape_sequences_in_strings() {
        let provider = JsonProvider;
        let source = br#"{"ctrl":"a\tb\\c\"d"}"#;

        let handles = provider
            .parse(Path::new("fixture.json"), source)
            .expect("json parse should succeed");

        let escaped_value = handles
            .iter()
            .find(|handle| handle.kind == "string" && handle.name.as_deref() == Some("ctrl"))
            .expect("escaped string value handle should exist");
        assert_eq!(escaped_value.text, r#""a\tb\\c\"d""#);
    }
}
