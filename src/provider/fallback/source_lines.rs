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

#[cfg(test)]
mod tests {
    use super::{collect_lines, line_index_for_offset};

    #[test]
    fn collects_mixed_line_terminators_with_byte_accurate_offsets() {
        let source = "alpha\r\nbeta\rgamma\u{2028}delta\u{2029}epsilon";
        let lines = collect_lines(source);

        assert_eq!(
            lines.iter().map(|line| line.text).collect::<Vec<_>>(),
            ["alpha", "beta", "gamma", "delta", "epsilon"]
        );
        for (line_index, line) in lines.iter().enumerate() {
            assert_eq!(line_index_for_offset(&lines, line.start), Some(line_index));
            assert_eq!(
                line_index_for_offset(&lines, line.end.saturating_sub(1)),
                Some(line_index)
            );
        }
    }
}
