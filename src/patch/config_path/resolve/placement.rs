pub(super) fn line_start_before_offset(source_text: &str, offset: usize) -> usize {
    let bytes = source_text.as_bytes();
    let mut cursor = offset.min(bytes.len());
    while cursor > 0 && bytes[cursor - 1] != b'\n' && bytes[cursor - 1] != b'\r' {
        cursor -= 1;
    }
    cursor
}

pub(super) fn line_end_with_ending_after_offset(source_text: &str, offset: usize) -> usize {
    let bytes = source_text.as_bytes();
    let mut cursor = line_end_after_offset(source_text, offset);
    if cursor < bytes.len() {
        if bytes[cursor] == b'\r' && cursor + 1 < bytes.len() && bytes[cursor + 1] == b'\n' {
            cursor += 2;
        } else {
            cursor += 1;
        }
    }
    cursor
}

pub(super) fn line_end_after_offset(source_text: &str, offset: usize) -> usize {
    let bytes = source_text.as_bytes();
    let mut cursor = offset.min(bytes.len());
    while cursor < bytes.len() && bytes[cursor] != b'\n' && bytes[cursor] != b'\r' {
        cursor += 1;
    }
    cursor
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SiblingEntry {
    pub(super) key: String,
    pub(super) insertion_start: usize,
    pub(super) end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SiblingGroup {
    start: usize,
    end: usize,
}

pub(super) fn group_aware_insertion_offset(
    source_text: &str,
    mut entries: Vec<SiblingEntry>,
    fallback: usize,
    new_key: &str,
) -> usize {
    if entries.is_empty() {
        return fallback;
    }

    entries.sort_by_key(|entry| entry.insertion_start);
    let groups = sibling_groups(source_text, &entries);
    if let Some(offset) = prefix_family_insertion_offset(&entries, &groups, new_key) {
        return offset;
    }
    if let Some(offset) = sorted_group_insertion_offset(&entries, &groups, new_key) {
        return offset;
    }
    fallback
}

fn sibling_groups(source_text: &str, entries: &[SiblingEntry]) -> Vec<SiblingGroup> {
    let mut groups = Vec::new();
    let mut start = 0usize;
    for index in 1..entries.len() {
        if has_blank_line_between(
            source_text,
            entries[index - 1].end,
            entries[index].insertion_start,
        ) {
            groups.push(SiblingGroup { start, end: index });
            start = index;
        }
    }
    groups.push(SiblingGroup {
        start,
        end: entries.len(),
    });
    groups
}

fn has_blank_line_between(source_text: &str, start: usize, end: usize) -> bool {
    if start >= end || end > source_text.len() {
        return false;
    }
    source_text[start..end]
        .lines()
        .any(|line| line.trim().is_empty())
}

fn prefix_family_insertion_offset(
    entries: &[SiblingEntry],
    groups: &[SiblingGroup],
    new_key: &str,
) -> Option<usize> {
    let family = key_family(new_key)?;
    for group in groups {
        let mut index = group.start;
        while index < group.end {
            if key_family(&entries[index].key) != Some(family) {
                index += 1;
                continue;
            }

            let run_start = index;
            while index < group.end && key_family(&entries[index].key) == Some(family) {
                index += 1;
            }
            let run_end = index;
            if run_end - run_start < 2 {
                continue;
            }

            if entries[run_start..run_end]
                .windows(2)
                .all(|window| window[0].key <= window[1].key)
            {
                for entry in &entries[run_start..run_end] {
                    if new_key < entry.key.as_str() {
                        return Some(entry.insertion_start);
                    }
                }
            }
            return Some(entries[run_end - 1].end);
        }
    }
    None
}

fn key_family(key: &str) -> Option<&str> {
    let (index, _) = key
        .char_indices()
        .rev()
        .find(|(_, character)| matches!(character, '_' | '-'))?;
    if index == 0 || index + 1 >= key.len() {
        return None;
    }
    Some(&key[..index])
}

fn sorted_group_insertion_offset(
    entries: &[SiblingEntry],
    groups: &[SiblingGroup],
    new_key: &str,
) -> Option<usize> {
    if groups.len() == 1 {
        let group = groups[0];
        if group.end - group.start >= 3 && group_is_sorted(entries, group) {
            return Some(insertion_offset_in_sorted_group(entries, group, new_key));
        }
        return None;
    }

    for (group_index, group) in groups.iter().copied().enumerate() {
        if group.end - group.start < 2 || !group_is_sorted(entries, group) {
            continue;
        }
        let lower_ok =
            group_index == 0 || new_key > entries[groups[group_index - 1].end - 1].key.as_str();
        let upper_ok = group_index + 1 == groups.len()
            || new_key < entries[groups[group_index + 1].start].key.as_str();
        if lower_ok && upper_ok {
            return Some(insertion_offset_in_sorted_group(entries, group, new_key));
        }
    }

    None
}

fn group_is_sorted(entries: &[SiblingEntry], group: SiblingGroup) -> bool {
    entries[group.start..group.end]
        .windows(2)
        .all(|window| window[0].key <= window[1].key)
}

fn insertion_offset_in_sorted_group(
    entries: &[SiblingEntry],
    group: SiblingGroup,
    new_key: &str,
) -> usize {
    for entry in &entries[group.start..group.end] {
        if new_key < entry.key.as_str() {
            return entry.insertion_start;
        }
    }
    entries[group.end - 1].end
}

pub(super) fn leading_comment_block_start(
    source_text: &str,
    key_line_start: usize,
    min_indent: usize,
) -> usize {
    let mut cursor = key_line_start;
    let mut start = key_line_start;
    while let Some((line_start, line_end)) = previous_line_bounds(source_text, cursor) {
        let line = &source_text[line_start..line_end];
        if line.trim().is_empty() {
            break;
        }
        let indent = line
            .bytes()
            .take_while(|byte| matches!(*byte, b' ' | b'\t'))
            .count();
        if indent >= min_indent && line[indent..].starts_with('#') {
            start = line_start;
            cursor = line_start;
            continue;
        }
        break;
    }
    start
}

pub(super) fn previous_line_bounds(source_text: &str, cursor: usize) -> Option<(usize, usize)> {
    if cursor == 0 {
        return None;
    }

    let bytes = source_text.as_bytes();
    let mut end = cursor;
    if end > 0 && bytes[end - 1] == b'\n' {
        end -= 1;
        if end > 0 && bytes[end - 1] == b'\r' {
            end -= 1;
        }
    } else if end > 0 && bytes[end - 1] == b'\r' {
        end -= 1;
    }

    let mut start = end;
    while start > 0 && bytes[start - 1] != b'\n' && bytes[start - 1] != b'\r' {
        start -= 1;
    }
    Some((start, end))
}

pub(super) fn starts_with_line_ending(value: &str) -> bool {
    value.starts_with('\n') || value.starts_with('\r')
}

pub(super) fn ends_with_line_ending(value: &str) -> bool {
    value.ends_with('\n') || value.ends_with('\r')
}

pub(super) fn ends_with_blank_line(value: &str) -> bool {
    value.ends_with("\n\n") || value.ends_with("\r\n\r\n") || value.ends_with("\r\r")
}

pub(super) fn line_ending_literal(source_text: &str) -> &'static str {
    let bytes = source_text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if index + 1 < bytes.len() && bytes[index + 1] == b'\n' => return "\r\n",
            b'\r' => return "\r",
            b'\n' => return "\n",
            _ => index += 1,
        }
    }
    "\n"
}

