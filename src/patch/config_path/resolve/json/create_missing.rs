use tree_sitter::{Node, Tree};

use crate::error::IdenteditError;

use super::super::super::render::array_index_out_of_bounds_error;
use super::super::super::syntax::{PathToken, expected_path_container_error};
use super::super::{named_children, unique_match};
use super::{json_root_value, matching_json_pairs};

pub(in crate::patch::config_path) fn rewrite_json_with_create_missing(
    tree: &Tree,
    source_text: &str,
    path_tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    // Validate syntax without materializing numbers or collapsing duplicate members.
    serde_json::from_str::<&serde_json::value::RawValue>(
        source_text.strip_prefix('\u{feff}').unwrap_or(source_text),
    )
    .map_err(|error| IdenteditError::InvalidRequest {
        message: format!("Config path create-missing could not parse JSON document: {error}"),
    })?;

    let root = json_root_value(tree.root_node()).ok_or_else(|| IdenteditError::InvalidRequest {
        message: "JSON document has no root value".to_string(),
    })?;
    let mut current = root;

    for (index, token) in path_tokens.iter().enumerate() {
        match token {
            PathToken::Key(key) => {
                if current.kind() != "object" {
                    return Err(expected_path_container_error(
                        raw_path,
                        token,
                        current.kind(),
                    ));
                }

                let matches = matching_json_pairs(current, source_text.as_bytes(), key);
                if matches.is_empty() {
                    return insert_json_member(
                        source_text,
                        root,
                        current,
                        &path_tokens[index..],
                        raw_path,
                        new_text,
                    );
                }

                let pair = unique_match(raw_path, token, matches)?;
                current = pair.child_by_field_name("value").ok_or_else(|| {
                    IdenteditError::InvalidRequest {
                        message: format!(
                            "Config path '{raw_path}' matched key '{key}' without a value node"
                        ),
                    }
                })?;
            }
            PathToken::Index(index) => {
                if current.kind() != "array" {
                    return Err(expected_path_container_error(
                        raw_path,
                        token,
                        current.kind(),
                    ));
                }
                let elements = named_children(current);
                current = *elements.get(*index).ok_or_else(|| {
                    array_index_out_of_bounds_error(raw_path, *index, elements.len())
                })?;
            }
        }
    }

    Err(IdenteditError::InvalidRequest {
        message: format!("Config path '{raw_path}' already exists; no missing member to create"),
    })
}

pub(in crate::patch::config_path) fn render_missing_json_object(
    tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    Ok(format!(
        "{{{}}}",
        render_missing_json_member(tokens, raw_path, new_text, ":")?
    ))
}

fn render_missing_json_member(
    tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
    outer_colon: &str,
) -> Result<String, IdenteditError> {
    let mut member = String::new();
    for (index, token) in tokens.iter().enumerate() {
        let key = match token {
            PathToken::Key(key) => key,
            PathToken::Index(index) => {
                return Err(array_index_out_of_bounds_error(raw_path, *index, 0));
            }
        };
        if index > 0 {
            member.push('{');
        }
        member.push_str(&serde_json::to_string(key).expect("JSON key serialization is infallible"));
        member.push_str(if index == 0 { outer_colon } else { ":" });
    }
    member.push_str(new_text);
    member.extend(std::iter::repeat_n('}', tokens.len().saturating_sub(1)));
    Ok(member)
}

