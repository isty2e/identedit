use std::collections::BTreeMap;

use super::{
    HASHLINE_DISPLAY_MAX_HEX_LEN, HASHLINE_DISPLAY_MIN_HEX_LEN, HashlineApplyError,
    HashlineApplyMode, HashlineCheckResult, HashlineEdit, HashlineMismatchStatus, ResolvedEdit,
    ResolvedOperation, check_hashline_edits, format_line_ref,
};

pub(super) fn prepare_edits_for_mode(
    source: &str,
    edits: &[HashlineEdit],
    mode: HashlineApplyMode,
) -> Result<Vec<HashlineEdit>, HashlineApplyError> {
    match mode {
        HashlineApplyMode::Strict => Ok(edits.to_vec()),
        HashlineApplyMode::Repair => prepare_repair_edits(source, edits),
    }
}

fn prepare_repair_edits(
    source: &str,
    edits: &[HashlineEdit],
) -> Result<Vec<HashlineEdit>, HashlineApplyError> {
    let check = check_hashline_edits(source, edits)?;
    if check.ok {
        return Ok(normalize_repair_edit_texts(edits));
    }

    if !check.mismatches.iter().all(|mismatch| {
        mismatch.status == HashlineMismatchStatus::Remappable && mismatch.remaps.len() == 1
    }) {
        return Err(HashlineApplyError::PreconditionFailed { check });
    }

    let remapped = remap_anchors_from_check(edits, &check);
    let normalized = normalize_repair_edit_texts(&remapped);
    let repaired_check = check_hashline_edits(source, &normalized)?;
    if !repaired_check.ok {
        return Err(HashlineApplyError::PreconditionFailed {
            check: repaired_check,
        });
    }

    Ok(normalized)
}

fn remap_anchors_from_check(
    edits: &[HashlineEdit],
    check: &HashlineCheckResult,
) -> Vec<HashlineEdit> {
    let mut remap_by_anchor = BTreeMap::<(usize, String), String>::new();
    for mismatch in &check.mismatches {
        if mismatch.status == HashlineMismatchStatus::Remappable && mismatch.remaps.len() == 1 {
            let target = &mismatch.remaps[0];
            remap_by_anchor.insert(
                (mismatch.edit_index, mismatch.anchor.clone()),
                format_line_ref(target.line, &target.hash),
            );
        }
    }

    edits
        .iter()
        .cloned()
        .enumerate()
        .map(|(edit_index, mut edit)| {
            match &mut edit {
                HashlineEdit::SetLine { set_line } => {
                    if let Some(remapped) =
                        remap_by_anchor.get(&(edit_index, set_line.anchor.clone()))
                    {
                        set_line.anchor = remapped.clone();
                    }
                }
                HashlineEdit::ReplaceLines { replace_lines } => {
                    if let Some(remapped) =
                        remap_by_anchor.get(&(edit_index, replace_lines.start_anchor.clone()))
                    {
                        replace_lines.start_anchor = remapped.clone();
                    }
                    if let Some(end_anchor) = &mut replace_lines.end_anchor
                        && let Some(remapped) =
                            remap_by_anchor.get(&(edit_index, end_anchor.clone()))
                    {
                        *end_anchor = remapped.clone();
                    }
                }
                HashlineEdit::InsertAfter { insert_after } => {
                    if let Some(remapped) =
                        remap_by_anchor.get(&(edit_index, insert_after.anchor.clone()))
                    {
                        insert_after.anchor = remapped.clone();
                    }
                }
            }
            edit
        })
        .collect()
}

fn normalize_repair_edit_texts(edits: &[HashlineEdit]) -> Vec<HashlineEdit> {
    edits
        .iter()
        .cloned()
        .map(|mut edit| {
            match &mut edit {
                HashlineEdit::SetLine { set_line } => {
                    set_line.new_text = apply_repair_text_heuristics(&set_line.new_text);
                }
                HashlineEdit::ReplaceLines { replace_lines } => {
                    replace_lines.new_text = apply_repair_text_heuristics(&replace_lines.new_text);
                }
                HashlineEdit::InsertAfter { insert_after } => {
                    insert_after.text = apply_repair_text_heuristics(&insert_after.text);
                }
            }
            edit
        })
        .collect()
}

