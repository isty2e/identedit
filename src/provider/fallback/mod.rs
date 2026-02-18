use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::provider::StructureProvider;

mod patterns;
mod scanner;

pub struct FallbackProvider;

#[derive(Clone, Copy)]
enum BoundaryKind {
    HeaderLine,
    Indentation,
    Braces,
}

struct Pattern {
    regex: Regex,
    kind: &'static str,
    boundary: BoundaryKind,
    suppress_in_python_multiline: bool,
    requires_commonjs_exports_object_top_level: bool,
}

type LineInfo<'a> = scanner::LineInfo<'a>;

struct Candidate {
    start_line_index: usize,
    boundary_line_index: usize,
    kind: &'static str,
    name: String,
    boundary: BoundaryKind,
}

impl StructureProvider for FallbackProvider {
    fn parse(&self, path: &Path, source: &[u8]) -> Result<Vec<SelectionHandle>, IdenteditError> {
        let source_text =
            std::str::from_utf8(source).map_err(|_| IdenteditError::ParseFailure {
                provider: self.name(),
                message: "Fallback provider requires UTF-8 text input".to_string(),
            })?;
        let lines = collect_lines(source_text);
        let mut handles = Vec::new();

        for candidate in detect_candidates(source_text.as_bytes(), &lines) {
            let start = lines[candidate.start_line_index].start;
            let line_end = lines[candidate.boundary_line_index].end;
            let end = infer_candidate_end(source_text.as_bytes(), &lines, &candidate, line_end);
            if end <= start {
                continue;
            }

            let Some(text) = source_text.get(start..end) else {
                continue;
            };
            if text.trim().is_empty() {
                continue;
            }

            handles.push(SelectionHandle::from_parts(
                path.to_path_buf(),
                Span { start, end },
                candidate.kind.to_string(),
                Some(candidate.name),
                text.to_string(),
            ));
        }

        Ok(handles)
    }

    fn can_handle(&self, _path: &Path) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "fallback"
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &[]
    }
}

fn collect_lines(source_text: &str) -> Vec<LineInfo<'_>> {
    scanner::collect_lines(source_text)
}

fn line_terminator_len_at(source: &[u8], index: usize) -> Option<usize> {
    scanner::line_terminator_len_at(source, index)
}

fn is_line_terminator_byte(source: &[u8], index: usize) -> bool {
    scanner::is_line_terminator_byte(source, index)
}

