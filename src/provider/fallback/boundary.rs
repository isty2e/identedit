use super::brace_lex::brace_block_end;
use super::source_lines::LineInfo;

#[derive(Clone, Copy)]
pub(super) enum BoundaryKind {
    HeaderLine,
    Indentation,
    Braces,
}

pub(super) fn infer_candidate_end(
    source: &[u8],
    lines: &[LineInfo<'_>],
    boundary_line_index: usize,
    boundary: BoundaryKind,
    line_end: usize,
) -> usize {
    match boundary {
        BoundaryKind::HeaderLine => line_end,
        BoundaryKind::Indentation => {
            indentation_block_end(lines, boundary_line_index).unwrap_or(line_end)
        }
        BoundaryKind::Braces => {
            brace_block_end(source, lines[boundary_line_index].start, line_end).unwrap_or(line_end)
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

#[cfg(test)]
mod tests {
    use super::{super::source_lines::collect_lines, indentation_block_end};

    #[test]
    fn indentation_boundary_includes_trailing_blank_lines_after_a_body() {
        let source = "def run():\n    return 1\n\nnext_value = 2\n";
        let lines = collect_lines(source);
        let end = indentation_block_end(&lines, 0).expect("function body should be found");

        assert_eq!(&source[..end], "def run():\n    return 1\n\n");
    }
}