#[cfg(test)]
mod tests {
    use super::{
        SiblingEntry, group_aware_insertion_offset, leading_comment_block_start,
        line_end_with_ending_after_offset, line_ending_literal, previous_line_bounds,
    };

    fn sibling_entry(source_text: &str, key: &str) -> SiblingEntry {
        let insertion_start = source_text
            .find(key)
            .unwrap_or_else(|| panic!("fixture should contain key '{key}'"));
        let end = line_end_with_ending_after_offset(source_text, insertion_start);
        SiblingEntry {
            key: key.to_string(),
            insertion_start,
            end,
        }
    }

    #[test]
    fn sorted_single_group_inserts_before_the_next_key() {
        let source = "alpha = 1\nbeta = 2\ndelta = 4\n";
        let entries = ["alpha", "beta", "delta"]
            .map(|key| sibling_entry(source, key))
            .to_vec();

        let offset = group_aware_insertion_offset(source, entries, source.len(), "charlie");

        assert_eq!(offset, source.find("delta").unwrap());
    }

    #[test]
    fn prefix_family_takes_precedence_over_unrelated_siblings() {
        let source = "zeta = 1\napi_host = \"localhost\"\napi_port = 8080\nalpha = 2\n";
        let entries = ["zeta", "api_host", "api_port", "alpha"]
            .map(|key| sibling_entry(source, key))
            .to_vec();
        let api_port_end = line_end_with_ending_after_offset(
            source,
            source
                .find("api_port")
                .expect("fixture should contain api_port"),
        );

        let offset = group_aware_insertion_offset(source, entries, source.len(), "api_timeout");

        assert_eq!(offset, api_port_end);
    }

    #[test]
    fn blank_lines_bound_sorted_group_insertion() {
        let source = "alpha = 1\ncharlie = 3\n\nomega = 4\nzulu = 5\n";
        let entries = ["alpha", "charlie", "omega", "zulu"]
            .map(|key| sibling_entry(source, key))
            .to_vec();

        let offset = group_aware_insertion_offset(source, entries, source.len(), "beta");

        assert_eq!(offset, source.find("charlie").unwrap());
    }

    #[test]
    fn unsorted_siblings_preserve_the_caller_fallback() {
        let source = "beta = 2\nalpha = 1\ngamma = 3\n";
        let entries = ["beta", "alpha", "gamma"]
            .map(|key| sibling_entry(source, key))
            .to_vec();
        let fallback = source.len();

        assert_eq!(
            group_aware_insertion_offset(source, entries, fallback, "delta"),
            fallback
        );
    }

    #[test]
    fn leading_comment_block_respects_blank_lines_and_indentation() {
        let source = "root = 1\n\n  # first\n  # second\n  child = 2\n";
        let key_start = source
            .find("  child")
            .expect("fixture should contain child");
        let comment_start = source
            .find("  # first")
            .expect("fixture should contain comment");

        assert_eq!(
            leading_comment_block_start(source, key_start, 2),
            comment_start
        );
        assert_eq!(leading_comment_block_start(source, key_start, 3), key_start);
    }

    #[test]
    fn line_helpers_preserve_crlf_and_cr_boundaries() {
        let source = "alpha\r\nbeta\rgamma";
        let beta_start = source.find("beta").expect("fixture should contain beta");
        let gamma_start = source.find("gamma").expect("fixture should contain gamma");

        assert_eq!(
            line_end_with_ending_after_offset(source, beta_start),
            gamma_start
        );
        assert_eq!(
            previous_line_bounds(source, gamma_start),
            Some((beta_start, beta_start + "beta".len()))
        );
        assert_eq!(line_ending_literal(source), "\r\n");
    }
}
