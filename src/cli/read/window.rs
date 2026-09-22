use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::IdenteditError;
use crate::handle::Span;
use crate::hashline::{HashedLine, LineAnchor, show_hashed_lines};

use super::{ReadHandle, ReadResponse};

#[derive(Debug, Serialize)]
pub(super) struct ReadWindow {
    file: PathBuf,
    total_lines: usize,
    start_line: Option<usize>,
    end_line: Option<usize>,
    omitted_before: usize,
    omitted_after: usize,
    #[serde(flatten)]
    view: WindowView,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WindowView {
    Line,
    Symbol {
        target_lines: LineBounds,
        target_start_column: usize,
        target_end_column: usize,
        lines: Vec<ContextLine>,
    },
}

#[derive(Debug, Serialize)]
struct LineBounds {
    start: usize,
    end: usize,
}

#[derive(Debug, Serialize)]
struct ContextLine {
    line: usize,
    anchor: LineAnchor,
    text: String,
}

impl ReadWindow {
    pub(super) fn page(
        file: PathBuf,
        lines: Vec<HashedLine>,
        offset: usize,
        limit: Option<usize>,
    ) -> (Self, Vec<HashedLine>) {
        let total = lines.len();
        let start = offset.saturating_sub(1).min(total);
        let count = limit.unwrap_or(total).min(total - start);
        let window = Self::new(file, total, start, start + count, WindowView::Line);
        let selected = lines.into_iter().skip(start).take(count).collect();
        (window, selected)
    }

    pub(super) fn symbol(
        file: PathBuf,
        source: &str,
        target: Span,
        context: usize,
    ) -> Result<Self, IdenteditError> {
        if target.start >= target.end || source.get(target.start..target.end).is_none() {
            return Err(IdenteditError::InvalidRequest {
                message: "Cannot render symbol context without a nonempty valid source span"
                    .to_string(),
            });
        }

        let lines = show_hashed_lines(source);
        let total = lines.len();
        let first = lines.partition_point(|line| line.span.end <= target.start);
        let end = lines.partition_point(|line| line.span.start < target.end);
        let target_start_column = target.start - lines[first].span.start + 1;
        let target_end_column = target.end - lines[end - 1].span.start + 1;
        let start = first.saturating_sub(context);
        let stop = end.saturating_add(context).min(total);
        let selected = lines
            .into_iter()
            .skip(start)
            .take(stop - start)
            .map(|line| ContextLine {
                line: line.line,
                anchor: LineAnchor::try_new(line.line, line.hash)
                    .expect("source line numbers are positive"),
                text: line.content,
            })
            .collect();

        Ok(Self::new(
            file,
            total,
            start,
            stop,
            WindowView::Symbol {
                target_lines: LineBounds {
                    start: first + 1,
                    end,
                },
                target_start_column,
                target_end_column,
                lines: selected,
            },
        ))
    }

    fn new(file: PathBuf, total_lines: usize, start: usize, end: usize, view: WindowView) -> Self {
        Self {
            file,
            total_lines,
            start_line: (start < end).then_some(start + 1),
            end_line: (start < end).then_some(end),
            omitted_before: start,
            omitted_after: total_lines - end,
            view,
        }
    }

    fn heading(&self) -> String {
        let range = match (self.start_line, self.end_line) {
            (Some(start), Some(end)) => format!("{start}-{end}"),
            _ => "none".to_string(),
        };
        format!(
            "# window lines {range} of {}; omitted before={} after={}",
            self.total_lines, self.omitted_before, self.omitted_after
        )
    }
}

pub(super) fn render_windows(response: &ReadResponse) -> String {
    let mut grouped = BTreeMap::<&Path, Vec<&ReadHandle>>::new();
    for handle in &response.handles {
        let file = match handle {
            ReadHandle::Line { file, .. } | ReadHandle::Node { file, .. } => file.as_path(),
        };
        grouped.entry(file).or_default().push(handle);
    }

    response
        .windows
        .iter()
        .map(|window| {
            let mut output = vec![format!("## {}", window.file.display())];
            let handles = grouped.get(window.file.as_path()).map(Vec::as_slice).unwrap_or_default();
            match &window.view {
                WindowView::Line => {
                    output.push(window.heading());
                    for handle in handles {
                        if let ReadHandle::Line {
                            anchor, text, ..
                        } = handle
                        {
                            output.push(format!("{anchor}|{text}"));
                        }
                    }
                }
                WindowView::Symbol {
                    target_lines,
                    target_start_column,
                    target_end_column,
                    lines,
                } => {
                    for handle in handles {
                        if let ReadHandle::Node {
                            span,
                            kind,
                            name,
                            identity,
                            ..
                        } = handle
                        {
                            output.push(format!(
                                "# node --at {identity} | {kind} {} | bytes [{}..{})",
                                name.as_deref().unwrap_or("-"),
                                span.start,
                                span.end
                            ));
                        }
                    }
                    output.push(format!("# node extent: {}:{}..{}:{} (1-based byte columns; end exclusive)", target_lines.start, target_start_column, target_lines.end, target_end_column));
                    output.push("# raw replacement: exclude context and the first-line prefix; keep indentation on later lines".to_string());
                    output.push(window.heading());
                    let mut previous_target = None;
                    for line in lines {
                        let is_target =
                            (target_lines.start..=target_lines.end).contains(&line.line);
                        if previous_target != Some(is_target) {
                            output.push(if is_target {
                                format!(
                                    "# target lines {}-{} (node extent is the byte span above)",
                                    target_lines.start, target_lines.end
                                )
                            } else {
                                "# context".to_string()
                            });
                            previous_target = Some(is_target);
                        }
                        output.push(format!("{}|{}", line.anchor, line.text));
                    }
                }
            }
            output.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::{ReadWindow, Span, WindowView};

    #[test]
    fn symbol_window_respects_end_exclusive_spans_at_line_boundaries() {
        for source in [
            "\u{3b1}\nbody\nafter",
            "\u{3b1}\r\nbody\r\nafter",
            "\u{3b1}\rbody\rafter",
        ] {
            let start = source.find("body").unwrap();
            let end = source.find("after").unwrap();
            let window = ReadWindow::symbol("file".into(), source, Span { start, end }, 0).unwrap();
            assert_eq!(window.start_line, Some(2));
            assert_eq!(window.end_line, Some(2));
            assert_eq!(window.omitted_before, 1);
            assert_eq!(window.omitted_after, 1);
            let WindowView::Symbol {
                target_lines,
                lines,
                ..
            } = window.view
            else {
                panic!("expected symbol window")
            };
            assert_eq!(target_lines.start, 2);
            assert_eq!(target_lines.end, 2);
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0].text, "body");
        }
    }

    #[test]
    fn symbol_window_columns_count_bytes_not_unicode_characters() {
        let window = ReadWindow::symbol(
            "file".into(),
            "\u{3b1}\u{3b2} target suffix",
            Span { start: 5, end: 11 },
            0,
        )
        .unwrap();
        let WindowView::Symbol {
            target_start_column,
            target_end_column,
            ..
        } = window.view
        else {
            panic!("expected symbol window")
        };
        assert_eq!(target_start_column, 6);
        assert_eq!(target_end_column, 12);
    }

    #[test]
    fn symbol_window_rejects_invalid_provider_spans() {
        for span in [
            Span { start: 0, end: 0 },
            Span { start: 3, end: 2 },
            Span { start: 1, end: 2 },
            Span {
                start: 0,
                end: usize::MAX,
            },
        ] {
            assert!(ReadWindow::symbol("file".into(), "\u{3b1}x", span, 0).is_err());
        }
    }
}
