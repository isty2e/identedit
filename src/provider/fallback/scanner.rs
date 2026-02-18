#[derive(Clone, Copy, PartialEq, Eq)]
enum PythonTripleDelimiter {
    SingleQuote,
    DoubleQuote,
}

#[derive(Default)]
struct CandidateSkipState {
    in_block_comment: bool,
    in_template_literal: bool,
    template_expression_depth: usize,
    template_expression_in_block_comment: bool,
    template_expression_in_nested_template_literal: bool,
    template_expression_in_single_quoted_string: bool,
    template_expression_in_double_quoted_string: bool,
    template_expression_in_regex_literal: bool,
    template_expression_in_regex_char_class: bool,
}

pub(super) struct LineInfo<'a> {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) text: &'a str,
    pub(super) indent: usize,
    pub(super) is_blank: bool,
}

pub(super) fn collect_lines(source_text: &str) -> Vec<LineInfo<'_>> {
    fn push_line<'a>(
        lines: &mut Vec<LineInfo<'a>>,
        source_text: &'a str,
        start: usize,
        end: usize,
    ) {
        let segment = &source_text[start..end];
        let line_without_newline = segment.trim_end_matches(['\r', '\n', '\u{2028}', '\u{2029}']);
        let indent = line_without_newline
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count();
        let is_blank = line_without_newline.trim().is_empty();

        lines.push(LineInfo {
            start,
            end,
            text: line_without_newline,
            indent,
            is_blank,
        });
    }

    let mut lines = Vec::new();
    let source = source_text.as_bytes();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < source.len() {
        if let Some(terminator_len) = line_terminator_len_at(source, index) {
            let end = index + terminator_len;
            push_line(&mut lines, source_text, start, end);
            start = end;
            index = end;
        } else {
            index += 1;
        }
    }

    if start < source.len() {
        push_line(&mut lines, source_text, start, source.len());
    }

    lines
}