fn detect_candidates(source: &[u8], lines: &[LineInfo<'_>]) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    let python_multiline_mask = build_python_multiline_mask(lines);
    let (block_comment_mask, template_literal_mask) = build_candidate_skip_masks(lines);
    let commonjs_exports_top_level_mask = build_commonjs_exports_top_level_mask(source, lines);

    for (line_index, line) in lines.iter().enumerate() {
        if block_comment_mask[line_index] || template_literal_mask[line_index] {
            continue;
        }

        for pattern in fallback_patterns() {
            if pattern.suppress_in_python_multiline && python_multiline_mask[line_index] {
                continue;
            }
            if pattern.requires_commonjs_exports_object_top_level
                && !commonjs_exports_top_level_mask[line_index]
            {
                continue;
            }

            let Some(captures) = pattern.regex.captures(line.text) else {
                continue;
            };
            let Some(name_match) = captures.get(1) else {
                continue;
            };

            let name = name_match.as_str().trim();
            if name.is_empty() {
                continue;
            }
            if pattern.requires_commonjs_exports_object_top_level
                && is_disallowed_control_flow_keyword(name)
            {
                continue;
            }

            candidates.push(Candidate {
                start_line_index: line_index,
                boundary_line_index: line_index,
                kind: pattern.kind,
                name: name.to_string(),
                boundary: pattern.boundary,
            });
            break;
        }
    }

    candidates.extend(detect_multiline_python_candidates(
        lines,
        &python_multiline_mask,
        &block_comment_mask,
        &template_literal_mask,
    ));
    candidates.extend(detect_multiline_arrow_candidates(
        lines,
        &python_multiline_mask,
        &block_comment_mask,
        &template_literal_mask,
    ));
    candidates.extend(detect_multiline_js_function_candidates(
        lines,
        &python_multiline_mask,
        &block_comment_mask,
        &template_literal_mask,
    ));
    candidates.sort_by_key(|candidate| (candidate.start_line_index, candidate.boundary_line_index));

    candidates
}

fn detect_multiline_python_candidates(
    lines: &[LineInfo<'_>],
    python_multiline_mask: &[bool],
    block_comment_mask: &[bool],
    template_literal_mask: &[bool],
) -> Vec<Candidate> {
    static PYTHON_CLASS_START_REGEX: OnceLock<Regex> = OnceLock::new();
    static PYTHON_FUNCTION_START_REGEX: OnceLock<Regex> = OnceLock::new();
    static PYTHON_SIGNATURE_TERMINATOR_REGEX: OnceLock<Regex> = OnceLock::new();

    let python_class_start_regex = PYTHON_CLASS_START_REGEX.get_or_init(|| {
        Regex::new(r"^\s*class\s+([\p{L}_][\p{L}\p{M}\p{N}_]*)\b")
            .expect("python multiline class start regex should compile")
    });
    let python_function_start_regex = PYTHON_FUNCTION_START_REGEX.get_or_init(|| {
        Regex::new(r"^\s*(?:async\s+)?def\s+([\p{L}_][\p{L}\p{M}\p{N}_]*)\b")
            .expect("python multiline function start regex should compile")
    });
    let python_signature_terminator_regex = PYTHON_SIGNATURE_TERMINATOR_REGEX.get_or_init(|| {
        Regex::new(r":\s*(?:#.*)?$").expect("python signature terminator regex should compile")
    });

    let mut candidates = Vec::new();
    for (start_line_index, line) in lines.iter().enumerate() {
        if python_multiline_mask[start_line_index]
            || block_comment_mask[start_line_index]
            || template_literal_mask[start_line_index]
        {
            continue;
        }

        let (kind, name) = if let Some(captures) = python_function_start_regex.captures(line.text) {
            ("function_definition", captures.get(1))
        } else if let Some(captures) = python_class_start_regex.captures(line.text) {
            ("class_definition", captures.get(1))
        } else {
            continue;
        };
        let Some(name_match) = name else {
            continue;
        };
        if python_signature_terminator_regex.is_match(line.text) {
            continue;
        }

        let base_indent = line.indent;
        let mut boundary_line_index = None;

        for (line_index, next_line) in lines.iter().enumerate().skip(start_line_index + 1) {
            if python_multiline_mask[line_index]
                || block_comment_mask[line_index]
                || template_literal_mask[line_index]
            {
                continue;
            }
            if next_line.is_blank {
                continue;
            }
            if next_line.indent < base_indent {
                break;
            }
            if next_line.indent == base_indent
                && python_signature_terminator_regex.is_match(next_line.text)
            {
                boundary_line_index = Some(line_index);
                break;
            }
        }

        let Some(boundary_line_index) = boundary_line_index else {
            continue;
        };
        candidates.push(Candidate {
            start_line_index,
            boundary_line_index,
            kind,
            name: name_match.as_str().trim().to_string(),
            boundary: BoundaryKind::Indentation,
        });
    }

    candidates
}

fn detect_multiline_arrow_candidates(
    lines: &[LineInfo<'_>],
    python_multiline_mask: &[bool],
    block_comment_mask: &[bool],
    template_literal_mask: &[bool],
) -> Vec<Candidate> {
    static MULTILINE_ARROW_BINDING_START_REGEX: OnceLock<Regex> = OnceLock::new();

    let multiline_arrow_binding_start_regex =
        MULTILINE_ARROW_BINDING_START_REGEX.get_or_init(|| {
            let js_identifier_start = r"(?:[\p{L}_$]|\\u[0-9A-Fa-f]{4}|\\u\{[0-9A-Fa-f]+\})";
            let js_identifier_continue =
                r"(?:[\p{L}\p{M}\p{N}_$\x{200C}\x{200D}]|\\u[0-9A-Fa-f]{4}|\\u\{[0-9A-Fa-f]+\})";
            let js_identifier = format!(r"(?:{js_identifier_start}{js_identifier_continue}*)");

            Regex::new(&format!(
                r"^\s*(?:export\s+)?(?:const|let|var)\s+({js_identifier})\s*(?::\s*[^\n=]+)?\s*="
            ))
            .expect("multiline arrow binding start regex should compile")
        });

    let mut candidates = Vec::new();
    for (start_line_index, line) in lines.iter().enumerate() {
        if python_multiline_mask[start_line_index]
            || block_comment_mask[start_line_index]
            || template_literal_mask[start_line_index]
        {
            continue;
        }
        let Some(captures) = multiline_arrow_binding_start_regex.captures(line.text) else {
            continue;
        };
        if contains_arrow_after_assignment(line.text) {
            continue;
        }

        let Some(name_match) = captures.get(1) else {
            continue;
        };

        let base_indent = line.indent;
        let mut matched_boundary = None;

        for (line_index, next_line) in lines.iter().enumerate().skip(start_line_index + 1) {
            if python_multiline_mask[line_index]
                || block_comment_mask[line_index]
                || template_literal_mask[line_index]
            {
                continue;
            }
            if next_line.is_blank {
                continue;
            }
            if next_line.indent < base_indent {
                break;
            }
            if next_line.text.contains("=>") {
                let boundary = if next_line.text.contains('{') {
                    BoundaryKind::Braces
                } else {
                    BoundaryKind::HeaderLine
                };
                matched_boundary = Some((line_index, boundary));
                break;
            }
            if next_line.text.contains(';') {
                break;
            }
        }

        let Some((boundary_line_index, boundary)) = matched_boundary else {
            continue;
        };
        candidates.push(Candidate {
            start_line_index,
            boundary_line_index,
            kind: "function_definition",
            name: name_match.as_str().trim().to_string(),
            boundary,
        });
    }

    candidates
}

fn detect_multiline_js_function_candidates(
    lines: &[LineInfo<'_>],
    python_multiline_mask: &[bool],
    block_comment_mask: &[bool],
    template_literal_mask: &[bool],
) -> Vec<Candidate> {
    static MULTILINE_JS_FUNCTION_KEYWORD_ONLY_REGEX: OnceLock<Regex> = OnceLock::new();
    static MULTILINE_JS_FUNCTION_NAME_PARAMS_REGEX: OnceLock<Regex> = OnceLock::new();

    let keyword_only_regex = MULTILINE_JS_FUNCTION_KEYWORD_ONLY_REGEX.get_or_init(|| {
        Regex::new(r"^\s*(?:export\s+(?:default\s+)?)?(?:async\s+)?function\s*\*?\s*$")
            .expect("multiline js function keyword regex should compile")
    });
    let name_params_regex = MULTILINE_JS_FUNCTION_NAME_PARAMS_REGEX.get_or_init(|| {
        let js_identifier_start = r"(?:[\p{L}_$]|\\u[0-9A-Fa-f]{4}|\\u\{[0-9A-Fa-f]+\})";
        let js_identifier_continue =
            r"(?:[\p{L}\p{M}\p{N}_$\x{200C}\x{200D}]|\\u[0-9A-Fa-f]{4}|\\u\{[0-9A-Fa-f]+\})";
        let js_identifier = format!(r"(?:{js_identifier_start}{js_identifier_continue}*)");

        Regex::new(&format!(r"^\s*({js_identifier})(?:\s*<[^(\n]+>)?\s*\("))
            .expect("multiline js function name+params regex should compile")
    });

    let mut candidates = Vec::new();
    for (start_line_index, line) in lines.iter().enumerate() {
        if python_multiline_mask[start_line_index]
            || block_comment_mask[start_line_index]
            || template_literal_mask[start_line_index]
        {
            continue;
        }
        if !keyword_only_regex.is_match(line.text) {
            continue;
        }

        for (line_index, next_line) in lines.iter().enumerate().skip(start_line_index + 1) {
            if python_multiline_mask[line_index]
                || block_comment_mask[line_index]
                || template_literal_mask[line_index]
            {
                continue;
            }
            if next_line.is_blank {
                continue;
            }
            let trimmed = next_line.text.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
                continue;
            }

            let Some(captures) = name_params_regex.captures(next_line.text) else {
                break;
            };
            let Some(name_match) = captures.get(1) else {
                break;
            };
            let name = name_match.as_str().trim();
            if name.is_empty() {
                break;
            }

            candidates.push(Candidate {
                start_line_index,
                boundary_line_index: line_index,
                kind: "function_definition",
                name: name.to_string(),
                boundary: BoundaryKind::Braces,
            });
            break;
        }
    }

    candidates
}

fn contains_arrow_after_assignment(line: &str) -> bool {
    let Some(equals_index) = line.find('=') else {
        return false;
    };
    line[equals_index + 1..].contains("=>")
}

fn build_commonjs_exports_top_level_mask(source: &[u8], lines: &[LineInfo<'_>]) -> Vec<bool> {
    static COMMONJS_EXPORTS_OBJECT_ASSIGNMENT_REGEX: OnceLock<Regex> = OnceLock::new();
    let commonjs_exports_assignment_regex =
        COMMONJS_EXPORTS_OBJECT_ASSIGNMENT_REGEX.get_or_init(|| {
            Regex::new(r"^\s*(?:module\.)?exports\s*=\s*\{")
                .expect("commonjs exports object assignment regex should compile")
        });

    let mut mask = vec![false; lines.len()];
    for (start_line_index, start_line) in lines.iter().enumerate() {
        if !commonjs_exports_assignment_regex.is_match(start_line.text) {
            continue;
        }

        let Some(block_end) = brace_block_end(source, start_line.start, start_line.end) else {
            continue;
        };
        let Some(end_line_index) = line_index_for_offset(lines, block_end.saturating_sub(1)) else {
            continue;
        };

        let mut top_level_property_indent: Option<usize> = None;
        for line_index in start_line_index + 1..=end_line_index {
            let line = &lines[line_index];
            if line.is_blank {
                continue;
            }

            let trimmed = line.text.trim_start();
            if line.indent <= start_line.indent {
                if trimmed.starts_with('}') {
                    break;
                }
                continue;
            }
            let should_mark = match top_level_property_indent {
                None => {
                    top_level_property_indent = Some(line.indent);
                    true
                }
                Some(indent) if line.indent == indent => true,
                Some(indent) => {
                    previous_significant_commonjs_line_index(lines, start_line_index, line_index)
                        .is_some_and(|previous_index| {
                            let previous_line = &lines[previous_index];
                            previous_line.indent <= indent
                                && is_commonjs_property_boundary_line(previous_line.text)
                        })
                }
            };

            if should_mark {
                mask[line_index] = true;
            }
        }
    }

    mask
}

fn previous_significant_commonjs_line_index(
    lines: &[LineInfo<'_>],
    start_line_index: usize,
    current_line_index: usize,
) -> Option<usize> {
    let mut line_index = current_line_index;
    while line_index > start_line_index {
        line_index -= 1;
        let line = &lines[line_index];
        if line.is_blank {
            continue;
        }
        let trimmed = line.text.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }
        return Some(line_index);
    }

    None
}

fn is_commonjs_property_boundary_line(line_text: &str) -> bool {
    let trimmed = line_text.trim();
    trimmed.ends_with(',') || trimmed == "}" || trimmed == "};" || trimmed == "},"
}

fn line_index_for_offset(lines: &[LineInfo<'_>], offset: usize) -> Option<usize> {
    scanner::line_index_for_offset(lines, offset)
}

fn is_disallowed_control_flow_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "else"
            | "switch"
            | "case"
            | "for"
            | "while"
            | "do"
            | "try"
            | "catch"
            | "finally"
            | "return"
            | "throw"
            | "break"
            | "continue"
    )
}

fn build_candidate_skip_masks(lines: &[LineInfo<'_>]) -> (Vec<bool>, Vec<bool>) {
    scanner::build_candidate_skip_masks(lines)
}

fn build_python_multiline_mask(lines: &[LineInfo<'_>]) -> Vec<bool> {
    scanner::build_python_multiline_mask(lines)
}

fn infer_candidate_end(
    source: &[u8],
    lines: &[LineInfo<'_>],
    candidate: &Candidate,
    line_end: usize,
) -> usize {
    match candidate.boundary {
        BoundaryKind::HeaderLine => line_end,
        BoundaryKind::Indentation => {
            indentation_block_end(lines, candidate.boundary_line_index).unwrap_or(line_end)
        }
        BoundaryKind::Braces => {
            brace_block_end(source, lines[candidate.boundary_line_index].start, line_end)
                .unwrap_or(line_end)
        }
    }
}

fn indentation_block_end(lines: &[LineInfo<'_>], start_line: usize) -> Option<usize> {
    let base_indent = lines[start_line].indent;
    let mut seen_body_line = false;
    let mut end = lines[start_line].end;

    for line in &lines[start_line + 1..] {
        if line.is_blank {
            if seen_body_line {
                end = line.end;
            }
            continue;
        }

        if line.indent > base_indent {
            seen_body_line = true;
            end = line.end;
            continue;
        }

        break;
    }

    if seen_body_line { Some(end) } else { None }
}

fn brace_block_end(source: &[u8], start: usize, header_end: usize) -> Option<usize> {
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

fn should_start_regex_literal(source: &[u8], start: usize, slash_index: usize) -> bool {
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

fn fallback_patterns() -> &'static [Pattern] {
    patterns::fallback_patterns()
}

#[cfg(test)]
mod tests;
