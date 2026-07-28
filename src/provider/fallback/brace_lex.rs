use super::source_lines::{is_line_terminator_byte, line_terminator_len_at};

pub(super) fn brace_block_end(source: &[u8], start: usize, header_end: usize) -> Option<usize> {
    let open_brace_index = find_brace_block_open_index(source, start, header_end)?;

    let mut depth = 0usize;
    let mut saw_open_brace = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_single_quoted_string = false;
    let mut in_double_quoted_string = false;
    let mut in_template_literal = false;
    let mut in_regex_literal = false;
    let mut in_regex_char_class = false;

    let mut index = open_brace_index;
    while index < source.len() {
        let byte = source[index];
        let next = source.get(index + 1).copied();

        if in_line_comment {
            if let Some(terminator_len) = line_terminator_len_at(source, index) {
                in_line_comment = false;
                index += terminator_len;
            } else {
                index += 1;
            }
            continue;
        }

        if in_block_comment {
            if byte == b'*' && next == Some(b'/') {
                in_block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if in_single_quoted_string {
            if byte == b'\\' {
                index += if next.is_some() { 2 } else { 1 };
                continue;
            }

            if byte == b'\'' {
                in_single_quoted_string = false;
            }

            index += 1;
            continue;
        }

        if in_double_quoted_string {
            if byte == b'\\' {
                index += if next.is_some() { 2 } else { 1 };
                continue;
            }

            if byte == b'"' {
                in_double_quoted_string = false;
            }

            index += 1;
            continue;
        }

        if in_template_literal {
            if byte == b'\\' {
                index += if next.is_some() { 2 } else { 1 };
                continue;
            }

            if byte == b'`' {
                in_template_literal = false;
            }

            index += 1;
            continue;
        }

        if in_regex_literal {
            if byte == b'\\' {
                index += if next.is_some() { 2 } else { 1 };
                continue;
            }

            if byte == b'[' {
                in_regex_char_class = true;
                index += 1;
                continue;
            }

            if byte == b']' && in_regex_char_class {
                in_regex_char_class = false;
                index += 1;
                continue;
            }

            if byte == b'/' && !in_regex_char_class {
                in_regex_literal = false;
                index += 1;
                continue;
            }

            index += 1;
            continue;
        }

        if byte == b'/' && next == Some(b'/') {
            in_line_comment = true;
            index += 2;
            continue;
        }

        if byte == b'/' && next == Some(b'*') {
            in_block_comment = true;
            index += 2;
            continue;
        }

        if byte == b'/' && should_start_regex_literal(source, start, index) {
            in_regex_literal = true;
            in_regex_char_class = false;
            index += 1;
            continue;
        }

        if byte == b'\'' {
            in_single_quoted_string = true;
            index += 1;
            continue;
        }

        if byte == b'"' {
            in_double_quoted_string = true;
            index += 1;
            continue;
        }

        if byte == b'`' {
            in_template_literal = true;
            index += 1;
            continue;
        }

        if byte == b'{' {
            depth += 1;
            saw_open_brace = true;
            index += 1;
            continue;
        }

        if byte == b'}' && saw_open_brace {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(advance_to_line_end(source, index + 1));
            }
        }

        index += 1;
    }

    None
}

fn find_brace_block_open_index(source: &[u8], start: usize, header_end: usize) -> Option<usize> {
    if let Some(open_in_header) = source[start..header_end]
        .iter()
        .position(|byte| *byte == b'{')
    {
        return Some(start + open_in_header);
    }

    let mut index = header_end;
    while index < source.len() {
        if let Some(terminator_len) = line_terminator_len_at(source, index) {
            index += terminator_len;
            continue;
        }

        let byte = source[index];
        let next = source.get(index + 1).copied();

        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }

        if byte == b'/' && next == Some(b'/') {
            index += 2;
            while index < source.len() {
                if let Some(terminator_len) = line_terminator_len_at(source, index) {
                    index += terminator_len;
                    break;
                }
                index += 1;
            }
            continue;
        }

        if byte == b'/' && next == Some(b'*') {
            index += 2;
            while index < source.len() {
                if source[index] == b'*' && source.get(index + 1) == Some(&b'/') {
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }

        return (byte == b'{').then_some(index);
    }

    None
}

pub(super) fn should_start_regex_literal(source: &[u8], start: usize, slash_index: usize) -> bool {
    if slash_index <= start {
        return true;
    }

    let (cursor, crossed_line_boundary) =
        match previous_significant_index(source, start, slash_index) {
            Some(index) => (index, false),
            None => match previous_significant_index_across_lines(source, start, slash_index) {
                Some(index) => (index, true),
                None => return true,
            },
        };
    let byte = source[cursor];

    if crossed_line_boundary && byte == b';' {
        return has_regex_literal_terminator_on_line(source, slash_index);
    }

    if is_identifier_byte(byte) {
        let token_end = cursor + 1;
        let mut token_start = cursor;
        while token_start > start && is_identifier_byte(source[token_start - 1]) {
            token_start -= 1;
        }

        if is_regex_prefix_keyword(&source[token_start..token_end]) {
            return true;
        }
    }

    if is_postfix_update_operator(source, start, cursor) {
        return false;
    }

    if byte == b')' && is_regex_after_control_flow_paren(source, start, cursor) {
        return true;
    }

    matches!(
        byte,
        b'(' | b'['
            | b'{'
            | b','
            | b':'
            | b';'
            | b'='
            | b'!'
            | b'?'
            | b'+'
            | b'-'
            | b'*'
            | b'%'
            | b'&'
            | b'|'
            | b'^'
            | b'~'
            | b'<'
            | b'>'
    )
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

fn previous_significant_index(source: &[u8], start: usize, before: usize) -> Option<usize> {
    let mut cursor = before;

    'scan: while cursor > start {
        cursor -= 1;
        let byte = source[cursor];

        if is_line_terminator_byte(source, cursor) {
            return None;
        }

        if byte.is_ascii_whitespace() {
            continue;
        }

        if byte == b'/'
            && cursor > start
            && source[cursor - 1] == b'*'
            && let Some(comment_open) = find_block_comment_open(source, start, cursor - 1)
        {
            cursor = comment_open;
            continue 'scan;
        }

        return Some(cursor);
    }

    None
}

fn is_regex_after_control_flow_paren(source: &[u8], start: usize, close_paren: usize) -> bool {
    let Some(open_paren) = find_matching_open_paren(source, start, close_paren) else {
        return false;
    };

    let Some(keyword_end) = previous_significant_index(source, start, open_paren) else {
        return false;
    };
    if !is_identifier_byte(source[keyword_end]) {
        return false;
    }

    let mut keyword_start = keyword_end;
    while keyword_start > start && is_identifier_byte(source[keyword_start - 1]) {
        keyword_start -= 1;
    }

    matches!(
        &source[keyword_start..=keyword_end],
        b"if" | b"while" | b"for" | b"with" | b"switch" | b"catch"
    )
}

fn find_matching_open_paren(source: &[u8], start: usize, close_paren: usize) -> Option<usize> {
    if source.get(close_paren) != Some(&b')') {
        return None;
    }

    let mut stack = Vec::new();
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_single_quoted_string = false;
    let mut in_double_quoted_string = false;
    let mut in_template_literal = false;
    let mut in_regex_literal = false;
    let mut in_regex_char_class = false;

    let mut index = start;
    while index <= close_paren {
        let byte = source[index];
        let next = source.get(index + 1).copied();

        if in_line_comment {
            if let Some(terminator_len) = line_terminator_len_at(source, index) {
                in_line_comment = false;
                index += terminator_len;
            } else {
                index += 1;
            }
            continue;
        }

        if in_block_comment {
            if byte == b'*' && next == Some(b'/') {
                in_block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if in_single_quoted_string {
            if byte == b'\\' {
                index += if next.is_some() { 2 } else { 1 };
                continue;
            }

            if byte == b'\'' {
                in_single_quoted_string = false;
            }

            index += 1;
            continue;
        }

        if in_double_quoted_string {
            if byte == b'\\' {
                index += if next.is_some() { 2 } else { 1 };
                continue;
            }

            if byte == b'"' {
                in_double_quoted_string = false;
            }

            index += 1;
            continue;
        }

        if in_template_literal {
            if byte == b'\\' {
                index += if next.is_some() { 2 } else { 1 };
                continue;
            }

            if byte == b'`' {
                in_template_literal = false;
            }

            index += 1;
            continue;
        }

        if in_regex_literal {
            if byte == b'\\' {
                index += if next.is_some() { 2 } else { 1 };
                continue;
            }

            if byte == b'[' {
                in_regex_char_class = true;
                index += 1;
                continue;
            }

            if byte == b']' && in_regex_char_class {
                in_regex_char_class = false;
                index += 1;
                continue;
            }

            if byte == b'/' && !in_regex_char_class {
                in_regex_literal = false;
            }

            index += 1;
            continue;
        }

        if byte == b'/' && next == Some(b'/') {
            in_line_comment = true;
            index += 2;
            continue;
        }

        if byte == b'/' && next == Some(b'*') {
            in_block_comment = true;
            index += 2;
            continue;
        }

        if byte == b'/' && should_start_regex_literal_for_paren_scan(source, start, index) {
            in_regex_literal = true;
            in_regex_char_class = false;
            index += 1;
            continue;
        }

        if byte == b'\'' {
            in_single_quoted_string = true;
            index += 1;
            continue;
        }

        if byte == b'"' {
            in_double_quoted_string = true;
            index += 1;
            continue;
        }

        if byte == b'`' {
            in_template_literal = true;
            index += 1;
            continue;
        }

        if byte == b'(' {
            stack.push(index);
            index += 1;
            continue;
        }

        if byte == b')' {
            let open_index = stack.pop()?;
            if index == close_paren {
                return Some(open_index);
            }
        }

        index += 1;
    }

    None
}

fn should_start_regex_literal_for_paren_scan(
    source: &[u8],
    start: usize,
    slash_index: usize,
) -> bool {
    if slash_index <= start {
        return true;
    }

    let (cursor, crossed_line_boundary) =
        match previous_significant_index(source, start, slash_index) {
            Some(index) => (index, false),
            None => match previous_significant_index_across_lines(source, start, slash_index) {
                Some(index) => (index, true),
                None => return true,
            },
        };
    let byte = source[cursor];

    if crossed_line_boundary && byte == b';' {
        return has_regex_literal_terminator_on_line(source, slash_index);
    }

    if is_identifier_byte(byte) {
        let token_end = cursor + 1;
        let mut token_start = cursor;
        while token_start > start && is_identifier_byte(source[token_start - 1]) {
            token_start -= 1;
        }

        if is_regex_prefix_keyword(&source[token_start..token_end]) {
            return true;
        }

        return false;
    }

    if is_postfix_update_operator(source, start, cursor) {
        return false;
    }

    matches!(
        byte,
        b'(' | b'['
            | b'{'
            | b','
            | b':'
            | b';'
            | b'='
            | b'!'
            | b'?'
            | b'+'
            | b'-'
            | b'*'
            | b'%'
            | b'&'
            | b'|'
            | b'^'
            | b'~'
            | b'<'
            | b'>'
    )
}

fn is_postfix_update_operator(source: &[u8], start: usize, cursor: usize) -> bool {
    let operator = source[cursor];
    if !matches!(operator, b'+' | b'-') {
        return false;
    }

    let Some(prev) = previous_significant_index(source, start, cursor) else {
        return false;
    };
    if source[prev] != operator {
        return false;
    }

    let Some(target) = previous_significant_index(source, start, prev) else {
        return false;
    };

    is_identifier_byte(source[target]) || matches!(source[target], b')' | b']')
}

fn previous_significant_index_across_lines(
    source: &[u8],
    start: usize,
    before: usize,
) -> Option<usize> {
    let mut cursor = before;

    'scan: while cursor > start {
        cursor -= 1;
        let byte = source[cursor];
        if is_line_terminator_byte(source, cursor) {
            continue;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }

        if byte == b'/'
            && cursor > start
            && source[cursor - 1] == b'*'
            && let Some(comment_open) = find_block_comment_open(source, start, cursor - 1)
        {
            cursor = comment_open;
            continue 'scan;
        }

        let line_start = line_start_index(source, start, cursor);
        let first_non_whitespace = source[line_start..=cursor]
            .iter()
            .position(|candidate| !candidate.is_ascii_whitespace())
            .map(|offset| line_start + offset);
        if let Some(first_non_whitespace) = first_non_whitespace
            && first_non_whitespace < cursor
            && source[first_non_whitespace] == b'/'
            && source[first_non_whitespace + 1] == b'/'
        {
            cursor = line_start;
            continue 'scan;
        }

        return Some(cursor);
    }

    None
}

fn line_start_index(source: &[u8], start: usize, mut index: usize) -> usize {
    while index > start {
        if is_line_terminator_byte(source, index - 1) {
            break;
        }
        index -= 1;
    }

    index
}

fn has_regex_literal_terminator_on_line(source: &[u8], slash_index: usize) -> bool {
    let mut cursor = slash_index + 1;
    let mut in_char_class = false;

    while cursor < source.len() {
        if line_terminator_len_at(source, cursor).is_some() {
            return false;
        }
        let byte = source[cursor];

        if byte == b'\\' {
            cursor += 1;
            if cursor < source.len() {
                cursor += 1;
            }
            continue;
        }

        if byte == b'[' && !in_char_class {
            in_char_class = true;
            cursor += 1;
            continue;
        }

        if byte == b']' && in_char_class {
            in_char_class = false;
            cursor += 1;
            continue;
        }

        if byte == b'/'
            && !in_char_class
            && is_valid_regex_terminator_suffix(source, slash_index, cursor)
        {
            return true;
        }

        cursor += 1;
    }

    false
}

fn is_valid_regex_terminator_suffix(
    source: &[u8],
    regex_start_index: usize,
    candidate_slash_index: usize,
) -> bool {
    let Some(&next) = source.get(candidate_slash_index + 1) else {
        return true;
    };

    if candidate_slash_index > regex_start_index
        && source[candidate_slash_index - 1] == b':'
        && next == b'/'
    {
        return false;
    }

    if candidate_slash_index > regex_start_index
        && source[candidate_slash_index - 1] == b'*'
        && has_block_comment_open_between(source, regex_start_index + 1, candidate_slash_index)
    {
        return false;
    }

    if candidate_slash_index > regex_start_index
        && source[candidate_slash_index - 1].is_ascii_whitespace()
        && next == b'*'
        && has_block_comment_close_after_on_line(source, candidate_slash_index + 2)
    {
        return false;
    }

    if candidate_slash_index > regex_start_index && source[candidate_slash_index - 1] == b'/' {
        return false;
    }

    if matches!(next, b'"' | b'\'' | b'`') {
        return false;
    }

    if next == b'/' {
        return false;
    }

    if next.is_ascii_whitespace() {
        return true;
    }

    if is_regex_flag_byte(next) {
        let mut cursor = candidate_slash_index + 1;
        while let Some(&flag) = source.get(cursor) {
            if !is_regex_flag_byte(flag) {
                break;
            }
            cursor += 1;
        }

        let Some(&after_flags) = source.get(cursor) else {
            return true;
        };
        return !is_identifier_byte(after_flags);
    }

    !is_identifier_byte(next)
}

fn has_block_comment_open_between(source: &[u8], start: usize, end: usize) -> bool {
    if end <= start + 1 {
        return false;
    }

    source[start..end].windows(2).any(|pair| pair == b"/*")
}

fn has_block_comment_close_after_on_line(source: &[u8], start: usize) -> bool {
    if start + 1 >= source.len() {
        return false;
    }

    let mut cursor = start;
    while cursor + 1 < source.len() {
        if line_terminator_len_at(source, cursor).is_some() {
            return false;
        }

        if source[cursor] == b'*' && source[cursor + 1] == b'/' {
            return true;
        }

        cursor += 1;
    }

    false
}

fn is_regex_flag_byte(byte: u8) -> bool {
    matches!(byte, b'd' | b'g' | b'i' | b'm' | b's' | b'u' | b'v' | b'y')
}

fn find_block_comment_open(source: &[u8], start: usize, star_index: usize) -> Option<usize> {
    let mut cursor = star_index;
    while cursor > start {
        cursor -= 1;
        if source[cursor] == b'/' && source[cursor + 1] == b'*' {
            return Some(cursor);
        }
    }

    None
}

fn is_regex_prefix_keyword(token: &[u8]) -> bool {
    matches!(
        token,
        b"return"
            | b"do"
            | b"else"
            | b"finally"
            | b"throw"
            | b"yield"
            | b"case"
            | b"delete"
            | b"void"
            | b"typeof"
            | b"instanceof"
            | b"extends"
            | b"default"
            | b"new"
            | b"in"
            | b"of"
            | b"await"
    )
}

fn advance_to_line_end(source: &[u8], mut index: usize) -> usize {
    while index < source.len() {
        if let Some(terminator_len) = line_terminator_len_at(source, index) {
            index += terminator_len;
            break;
        } else {
            index += 1;
        }
    }

    index
}

#[cfg(test)]
mod tests {
    use super::{brace_block_end, should_start_regex_literal};
    use crate::provider::fallback::source_lines::collect_lines;

    #[test]
    fn brace_boundary_ignores_javascript_regex_and_template_literal_braces() {
        let source = "function run(value) {\n  const matcher = /[{}]/g;\n  return `${value} }`;\n}\nnext();\n";
        let lines = collect_lines(source);
        let end = brace_block_end(source.as_bytes(), lines[0].start, lines[0].end)
            .expect("function brace boundary should be found");

        assert_eq!(
            &source[..end],
            "function run(value) {\n  const matcher = /[{}]/g;\n  return `${value} }`;\n}\n"
        );
    }

    #[test]
    fn regex_classifier_distinguishes_prefix_from_division_context() {
        let prefix_source = b"return /[{}]/g;";
        let prefix_slash = prefix_source
            .iter()
            .position(|byte| *byte == b'/')
            .expect("prefix slash should exist");
        assert!(should_start_regex_literal(prefix_source, 0, prefix_slash));

        let division_source = b"value / divisor;";
        let division_slash = division_source
            .iter()
            .position(|byte| *byte == b'/')
            .expect("division slash should exist");
        assert!(!should_start_regex_literal(
            division_source,
            0,
            division_slash
        ));
    }
}