pub(super) fn line_terminator_len_at(source: &[u8], index: usize) -> Option<usize> {
    let byte = *source.get(index)?;
    match byte {
        b'\n' => Some(1),
        b'\r' => {
            if source.get(index + 1) == Some(&b'\n') {
                Some(2)
            } else {
                Some(1)
            }
        }
        0xE2 if source.get(index + 1) == Some(&0x80) => match source.get(index + 2) {
            Some(0xA8 | 0xA9) => Some(3),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn is_line_terminator_byte(source: &[u8], index: usize) -> bool {
    match source.get(index).copied() {
        Some(b'\n' | b'\r') => true,
        Some(0xE2) => {
            source.get(index + 1) == Some(&0x80)
                && matches!(source.get(index + 2), Some(0xA8 | 0xA9))
        }
        Some(0x80) => {
            index >= 1
                && source[index - 1] == 0xE2
                && matches!(source.get(index + 1), Some(0xA8 | 0xA9))
        }
        Some(0xA8 | 0xA9) => index >= 2 && source[index - 2] == 0xE2 && source[index - 1] == 0x80,
        _ => false,
    }
}

pub(super) fn line_index_for_offset(lines: &[LineInfo<'_>], offset: usize) -> Option<usize> {
    lines
        .iter()
        .position(|line| offset >= line.start && offset < line.end)
}

pub(super) fn build_candidate_skip_masks(lines: &[LineInfo<'_>]) -> (Vec<bool>, Vec<bool>) {
    let mut block_comment_mask = vec![false; lines.len()];
    let mut template_literal_mask = vec![false; lines.len()];
    let mut state = CandidateSkipState::default();

    for (index, line) in lines.iter().enumerate() {
        block_comment_mask[index] = state.in_block_comment;
        template_literal_mask[index] = state.in_template_literal;
        update_candidate_skip_state(line.text, &mut state);
    }

    (block_comment_mask, template_literal_mask)
}

fn update_candidate_skip_state(line: &str, state: &mut CandidateSkipState) {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    let mut in_single_quoted_string = false;
    let mut in_double_quoted_string = false;
    let mut in_regex_literal = false;
    let mut in_regex_char_class = false;

    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();

        if state.in_block_comment {
            if byte == b'*' && next == Some(b'/') {
                state.in_block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if state.in_template_literal {
            if state.template_expression_in_block_comment {
                if byte == b'*' && next == Some(b'/') {
                    state.template_expression_in_block_comment = false;
                    index += 2;
                } else {
                    index += 1;
                }
                continue;
            }

            if state.template_expression_in_nested_template_literal {
                if byte == b'\\' {
                    index += if next.is_some() { 2 } else { 1 };
                    continue;
                }

                if byte == b'`' {
                    state.template_expression_in_nested_template_literal = false;
                }

                index += 1;
                continue;
            }

            if state.template_expression_in_single_quoted_string {
                if byte == b'\\' {
                    index += if next.is_some() { 2 } else { 1 };
                    continue;
                }

                if byte == b'\'' {
                    state.template_expression_in_single_quoted_string = false;
                }

                index += 1;
                continue;
            }

            if state.template_expression_in_double_quoted_string {
                if byte == b'\\' {
                    index += if next.is_some() { 2 } else { 1 };
                    continue;
                }

                if byte == b'"' {
                    state.template_expression_in_double_quoted_string = false;
                }

                index += 1;
                continue;
            }

            if state.template_expression_in_regex_literal {
                if byte == b'\\' {
                    index += if next.is_some() { 2 } else { 1 };
                    continue;
                }

                if byte == b'[' {
                    state.template_expression_in_regex_char_class = true;
                    index += 1;
                    continue;
                }

                if byte == b']' && state.template_expression_in_regex_char_class {
                    state.template_expression_in_regex_char_class = false;
                    index += 1;
                    continue;
                }

                if byte == b'/' && !state.template_expression_in_regex_char_class {
                    state.template_expression_in_regex_literal = false;
                    index += 1;
                    continue;
                }

                index += 1;
                continue;
            }

            if state.template_expression_depth == 0 {
                if byte == b'\\' {
                    index += if next.is_some() { 2 } else { 1 };
                    continue;
                }

                if byte == b'$' && next == Some(b'{') {
                    state.template_expression_depth = 1;
                    index += 2;
                    continue;
                }

                if byte == b'`' {
                    state.in_template_literal = false;
                    state.template_expression_depth = 0;
                    state.template_expression_in_block_comment = false;
                    state.template_expression_in_nested_template_literal = false;
                    state.template_expression_in_single_quoted_string = false;
                    state.template_expression_in_double_quoted_string = false;
                    state.template_expression_in_regex_literal = false;
                    state.template_expression_in_regex_char_class = false;
                    index += 1;
                    continue;
                }

                index += 1;
                continue;
            }

            if byte == b'/' && next == Some(b'/') {
                break;
            }

            if byte == b'/' && next == Some(b'*') {
                state.template_expression_in_block_comment = true;
                index += 2;
                continue;
            }

            if byte == b'/' && super::should_start_regex_literal(bytes, 0, index) {
                state.template_expression_in_regex_literal = true;
                state.template_expression_in_regex_char_class = false;
                index += 1;
                continue;
            }

            if byte == b'`' {
                state.template_expression_in_nested_template_literal = true;
                index += 1;
                continue;
            }

            if byte == b'\'' {
                state.template_expression_in_single_quoted_string = true;
                index += 1;
                continue;
            }

            if byte == b'"' {
                state.template_expression_in_double_quoted_string = true;
                index += 1;
                continue;
            }

            if byte == b'$' && next == Some(b'{') {
                state.template_expression_depth += 1;
                index += 2;
                continue;
            }

            if byte == b'{' {
                state.template_expression_depth += 1;
                index += 1;
                continue;
            }

            if byte == b'}' {
                state.template_expression_depth = state.template_expression_depth.saturating_sub(1);
                if state.template_expression_depth == 0 {
                    state.template_expression_in_block_comment = false;
                    state.template_expression_in_nested_template_literal = false;
                    state.template_expression_in_single_quoted_string = false;
                    state.template_expression_in_double_quoted_string = false;
                    state.template_expression_in_regex_literal = false;
                    state.template_expression_in_regex_char_class = false;
                }

                index += 1;
                continue;
            }

            index += 1;
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
            break;
        }

        if byte == b'/' && next == Some(b'*') {
            state.in_block_comment = true;
            index += 2;
            continue;
        }

        if byte == b'/' && super::should_start_regex_literal(bytes, 0, index) {
            in_regex_literal = true;
            in_regex_char_class = false;
            index += 1;
            continue;
        }

        if byte == b'`' {
            state.in_template_literal = true;
            state.template_expression_depth = 0;
            state.template_expression_in_block_comment = false;
            state.template_expression_in_nested_template_literal = false;
            state.template_expression_in_single_quoted_string = false;
            state.template_expression_in_double_quoted_string = false;
            state.template_expression_in_regex_literal = false;
            state.template_expression_in_regex_char_class = false;
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

        index += 1;
    }
}

pub(super) fn build_python_multiline_mask(lines: &[LineInfo<'_>]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut state: Option<PythonTripleDelimiter> = None;

    for (index, line) in lines.iter().enumerate() {
        mask[index] = state.is_some();
        update_python_triple_quote_state(line.text, &mut state);
    }

    mask
}

fn update_python_triple_quote_state(line: &str, state: &mut Option<PythonTripleDelimiter>) {
    let line_to_scan = if state.is_none() {
        match find_python_comment_start(line) {
            Some(comment_start) => &line[..comment_start],
            None => line,
        }
    } else {
        line
    };

    let bytes = line_to_scan.as_bytes();
    let mut index = 0usize;
    let mut in_single_quoted_string = false;
    let mut in_double_quoted_string = false;

    while index < bytes.len() {
        if let Some(active_delimiter) = *state {
            if let Some(found_delimiter) = triple_quote_delimiter_at(bytes, index)
                && found_delimiter == active_delimiter
                && !is_escaped(bytes, index)
            {
                *state = None;
                index += 3;
                continue;
            }

            index += 1;
            continue;
        }

        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();

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

        if let Some(delimiter) = triple_quote_delimiter_at(bytes, index)
            && !is_escaped(bytes, index)
        {
            *state = Some(delimiter);
            index += 3;
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

        index += 1;
    }
}

fn triple_quote_delimiter_at(bytes: &[u8], index: usize) -> Option<PythonTripleDelimiter> {
    if index + 2 >= bytes.len() {
        return None;
    }

    if bytes[index] == b'\'' && bytes[index + 1] == b'\'' && bytes[index + 2] == b'\'' {
        return Some(PythonTripleDelimiter::SingleQuote);
    }

    if bytes[index] == b'"' && bytes[index + 1] == b'"' && bytes[index + 2] == b'"' {
        return Some(PythonTripleDelimiter::DoubleQuote);
    }

    None
}

fn find_python_comment_start(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    let mut in_single_quoted_string = false;
    let mut in_double_quoted_string = false;

    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();

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

        if byte == b'#' {
            return Some(index);
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

        index += 1;
    }

    None
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    if index == 0 {
        return false;
    }

    let mut slash_count = 0usize;
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        if bytes[cursor] == b'\\' {
            slash_count += 1;
        } else {
            break;
        }
    }

    slash_count % 2 == 1
}
