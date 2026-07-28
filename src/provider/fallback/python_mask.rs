#[derive(Clone, Copy, PartialEq, Eq)]
enum PythonTripleDelimiter {
    SingleQuote,
    DoubleQuote,
}

use super::source_lines::LineInfo;

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

#[cfg(test)]
mod tests {
    use super::{super::source_lines::collect_lines, build_python_multiline_mask};

    #[test]
    fn masks_triple_quoted_bodies_but_not_comment_or_quoted_delimiters() {
        let source = "# ''' ignored\nvalue = \"'''\"\ntext = '''\ndef hidden():\n    pass\n'''\ndef visible():\n    pass\n";
        let lines = collect_lines(source);
        let mask = build_python_multiline_mask(&lines);

        assert!(!mask[0]);
        assert!(!mask[1]);
        assert!(!mask[2]);
        assert!(mask[3]);
        assert!(mask[4]);
        assert!(mask[5]);
        assert!(!mask[6]);
    }
}
