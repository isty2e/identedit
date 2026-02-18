use std::borrow::Cow;

use tree_sitter::Node;

pub(crate) fn node_text(node: Node<'_>, source: &[u8]) -> Option<String> {
    let start = node.start_byte();
    let end = node.end_byte();

    if start > end || end > source.len() {
        return None;
    }

    Some(String::from_utf8_lossy(&source[start..end]).to_string())
}

pub(crate) fn normalize_bare_cr_for_parser(source: &[u8]) -> Cow<'_, [u8]> {
    let mut contains_standalone_cr = false;

    for (index, byte) in source.iter().enumerate() {
        if *byte == b'\r' && source.get(index + 1) != Some(&b'\n') {
            contains_standalone_cr = true;
            break;
        }
    }

    if !contains_standalone_cr {
        return Cow::Borrowed(source);
    }

    let mut normalized = source.to_vec();
    for (index, byte) in source.iter().enumerate() {
        if *byte == b'\r' && source.get(index + 1) != Some(&b'\n') {
            normalized[index] = b'\n';
        }
    }

    Cow::Owned(normalized)
}

#[cfg(test)]
mod tests {
    use super::normalize_bare_cr_for_parser;

    #[test]
    fn normalize_bare_cr_for_parser_converts_only_standalone_cr() {
        let source = b"line1\rline2\r\nline3\n";
        let normalized = normalize_bare_cr_for_parser(source);

        assert_eq!(
            normalized.as_ref(),
            b"line1\nline2\r\nline3\n",
            "standalone CR should be normalized while CRLF should be preserved"
        );
        assert_eq!(
            normalized.len(),
            source.len(),
            "normalization must preserve byte length for span stability"
        );
    }

    #[test]
    fn normalize_bare_cr_for_parser_is_noop_when_not_needed() {
        let source = b"line1\r\nline2\n";
        let normalized = normalize_bare_cr_for_parser(source);
        assert_eq!(normalized.as_ref(), source);
    }
}
