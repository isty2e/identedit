use std::sync::OnceLock;

use regex::Regex;

use super::boundary::BoundaryKind;
use super::brace_lex::brace_block_end;
use super::javascript_mask::build_candidate_skip_masks;
use super::patterns::fallback_patterns;
use super::python_mask::build_python_multiline_mask;
use super::source_lines::{LineInfo, line_index_for_offset};

pub(super) struct Candidate {
    pub(super) start_line_index: usize,
    pub(super) boundary_line_index: usize,
    pub(super) kind: &'static str,
    pub(super) name: String,
    pub(super) boundary: BoundaryKind,
}
pub(super) fn detect_candidates(source: &[u8], lines: &[LineInfo<'_>]) -> Vec<Candidate> {
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

#[cfg(test)]
mod tests {
    use super::{super::source_lines::collect_lines, build_commonjs_exports_top_level_mask};

    #[test]
    fn commonjs_exports_top_level_mask_marks_exported_property_lines() {
        let source = "module.exports = {\n  parse(value) {\n    return value + 4;\n  },\n  build(value) {\n    return value + 5;\n  },\n};\n";
        let lines = collect_lines(source);
        let mask = build_commonjs_exports_top_level_mask(source.as_bytes(), &lines);

        assert_eq!(mask.len(), lines.len());
        assert!(mask[1], "first exported property line should be marked");
        assert!(mask[4], "second exported property line should be marked");
    }
}
