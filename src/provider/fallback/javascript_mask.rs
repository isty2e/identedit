use super::brace_lex::should_start_regex_literal;
use super::source_lines::LineInfo;

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

            if byte == b'/' && should_start_regex_literal(bytes, 0, index) {
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

        if byte == b'/' && should_start_regex_literal(bytes, 0, index) {
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

#[cfg(test)]
mod tests {
    use super::{super::source_lines::collect_lines, build_candidate_skip_masks};

    #[test]
    fn masks_lines_inside_block_comments_and_template_literal_raw_text() {
        let source = "/*\nfunction hidden() {}\n*/\n`raw\nfunction hidden_template() {}\n`\nfunction visible() {}\n";
        let lines = collect_lines(source);
        let (block_comments, template_literals) = build_candidate_skip_masks(&lines);

        assert!(block_comments[1]);
        assert!(template_literals[4]);
        assert!(!block_comments[6]);
        assert!(!template_literals[6]);
    }
}
