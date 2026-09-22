use super::{HashedLine, compute_line_hash};
use crate::handle::Span;

pub(crate) fn source_line_spans(source: &str) -> impl Iterator<Item = Span> + '_ {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start == source.len() {
            return None;
        }

        let end = match source[start..].find(['\r', '\n']) {
            Some(relative) => {
                let offset = start + relative;
                offset
                    + if source[offset..].starts_with("\r\n") {
                        2
                    } else {
                        1
                    }
            }
            None => source.len(),
        };
        let span = Span { start, end };
        start = end;
        Some(span)
    })
}

pub(super) fn show_hashed_lines(source: &str) -> Vec<HashedLine> {
    let mut start = 0;
    split_source_lines(source)
        .lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let end = start + line.content.len() + line.terminator.len();
            let span = crate::handle::Span { start, end };
            start = end;
            HashedLine {
                line: index + 1,
                hash: compute_line_hash(&line.content),
                content: line.content,
                span,
            }
        })
        .collect()
}

pub(super) fn split_set_line_text(text: &str) -> Vec<String> {
    split_multiline_text(text)
}

pub(super) fn split_replace_lines_text(text: &str) -> Vec<String> {
    if text.is_empty() {
        Vec::new()
    } else {
        split_multiline_text(text)
    }
}

pub(super) fn split_multiline_text(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .split('\n')
        .map(ToString::to_string)
        .collect()
}

#[derive(Debug, Clone)]
struct SourceLine {
    content: String,
    terminator: String,
}

#[derive(Debug, Clone)]
pub(super) struct SourceLayout {
    lines: Vec<SourceLine>,
}

pub(super) fn split_source_lines(source: &str) -> SourceLayout {
    let lines = source_line_spans(source)
        .map(|span| {
            let line = &source[span.start..span.end];
            let terminator = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\r') {
                "\r"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            SourceLine {
                content: line[..line.len() - terminator.len()].to_string(),
                terminator: terminator.to_string(),
            }
        })
        .collect();

    SourceLayout { lines }
}

impl SourceLayout {
    pub(super) fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub(super) fn line_content(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(|line| line.content.as_str())
    }

    pub(super) fn replace_range(
        &mut self,
        start_line: usize,
        end_line: usize,
        replacement_contents: Vec<String>,
    ) {
        let start_index = start_line - 1;
        let end_index = end_line;
        let original_line_count = self.lines.len();
        let source_had_trailing_newline = self
            .lines
            .last()
            .is_some_and(|line| !line.terminator.is_empty());
        let final_terminator = self.lines[end_index - 1].terminator.clone();
        let newline = self.preferred_newline(start_index);
        let replacement_count = replacement_contents.len();
        let replacement = replacement_contents
            .into_iter()
            .enumerate()
            .map(|(index, content)| SourceLine {
                content,
                terminator: if index + 1 == replacement_count {
                    final_terminator.clone()
                } else {
                    newline.clone()
                },
            });

        self.lines.splice(start_index..end_index, replacement);

        if replacement_count == 0
            && end_index == original_line_count
            && !source_had_trailing_newline
            && let Some(last_line) = self.lines.last_mut()
        {
            last_line.terminator.clear();
        }
    }

    pub(super) fn insert_after(&mut self, anchor_line: usize, contents: Vec<String>) {
        let anchor_index = anchor_line - 1;
        let original_terminator = self.lines[anchor_index].terminator.clone();
        let newline = self.preferred_newline(anchor_index);
        self.lines[anchor_index].terminator = newline.clone();

        let inserted_count = contents.len();
        let inserted = contents
            .into_iter()
            .enumerate()
            .map(|(index, content)| SourceLine {
                content,
                terminator: if index + 1 == inserted_count {
                    original_terminator.clone()
                } else {
                    newline.clone()
                },
            });
        self.lines
            .splice(anchor_index + 1..anchor_index + 1, inserted);
    }

    pub(super) fn into_content(self) -> String {
        let capacity = self
            .lines
            .iter()
            .map(|line| line.content.len() + line.terminator.len())
            .sum();
        let mut content = String::with_capacity(capacity);
        for line in self.lines {
            content.push_str(&line.content);
            content.push_str(&line.terminator);
        }
        content
    }

    fn preferred_newline(&self, index: usize) -> String {
        self.lines
            .get(index)
            .filter(|line| !line.terminator.is_empty())
            .or_else(|| {
                self.lines[..index]
                    .iter()
                    .rev()
                    .find(|line| !line.terminator.is_empty())
            })
            .or_else(|| {
                self.lines[index.saturating_add(1)..]
                    .iter()
                    .find(|line| !line.terminator.is_empty())
            })
            .map_or_else(|| "\n".to_string(), |line| line.terminator.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::show_hashed_lines;
    use crate::hashline::compute_line_hash;

    #[test]
    fn hashed_line_spans_partition_original_bytes_including_terminators() {
        for source in [
            "",
            "\n",
            "\r\n\r",
            "\u{feff}\u{3b1}\r\n\r\u{d55c}\u{ae00}\nlast",
            "a\n\n",
        ] {
            let lines = show_hashed_lines(source);
            let mut end = 0;
            for (index, line) in lines.iter().enumerate() {
                assert_eq!(line.line, index + 1);
                assert_eq!(line.span.start, end);
                assert!(line.span.end > line.span.start);
                let original = &source[line.span.start..line.span.end];
                assert_eq!(original.trim_end_matches(['\r', '\n']), line.content);
                assert_eq!(line.hash, compute_line_hash(&line.content));
                let wire = serde_json::to_value(line).unwrap();
                assert!(wire.get("span").is_none());
                end = line.span.end;
            }
            assert_eq!(end, source.len());
        }
    }
}
