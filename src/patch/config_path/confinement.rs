use std::collections::HashMap;
use std::ops::Range;

use tree_sitter::{Node, Tree};

use crate::error::IdenteditError;

use super::composition::ConfigWrite;

#[derive(Clone, Copy)]
pub(super) enum ValueEdit<'tree> {
    Set {
        owner: Node<'tree>,
    },
    Append {
        sequence: Node<'tree>,
        value_start: usize,
    },
}

impl<'tree> ValueEdit<'tree> {
    pub(super) fn toml_set(slot: Node<'tree>) -> Result<Self, IdenteditError> {
        let owner = slot
            .parent()
            .filter(|owner| matches!(owner.kind(), "pair" | "array") && is_value_child(slot))
            .ok_or_else(invalid_value_edit)?;
        Ok(Self::Set { owner })
    }

    fn owner(self) -> Node<'tree> {
        match self {
            Self::Set { owner } => owner,
            Self::Append { sequence, .. } => sequence,
        }
    }
}

pub(super) struct RenderedWrite<'tree> {
    original: Range<usize>,
    output: Range<usize>,
    value_edit: Option<ValueEdit<'tree>>,
}

impl<'tree> RenderedWrite<'tree> {
    pub(super) fn new(
        write: &ConfigWrite,
        start: usize,
        value_edit: Option<ValueEdit<'tree>>,
    ) -> Self {
        Self {
            original: write.span.start..write.span.end,
            output: start..(start + write.new_text.len()),
            value_edit,
        }
    }

    fn has_one_value(&self, edit: ValueEdit<'_>, owner: Node<'_>, output: &Projection<'_>) -> bool {
        let (payload, value) = match edit {
            ValueEdit::Set { .. } => {
                let payload = self.output.clone();
                let mut cursor = owner.walk();
                let mut children = owner
                    .named_children(&mut cursor)
                    .filter(|node| is_value_child(*node));
                let value = if owner.kind() == "pair" {
                    children.last()
                } else {
                    children.find(|node| {
                        let span = trim_range(output.bytes, node.byte_range());
                        !span.is_empty() && contains(&payload, &span)
                    })
                };
                (payload, value)
            }
            ValueEdit::Append { value_start, .. } => {
                let mut cursor = owner.walk();
                let last = owner
                    .named_children(&mut cursor)
                    .filter(|node| is_value_child(*node))
                    .last();
                let value = if owner.kind() == "block_sequence" {
                    // The inserted dash belongs to the renderer, not the supplied value.
                    last.and_then(|item| {
                        let mut cursor = item.walk();
                        item.named_children(&mut cursor)
                            .find(|node| matches!(node.kind(), "block_node" | "flow_node"))
                    })
                } else {
                    last
                };
                ((self.output.start + value_start)..self.output.end, value)
            }
        };
        let Some(value) = value else {
            return false;
        };
        let extent = trim_range(output.bytes, value.byte_range());
        !extent.is_empty()
            && contains(&payload, &extent)
            && output.only_trivia(payload.start..extent.start)
            && output.only_trivia(extent.end..payload.end)
    }
}

fn is_value_child(node: Node<'_>) -> bool {
    !node.is_extra() && node.kind() != "comment"
}

fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn trim_range(bytes: &[u8], range: Range<usize>) -> Range<usize> {
    let raw = &bytes[range.clone()];
    let trimmed_start = raw.trim_ascii_start();
    let start = range.end - trimmed_start.len();
    start..start + trimmed_start.trim_ascii_end().len()
}

fn invalid_value_edit() -> IdenteditError {
    IdenteditError::InvalidRequest {
        message: "Config path edit must change exactly one value at the selected location without changing unrelated config structure".to_string(),
    }
}

