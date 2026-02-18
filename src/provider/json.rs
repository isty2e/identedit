use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::provider::{StructureProvider, node_text};

pub struct JsonProvider;

impl StructureProvider for JsonProvider {
    fn parse(&self, path: &Path, source: &[u8]) -> Result<Vec<SelectionHandle>, IdenteditError> {
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

        let mut handles = Vec::new();
        collect_json_nodes(tree.root_node(), path, source, None, &mut handles);
        Ok(handles)
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

fn collect_json_nodes(
    node: Node<'_>,
    path: &Path,
    source: &[u8],
    inherited_name: Option<String>,
    handles: &mut Vec<SelectionHandle>,
) {
    if node.kind() == "pair" {
        collect_pair(node, path, source, handles);
        return;
    }

    if let Some(kind) = normalized_kind(node.kind())
        && let Some(text) = node_text(node, source)
    {
        let span = Span {
            start: node.start_byte(),
            end: node.end_byte(),
        };
        let normalized_text = normalize_value_text(kind, &text);

        handles.push(SelectionHandle::from_parts(
            path.to_path_buf(),
            span,
            kind.to_string(),
            inherited_name.clone(),
            normalized_text,
        ));
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_json_nodes(child, path, source, inherited_name.clone(), handles);
    }
}

fn collect_pair(node: Node<'_>, path: &Path, source: &[u8], handles: &mut Vec<SelectionHandle>) {
    let key_node = match node.child_by_field_name("key") {
        Some(node) => node,
        None => return,
    };

    let value_node = match node.child_by_field_name("value") {
        Some(node) => node,
        None => return,
    };

    let key_raw = match node_text(key_node, source) {
        Some(text) => text,
        None => return,
    };

    let key_name = decode_json_string(&key_raw).unwrap_or(key_raw.trim_matches('"').to_string());
    let key_span = Span {
        start: key_node.start_byte(),
        end: key_node.end_byte(),
    };

    handles.push(SelectionHandle::from_parts(
        path.to_path_buf(),
        key_span,
        "key".to_string(),
        Some(key_name.clone()),
        key_name.clone(),
    ));

    collect_json_nodes(value_node, path, source, Some(key_name), handles);
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

fn normalize_value_text(kind: &str, text: &str) -> String {
    if kind == "string" {
        return decode_json_string(text).unwrap_or_else(|| text.to_string());
    }

    text.to_string()
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
    fn parse_decodes_escaped_key_and_string_value_text() {
        let provider = JsonProvider;
        let source = br#"{"na\u006de":"va\u004Cue","quote\"key":"line\nvalue"}"#;

        let handles = provider
            .parse(Path::new("fixture.json"), source)
            .expect("json parse should succeed");

        let name_key = handles
            .iter()
            .find(|handle| handle.kind == "key" && handle.name.as_deref() == Some("name"))
            .expect("decoded key handle should exist");
        assert_eq!(name_key.text, "name");

        let decoded_value = handles
            .iter()
            .find(|handle| handle.kind == "string" && handle.name.as_deref() == Some("name"))
            .expect("decoded string value handle should exist");
        assert_eq!(decoded_value.text, "vaLue");

        let quoted_key = handles
            .iter()
            .find(|handle| handle.kind == "key" && handle.name.as_deref() == Some("quote\"key"))
            .expect("escaped-quote key handle should exist");
        assert_eq!(quoted_key.text, "quote\"key");

        let multiline_value = handles
            .iter()
            .find(|handle| handle.kind == "string" && handle.name.as_deref() == Some("quote\"key"))
            .expect("escaped newline value handle should exist");
        assert_eq!(multiline_value.text, "line\nvalue");
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
        assert_eq!(emoji_key.text, "😃");

        let emoji_value = handles
            .iter()
            .find(|handle| handle.kind == "string" && handle.name.as_deref() == Some("😃"))
            .expect("emoji value handle should exist");
        assert_eq!(emoji_value.text, "🚀");
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
        assert_eq!(escaped_value.text, "a\tb\\c\"d");
    }
}