fn insert_json_member(
    source: &str,
    root: Node<'_>,
    object: Node<'_>,
    tokens: &[PathToken],
    raw_path: &str,
    new_text: &str,
) -> Result<String, IdenteditError> {
    let pairs = named_children(object)
        .into_iter()
        .filter(|node| node.kind() == "pair")
        .collect::<Vec<_>>();
    let (offset, separator, colon) = if let Some(last) = pairs.last() {
        let key = last
            .child_by_field_name("key")
            .expect("validated JSON pair has key");
        let value = last
            .child_by_field_name("value")
            .expect("validated JSON pair has value");
        let gap = if pairs.len() > 1 {
            let previous = pairs[pairs.len() - 2];
            source[previous.end_byte()..last.start_byte()]
                .split_once(',')
                .expect("validated adjacent JSON pairs have comma")
                .1
        } else {
            &source[object.start_byte() + 1..last.start_byte()]
        };
        (
            last.end_byte(),
            format!(",{gap}"),
            &source[key.end_byte()..value.start_byte()],
        )
    } else {
        let trivia = &source[object.start_byte() + 1..object.end_byte() - 1];
        let separator = if let Some(newline) = last_line_ending(trivia) {
            let closing_indent = trivia.rsplit(['\r', '\n']).next().unwrap_or("");
            format!("{newline}{closing_indent}  ")
        } else {
            String::new()
        };
        let colon = if separator.is_empty() { ":" } else { ": " };
        (object.start_byte() + 1, separator, colon)
    };

    let member = render_missing_json_member(tokens, raw_path, new_text, colon)?;
    let insertion = format!("{separator}{member}");
    let mut updated = source[root.start_byte()..root.end_byte()].to_string();
    updated.insert_str(offset - root.start_byte(), &insertion);
    Ok(updated)
}

fn last_line_ending(text: &str) -> Option<&'static str> {
    let index = text.rfind(['\r', '\n'])?;
    Some(if text.as_bytes()[index] == b'\r' {
        "\r"
    } else if index > 0 && text.as_bytes()[index - 1] == b'\r' {
        "\r\n"
    } else {
        "\n"
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::patch::config_path::safety::{ConfigFormat, parse_tree_for_format};

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn insertion_preserves_every_original_byte_and_raw_payload(
            key in ".{0,40}",
            leaf in ".{0,40}",
            value in ".{0,80}",
            newline in prop::sample::select(vec!["\n", "\r\n", "\r"]),
            indent in prop::sample::select(vec![" ", "  ", "\t", "    "]),
        ) {
            let prefix = format!("{{\"z\":1e3,\"dup\":1,\"dup\":2,\"obj\":{{{newline}{indent}\"keep\":1.50");
            let suffix = format!("{newline}}},\"escaped\":\"\\u0061\"}}");
            let source = format!("{prefix}{suffix}");
            // Include duplicate members and unbounded numeric spelling in the new value too.
            let payload = format!(" \t{{\"v\":{},\"n\":12345678901234567890123,\"n\":1.50}}\r\n", serde_json::to_string(&value).unwrap());
            let new_key = format!("new{key}");
            let path = vec![PathToken::Key("obj".into()), PathToken::Key(new_key.clone()), PathToken::Key(leaf.clone())];
            let tree = parse_tree_for_format(&ConfigFormat::Json, source.as_bytes()).unwrap();
            let updated = rewrite_json_with_create_missing(&tree, &source, &path, "obj.new.leaf", &payload).unwrap();

            prop_assert!(updated.starts_with(&prefix));
            prop_assert!(updated.ends_with(&suffix));
            let insertion = &updated[prefix.len()..updated.len() - suffix.len()];
            prop_assert!(insertion.contains(&payload));
            prop_assert_eq!(updated.len(), source.len() + insertion.len());
            let reparsed = parse_tree_for_format(&ConfigFormat::Json, updated.as_bytes()).unwrap();
            prop_assert!(!reparsed.root_node().has_error());
            let document: serde_json::Value = serde_json::from_str(&updated).unwrap();
            prop_assert_eq!(&document["obj"][&new_key][&leaf]["v"], &serde_json::Value::String(value));
        }

        #[test]
        fn generated_missing_key_chain_preserves_key_identity_and_payload(
            keys in prop::collection::vec(".{0,30}", 1..12),
            payload in ".{0,100}",
        ) {
            let tokens = keys.iter().cloned().map(PathToken::Key).collect::<Vec<_>>();
            let raw = format!(" \r\n{}\t", serde_json::to_string(&payload).unwrap());
            let text = render_missing_json_object(&tokens, "path", &raw).unwrap();
            prop_assert!(text.contains(&raw));
            let document: serde_json::Value = serde_json::from_str(&text).unwrap();
            let mut current = &document;
            for key in keys {
                current = &current[&key];
            }
            prop_assert_eq!(current, &serde_json::Value::String(payload));
        }
    }
}