pub(super) fn validate_value_edits(
    original_tree: &Tree,
    original: &str,
    output_tree: &Tree,
    output: &str,
    writes: &[RenderedWrite<'_>],
) -> Result<(), IdenteditError> {
    let original = Projection {
        root: original_tree.root_node(),
        bytes: original.as_bytes(),
        writes,
        is_output: false,
    };
    let output = Projection {
        root: output_tree.root_node(),
        bytes: output.as_bytes(),
        writes,
        is_output: true,
    };

    let mut checks = HashMap::<_, Vec<_>>::new();
    for write in writes {
        if let Some(edit) = write.value_edit {
            checks
                .entry(edit.owner().id())
                .or_default()
                .push((write, edit));
        }
    }

    // Compare surviving syntax in order. A write covers its own nodes, not the
    // enclosing guard. In particular, unchanged siblings keep their source
    // addresses even when a deletion shifts their array indices.
    let mut pairs = vec![(original.root, output.root)];
    let mut original_comments = Vec::new();
    let mut output_comments = Vec::new();
    while let Some((before, after)) = pairs.pop() {
        if let Some(requests) = checks.remove(&before.id())
            && !requests
                .into_iter()
                .all(|(write, edit)| write.has_one_value(edit, after, &output))
        {
            return Err(invalid_value_edit());
        }
        if before.kind_id() != after.kind_id() {
            return Err(invalid_value_edit());
        }
        let before_text = &original.bytes[before.byte_range()];
        let after_text = &output.bytes[after.byte_range()];
        if before.kind() == "block_scalar" {
            if before_text != after_text
                || block_scalar_ending(before, original.bytes)
                    != block_scalar_ending(after, output.bytes)
            {
                return Err(invalid_value_edit());
            }
        } else if before.child_count() == 0 && before_text != after_text {
            return Err(invalid_value_edit());
        }

        let mut before_cursor = before.walk();
        let mut after_cursor = after.walk();
        let mut before_children =
            original.children(before, &mut before_cursor, &mut original_comments);
        let mut after_children = output.children(after, &mut after_cursor, &mut output_comments);
        loop {
            match (before_children.next(), after_children.next()) {
                (Some((before, a, field_a)), Some((after, b, field_b)))
                    if a == b && field_a == field_b =>
                {
                    pairs.push((before, after));
                }
                (None, None) => break,
                _ => return Err(invalid_value_edit()),
            }
        }
    }

    // A comment must still be a comment, but need not be assigned to the same
    // parent by the grammar when adjacent values move or gain delimiters.
    original_comments.sort_unstable();
    output_comments.sort_unstable();
    if !checks.is_empty() || original_comments != output_comments {
        return Err(invalid_value_edit());
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
struct BlockScalarEnding<'source> {
    last_line_spaces: &'source [u8],
    final_break: bool,
    kept_blank_lines: &'source [u8],
}

fn block_scalar_ending<'source>(
    node: Node<'_>,
    source: &'source [u8],
) -> BlockScalarEnding<'source> {
    // The grammar can end the node before trailing spaces and line breaks
    // whose interpretation is still controlled by the scalar header.
    let text = &source[node.byte_range()];
    let header_end = text.iter().position(|&byte| is_break(byte));
    let header = text
        .split(|byte| byte.is_ascii_whitespace() || *byte == b'#')
        .next()
        .unwrap_or_default();
    let has_content =
        header_end.is_some_and(|end| text[end..].iter().any(|byte| !byte.is_ascii_whitespace()));

    let following = if has_content {
        &source[node.start_byte() + text.trim_ascii_end().len()..]
    } else {
        // An empty scalar's header (possibly with a comment) can end outside
        // the node. Its own line break is not a line of the value.
        after_line_break(&source[node.start_byte()..])
    };
    let whitespace_end = following
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        .unwrap_or(following.len());
    let whitespace = &following[..whitespace_end];
    let first_break = whitespace.iter().position(|&byte| is_break(byte));
    let blank_lines = if has_content {
        after_line_break(whitespace)
    } else {
        whitespace
    };
    let kept_blank_lines = if header.contains(&b'+') {
        if whitespace_end == following.len() {
            blank_lines
        } else {
            let end = blank_lines
                .iter()
                .rposition(|&byte| is_break(byte))
                .map_or(0, |index| index + 1);
            &blank_lines[..end]
        }
    } else {
        &[]
    };
    BlockScalarEnding {
        last_line_spaces: if has_content {
            &whitespace[..first_break.unwrap_or(whitespace.len())]
        } else {
            &[]
        },
        final_break: has_content && first_break.is_some() && !header.contains(&b'-'),
        kept_blank_lines,
    }
}