pub(super) fn apply_repair_merge_expansion(lines: &[String], resolved: &mut [ResolvedEdit]) {
    for resolved_edit in resolved.iter_mut() {
        let ResolvedOperation::ReplaceRange {
            start_line,
            end_line,
            replacement_lines,
        } = &mut resolved_edit.operation
        else {
            continue;
        };

        if *start_line != *end_line || replacement_lines.len() != 1 || *start_line >= lines.len() {
            continue;
        }

        let current_line = &lines[*start_line - 1];
        let next_line = &lines[*start_line];
        if should_expand_single_line_merge(current_line, next_line, &replacement_lines[0]) {
            *end_line += 1;
            resolved_edit.span.end_line = *end_line;
        }
    }
}

fn should_expand_single_line_merge(current_line: &str, next_line: &str, replacement: &str) -> bool {
    if !has_merge_continuation_hint(current_line) {
        return false;
    }

    let exact = format!("{current_line}{next_line}");
    if replacement == exact {
        return true;
    }

    let trimmed_join = format!("{}{}", current_line.trim_end(), next_line.trim_start());
    if replacement == trimmed_join {
        return true;
    }

    let spaced_join = format!("{} {}", current_line.trim_end(), next_line.trim_start());
    replacement == spaced_join
}

fn has_merge_continuation_hint(current_line: &str) -> bool {
    let trimmed = current_line.trim_end();
    const TOKENS: [&str; 5] = ["&&", "||", "??", "\\", ","];
    TOKENS.iter().any(|token| {
        if !trimmed.ends_with(token) {
            return false;
        }

        let prefix = trimmed[..trimmed.len() - token.len()].trim_end();
        !prefix.is_empty() && !prefix.ends_with(':')
    })
}

fn apply_repair_text_heuristics(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n");
    let lines = normalized.split('\n').collect::<Vec<_>>();
    let non_empty = lines.iter().filter(|line| !line.trim().is_empty()).count();
    if non_empty == 0 {
        return normalized;
    }

    let mut hash_prefix_count = 0usize;
    let mut plus_prefix_count = 0usize;
    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }

        let mut candidate = *line;
        if let Some(stripped_plus) = strip_diff_plus_prefix_once(candidate) {
            plus_prefix_count += 1;
            candidate = stripped_plus;
        }

        if strip_hashline_display_prefix_once(candidate).is_some() {
            hash_prefix_count += 1;
        }
    }

    let strip_hash_prefix = hash_prefix_count >= 2 && hash_prefix_count * 2 > non_empty;
    let strip_plus_prefix = plus_prefix_count > 0 && plus_prefix_count * 2 > non_empty;

    lines
        .into_iter()
        .map(|line| {
            let mut candidate = line;
            if strip_plus_prefix && let Some(stripped) = strip_diff_plus_prefix_once(candidate) {
                candidate = stripped;
            }
            if strip_hash_prefix
                && let Some(stripped) = strip_hashline_display_prefix_once(candidate)
            {
                candidate = stripped;
            }
            candidate.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_diff_plus_prefix_once(line: &str) -> Option<&str> {
    if line.starts_with('+') && !line.starts_with("++") {
        return Some(&line[1..]);
    }
    None
}

fn strip_hashline_display_prefix_once(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 || index >= bytes.len() || bytes[index] != b':' {
        return None;
    }
    index += 1;

    let hash_start = index;
    while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
        index += 1;
    }
    let hash_len = index.saturating_sub(hash_start);
    if !(HASHLINE_DISPLAY_MIN_HEX_LEN..=HASHLINE_DISPLAY_MAX_HEX_LEN).contains(&hash_len) {
        return None;
    }
    if index >= bytes.len() || bytes[index] != b'|' {
        return None;
    }

    let hash = &line[hash_start..index];
    let content = &line[index + 1..];
    if !hash_matches_content(hash, content) {
        return None;
    }

    Some(content)
}

fn hash_matches_content(hash: &str, content: &str) -> bool {
    let normalized = hash.to_ascii_lowercase();
    compute_line_hash_full(content).starts_with(&normalized)
}

fn compute_line_hash_full(line: &str) -> String {
    blake3::hash(line.as_bytes()).to_hex().to_string()
}
