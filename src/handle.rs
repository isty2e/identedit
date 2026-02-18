use std::path::PathBuf;

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::hash::{hash_text, shorten_hex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionHandle {
    pub file: PathBuf,
    pub span: Span,
    pub kind: String,
    pub name: Option<String>,
    pub identity: String,
    pub expected_old_hash: String,
    pub text: String,
}

impl SelectionHandle {
    pub fn from_parts(
        file: PathBuf,
        span: Span,
        kind: String,
        name: Option<String>,
        text: String,
    ) -> Self {
        let identity = compute_identity(&kind, name.as_deref(), &text);
        let expected_old_hash = hash_text(&text);

        Self {
            file,
            span,
            kind,
            name,
            identity,
            expected_old_hash,
            text,
        }
    }
}

pub fn compute_identity(kind: &str, name: Option<&str>, text: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(kind.as_bytes());
    hasher.update(b"\n");

    if let Some(symbol_name) = name {
        hasher.update(symbol_name.as_bytes());
    }

    hasher.update(b"\n");
    hasher.update(text.as_bytes());
    let full_hex = hasher.finalize().to_hex().to_string();
    shorten_hex(&full_hex)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{SelectionHandle, Span, compute_identity};

    #[test]
    fn compute_identity_is_deterministic() {
        let first = compute_identity("function_definition", Some("process_data"), "def a(): pass");
        let second = compute_identity("function_definition", Some("process_data"), "def a(): pass");
        assert_eq!(first, second);
    }

    #[test]
    fn compute_identity_changes_when_content_changes() {
        let original =
            compute_identity("function_definition", Some("process_data"), "def a(): pass");
        let changed_text = compute_identity(
            "function_definition",
            Some("process_data"),
            "def a():\n    pass",
        );
        let changed_name = compute_identity("function_definition", Some("other"), "def a(): pass");
        let changed_kind =
            compute_identity("class_definition", Some("process_data"), "def a(): pass");

        assert_ne!(original, changed_text);
        assert_ne!(original, changed_name);
        assert_ne!(original, changed_kind);
    }

    #[test]
    fn compute_identity_distinguishes_canonical_equivalent_unicode_bytes() {
        let reordered_combining_marks = compute_identity(
            "function_definition",
            Some("process_data"),
            "return 'a\u{0301}\u{0323}'",
        );
        let canonical_order = compute_identity(
            "function_definition",
            Some("process_data"),
            "return 'a\u{0323}\u{0301}'",
        );

        assert_ne!(
            reordered_combining_marks, canonical_order,
            "identity hashing should remain byte-sensitive even for canonically equivalent Unicode"
        );
    }

    #[test]
    fn compute_identity_uses_fixed_hex_prefix_length() {
        let identity =
            compute_identity("function_definition", Some("process_data"), "def a(): pass");
        assert_eq!(identity.len(), crate::changeset::HASH_HEX_LEN);
        assert!(
            identity
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }

    #[test]
    fn selection_handle_expected_hash_uses_fixed_hex_prefix_length() {
        let handle = SelectionHandle::from_parts(
            PathBuf::from("example.py"),
            Span { start: 0, end: 13 },
            "function_definition".to_string(),
            Some("process_data".to_string()),
            "def process_data(value):\n    return value + 1\n".to_string(),
        );
        assert_eq!(
            handle.expected_old_hash.len(),
            crate::changeset::HASH_HEX_LEN
        );
        assert!(
            handle
                .expected_old_hash
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }
}