fn is_break(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r')
}

fn after_line_break(bytes: &[u8]) -> &[u8] {
    let Some(index) = bytes.iter().position(|&byte| is_break(byte)) else {
        return &[];
    };
    let length = if bytes[index..].starts_with(b"\r\n") {
        2
    } else {
        1
    };
    &bytes[index + length..]
}

struct Projection<'a> {
    root: Node<'a>,
    bytes: &'a [u8],
    writes: &'a [RenderedWrite<'a>],
    is_output: bool,
}

type Comment<'a> = ((usize, usize), &'a [u8]);

impl<'a> Projection<'a> {
    fn write_range<'w>(&self, write: &'w RenderedWrite<'_>) -> &'w Range<usize> {
        if self.is_output {
            &write.output
        } else {
            &write.original
        }
    }

    fn retained_edge(&self, mut range: Range<usize>, first: bool) -> Option<usize> {
        while !range.is_empty() {
            let byte = if first { range.start } else { range.end - 1 };
            let previous = self
                .writes
                .partition_point(|write| self.write_range(write).start <= byte);
            let write = previous.checked_sub(1).map(|index| &self.writes[index]);
            if let Some(region) = write
                .map(|write| self.write_range(write))
                .filter(|span| span.contains(&byte))
            {
                if first {
                    range.start = region.end.min(range.end);
                } else {
                    range.end = region.start.max(range.start);
                }
                continue;
            }

            // Container spans can gain/lose delimiter-adjacent whitespace.
            // Terminal text is compared separately without this trimming.
            if !self.bytes[byte].is_ascii_whitespace() {
                return Some(match (self.is_output, write) {
                    (true, Some(write)) => write.original.end + (byte - write.output.end),
                    _ => byte,
                });
            }
            if first {
                range.start += 1;
            } else {
                range.end -= 1;
            }
        }
        None
    }

