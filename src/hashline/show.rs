use super::{HashedLine, compute_line_hash};

pub(super) fn show_hashed_lines(source: &str) -> Vec<HashedLine> {
    split_source_lines(source)
        .into_line_contents()
        .into_iter()
        .enumerate()
        .map(|(index, content)| HashedLine {
            line: index + 1,
            hash: compute_line_hash(&content),
            content,
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
    if source.is_empty() {
        return SourceLayout { lines: Vec::new() };
    }

    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines.push(SourceLine {
                    content: source[start..index].to_string(),
                    terminator: "\n".to_string(),
                });
                index += 1;
                start = index;
            }
            b'\r' => {
                let terminator = if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    index += 2;
                    "\r\n"
                } else {
                    index += 1;
                    "\r"
                };
                lines.push(SourceLine {
                    content: source[start..index - terminator.len()].to_string(),
                    terminator: terminator.to_string(),
                });
                start = index;
            }
            _ => {
                index += 1;
            }
        }
    }

    if start < source.len() {
        lines.push(SourceLine {
            content: source[start..].to_string(),
            terminator: String::new(),
        });
    }

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

    fn into_line_contents(self) -> Vec<String> {
        self.lines.into_iter().map(|line| line.content).collect()
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