    fn children<'b>(
        &'b self,
        parent: Node<'a>,
        cursor: &'b mut tree_sitter::TreeCursor<'a>,
        comments: &'b mut Vec<Comment<'a>>,
    ) -> impl Iterator<Item = (Node<'a>, (usize, usize), Option<&'static str>)> + 'b {
        parent
            .children(cursor)
            .enumerate()
            .filter_map(move |(index, node)| {
                let start = self.retained_edge(node.byte_range(), true)?;
                let end = self.retained_edge(node.byte_range(), false)?;
                if node.kind() == "comment" {
                    comments.push(((start, end), &self.bytes[node.byte_range()]));
                    return None;
                }
                Some((
                    node,
                    (start, end),
                    parent.field_name_for_child(index as u32),
                ))
            })
    }

    fn only_trivia(&self, mut span: Range<usize>) -> bool {
        loop {
            span = trim_range(self.bytes, span);
            if span.is_empty() {
                return true;
            }
            let Some(comment) = self
                .root
                .descendant_for_byte_range(span.start, span.start + 1)
            else {
                return false;
            };
            if comment.kind() != "comment"
                || comment.start_byte() != span.start
                || comment.end_byte() > span.end
            {
                return false;
            }
            span.start = comment.end_byte();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::handle::Span;

    use super::super::safety::{ConfigFormat, parse_tree_for_format};
    use super::*;

    #[test]
    fn config_value_must_belong_to_the_selected_sequence() {
        let original = r#"{"first":[],"second":[]}"#;
        let output = r#"{"first":[],"second":[true]}"#;
        let original_tree =
            parse_tree_for_format(&ConfigFormat::Json, original.as_bytes()).unwrap();
        let output_tree = parse_tree_for_format(&ConfigFormat::Json, output.as_bytes()).unwrap();
        let object = original_tree.root_node().named_child(0).unwrap();
        let start = original.rfind(']').unwrap();
        let write = ConfigWrite {
            span: Span { start, end: start },
            new_text: "true".to_string(),
        };

        for (index, valid) in [(0, false), (1, true)] {
            let sequence = object
                .named_child(index)
                .unwrap()
                .child_by_field_name("value")
                .unwrap();
            let writes = [RenderedWrite::new(
                &write,
                start,
                Some(ValueEdit::Append {
                    sequence,
                    value_start: 0,
                }),
            )];
            let result =
                validate_value_edits(&original_tree, original, &output_tree, output, &writes);
            if valid {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(IdenteditError::InvalidRequest { .. })));
            }
        }
    }

    #[test]
    fn config_append_requires_one_direct_value_in_the_rendered_result() {
        for (format, kind, original, nested_value, extra_element) in [
            (
                ConfigFormat::Json,
                "array",
                r#"{"items":[0]}"#,
                r#"{"items":[0,[1,2]]}"#,
                r#"{"items":[0,1,2]}"#,
            ),
            (
                ConfigFormat::Toml,
                "array",
                "items = [0]\n",
                "items = [0, [1, 2]]\n",
                "items = [0, 1, 2]\n",
            ),
            (
                ConfigFormat::Yaml,
                "flow_sequence",
                "items: [0]\n",
                "items: [0, [1, 2]]\n",
                "items: [0, 1, 2]\n",
            ),
            (
                ConfigFormat::Yaml,
                "block_sequence",
                "items:\n  - 0\n",
                "items:\n  - 0\n  - [1, 2]\n",
                "items:\n  - 0\n  - 1\n  - 2\n",
            ),
        ] {
            let before = parse_tree_for_format(&format, original.as_bytes()).unwrap();
            let anchor = original.find('0').unwrap();
            let sequence = enclosing_sequence(&before, anchor, kind);
            let start = sequence.named_child(0).unwrap().end_byte();

            for (output, valid) in [(nested_value, true), (extra_element, false)] {
                let after = parse_tree_for_format(&format, output.as_bytes()).unwrap();
                let updated = enclosing_sequence(&after, anchor, kind);
                let mut first_new = updated.named_child(1).unwrap();
                if kind == "block_sequence" {
                    first_new = first_new.named_child(0).unwrap();
                }
                let original_tail = original.len() - start;
                assert_eq!(&original[start..], &output[output.len() - original_tail..]);
                let writes = [RenderedWrite {
                    original: start..start,
                    output: start..output.len() - original_tail,
                    value_edit: Some(ValueEdit::Append {
                        sequence,
                        value_start: first_new.start_byte() - start,
                    }),
                }];

                let result = validate_value_edits(&before, original, &after, output, &writes);
                if valid {
                    result.unwrap();
                } else {
                    assert!(
                        matches!(result, Err(IdenteditError::InvalidRequest { .. })),
                        "{format:?} {kind}"
                    );
                }
            }
        }
    }

    #[test]
    fn config_toml_set_rejects_a_rendered_change_outside_the_value_slot() {
        for (original, output) in [
            (
                "value = 0\nkeep = true\n",
                "value = 1\nextra = 2\nkeep = true\n",
            ),
            ("items = [0, 3]\n", "items = [1, 2, 3]\n"),
        ] {
            let before = parse_tree_for_format(&ConfigFormat::Toml, original.as_bytes()).unwrap();
            let after = parse_tree_for_format(&ConfigFormat::Toml, output.as_bytes()).unwrap();
            let start = original.find('0').unwrap();
            let end = start + 1;
            let slot = before
                .root_node()
                .named_descendant_for_byte_range(start, end)
                .unwrap();
            let writes = [RenderedWrite {
                original: start..end,
                output: start..(output.len() - (original.len() - end)),
                value_edit: Some(ValueEdit::toml_set(slot).unwrap()),
            }];

            assert!(matches!(
                validate_value_edits(&before, original, &after, output, &writes),
                Err(IdenteditError::InvalidRequest { .. })
            ));
        }
    }

    fn enclosing_sequence<'tree>(tree: &'tree Tree, offset: usize, kind: &str) -> Node<'tree> {
        let mut node = tree
            .root_node()
            .descendant_for_byte_range(offset, offset + 1)
            .unwrap();
        while node.kind() != kind {
            node = node.parent().expect("fixture value belongs to a sequence");
        }
        node
    }
}
