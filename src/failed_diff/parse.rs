use std::sync::LazyLock;

use regex::Regex;

use super::model::{FailedDiffChange, FailedDiffError, ParsedFailedDiff};

static NUMBERED_HUNK_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^@@ -\d+(?:,(\d+))? \+\d+(?:,(\d+))? @@")
        .expect("failed diff hunk regex should compile")
});

#[derive(Debug, Clone, PartialEq, Eq)]
enum HunkLine {
    Context(String),
    Removed(String),
    Added(String),
    NoFinalNewline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangedSide {
    Old,
    New,
}

pub(crate) fn parse_failed_diff(input: &str) -> Result<ParsedFailedDiff, FailedDiffError> {
    if input.trim().is_empty() {
        return Err(FailedDiffError::new("input is empty"));
    }

    let lines = logical_lines(input);
    let mut header_file = None::<String>;
    let mut changes = Vec::new();
    let mut index = 0usize;
    let mut source_hunk_count = 0usize;

    while index < lines.len() {
        let line = lines[index];

        if let Some(path) = line.strip_prefix("*** Update File: ") {
            register_header_file(&mut header_file, parse_header_path(path)?)?;
            index += 1;
            continue;
        }
        if line.starts_with("*** Add File:")
            || line.starts_with("*** Delete File:")
            || line.starts_with("*** Move to:")
        {
            return Err(FailedDiffError::new(
                "file creation, deletion, and rename diffs are not supported",
            ));
        }
        if let Some(old_path) = line.strip_prefix("--- ") {
            let Some(new_header) = lines.get(index + 1) else {
                return Err(FailedDiffError::new(
                    "unified diff old-file header is missing a matching new-file header",
                ));
            };
            let Some(new_path) = new_header.strip_prefix("+++ ") else {
                return Err(FailedDiffError::new(
                    "unified diff old-file header is missing a matching new-file header",
                ));
            };
            let old_path = parse_header_path(old_path)?;
            let new_path = parse_header_path(new_path)?;
            let path = normalize_unified_header_pair(old_path, new_path)?;
            register_header_file(&mut header_file, path)?;
            index += 2;
            continue;
        }
        if line.starts_with("@@") {
            let source_hunk_index = source_hunk_count;
            source_hunk_count += 1;
            let (body, next_index) = parse_hunk_body(&lines, index)?;
            changes.extend(split_change_blocks(&body)?.into_iter().enumerate().map(
                |(block_index, mut change)| {
                    change.source_hunk_index = source_hunk_index;
                    change.block_index = block_index;
                    change
                },
            ));
            index = next_index;
            continue;
        }

        index += 1;
    }

    if source_hunk_count == 0 {
        return Err(FailedDiffError::new("no unified diff hunk was found"));
    }
    if changes.is_empty() {
        return Err(FailedDiffError::new(
            "diff hunks do not contain any added or removed lines",
        ));
    }

    Ok(ParsedFailedDiff {
        header_file,
        source_hunk_count,
        changes,
    })
}

fn logical_lines(input: &str) -> Vec<&str> {
    let mut lines = input.split('\n').collect::<Vec<_>>();
    if input.ends_with('\n') {
        lines.pop();
    }
    lines
        .into_iter()
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

fn parse_header_path(raw: &str) -> Result<String, FailedDiffError> {
    let path = raw.split_once('\t').map_or(raw, |(path, _)| path).trim();
    if path == "/dev/null" {
        return Err(FailedDiffError::new(
            "file creation and deletion diffs are not supported",
        ));
    }
    if path.is_empty() {
        return Err(FailedDiffError::new("diff file header path is empty"));
    }
    if path.starts_with('"') || path.ends_with('"') {
        return Err(FailedDiffError::new(
            "quoted diff paths are not supported; provide FILE explicitly with a bare hunk",
        ));
    }

    Ok(path.to_string())
}

fn normalize_unified_header_pair(
    old_path: String,
    new_path: String,
) -> Result<String, FailedDiffError> {
    if old_path == new_path {
        return Ok(old_path);
    }
    if let (Some(old_suffix), Some(new_suffix)) =
        (old_path.strip_prefix("a/"), new_path.strip_prefix("b/"))
        && old_suffix == new_suffix
    {
        return Ok(old_suffix.to_string());
    }

    Err(FailedDiffError::new(
        "file creation, deletion, and rename diffs are not supported",
    ))
}

fn register_header_file(
    header_file: &mut Option<String>,
    path: String,
) -> Result<(), FailedDiffError> {
    match header_file {
        Some(existing) if existing != &path => Err(FailedDiffError::new(
            "failed-diff handoff supports exactly one file",
        )),
        Some(_) => Ok(()),
        None => {
            *header_file = Some(path);
            Ok(())
        }
    }
}

fn parse_hunk_body(
    lines: &[&str],
    header_index: usize,
) -> Result<(Vec<HunkLine>, usize), FailedDiffError> {
    let expected_counts = numbered_hunk_counts(lines[header_index])?;
    let mut body = Vec::new();
    let mut old_count = 0usize;
    let mut new_count = 0usize;
    let mut index = header_index + 1;

    while index < lines.len() {
        if let Some((expected_old, expected_new)) = expected_counts
            && old_count == expected_old
            && new_count == expected_new
            && lines[index] != r"\ No newline at end of file"
        {
            break;
        }

        let line = lines[index];
        if expected_counts.is_none() && is_unnumbered_hunk_boundary(lines, index) {
            break;
        }

        let parsed = if let Some(content) = line.strip_prefix(' ') {
            old_count += 1;
            new_count += 1;
            HunkLine::Context(content.to_string())
        } else if let Some(content) = line.strip_prefix('-') {
            old_count += 1;
            HunkLine::Removed(content.to_string())
        } else if let Some(content) = line.strip_prefix('+') {
            new_count += 1;
            HunkLine::Added(content.to_string())
        } else if line == r"\ No newline at end of file" {
            HunkLine::NoFinalNewline
        } else {
            return Err(FailedDiffError::new(format!(
                "malformed hunk line {}: expected context, '+', '-', or no-newline marker",
                index + 1
            )));
        };
        body.push(parsed);
        index += 1;
    }

    if let Some((expected_old, expected_new)) = expected_counts
        && (old_count != expected_old || new_count != expected_new)
    {
        return Err(FailedDiffError::new(format!(
            "hunk line counts do not match header: expected old/new {expected_old}/{expected_new}, got {old_count}/{new_count}"
        )));
    }
    if expected_counts.is_some()
        && index < lines.len()
        && looks_like_hunk_body(lines[index])
        && !is_structural_boundary(lines, index)
    {
        return Err(FailedDiffError::new(format!(
            "hunk contains extra body lines beyond its declared counts at line {}",
            index + 1
        )));
    }

    Ok((body, index))
}

fn numbered_hunk_counts(header: &str) -> Result<Option<(usize, usize)>, FailedDiffError> {
    let Some(captures) = NUMBERED_HUNK_HEADER.captures(header) else {
        if header == "@@" || header.starts_with("@@ ") {
            return Ok(None);
        }
        return Err(FailedDiffError::new(format!(
            "malformed hunk header '{header}'"
        )));
    };

    let old_count = parse_optional_count(captures.get(1).map(|capture| capture.as_str()))?;
    let new_count = parse_optional_count(captures.get(2).map(|capture| capture.as_str()))?;
    Ok(Some((old_count, new_count)))
}

fn parse_optional_count(count: Option<&str>) -> Result<usize, FailedDiffError> {
    count.map_or(Ok(1), |value| {
        value
            .parse::<usize>()
            .map_err(|_| FailedDiffError::new("hunk count does not fit in memory"))
    })
}

fn is_unnumbered_hunk_boundary(lines: &[&str], index: usize) -> bool {
    is_structural_boundary(lines, index)
}

fn is_structural_boundary(lines: &[&str], index: usize) -> bool {
    let line = lines[index];
    line.starts_with("@@")
        || line == "*** End Patch"
        || line.starts_with("*** Update File:")
        || line.starts_with("*** Add File:")
        || line.starts_with("*** Delete File:")
        || line.starts_with("*** Move to:")
        || line.starts_with("diff --git ")
        || (line.starts_with("--- ")
            && lines
                .get(index + 1)
                .is_some_and(|next| next.starts_with("+++ "))
            && lines
                .get(index + 2)
                .is_some_and(|next| next.starts_with("@@")))
}

fn looks_like_hunk_body(line: &str) -> bool {
    line.starts_with(' ')
        || line.starts_with('+')
        || line.starts_with('-')
        || line == r"\ No newline at end of file"
}

fn split_change_blocks(body: &[HunkLine]) -> Result<Vec<FailedDiffChange>, FailedDiffError> {
    let mut changes = Vec::new();
    let mut index = 0usize;

    while index < body.len() {
        while index < body.len() {
            let is_shared_context_marker = matches!(body[index], HunkLine::NoFinalNewline)
                && index > 0
                && index + 1 == body.len()
                && matches!(body[index - 1], HunkLine::Context(_));
            if matches!(body[index], HunkLine::Context(_)) || is_shared_context_marker {
                index += 1;
            } else {
                break;
            }
        }
        if index == body.len() {
            break;
        }
        if matches!(body[index], HunkLine::NoFinalNewline) {
            return Err(FailedDiffError::new(
                "no-newline marker must follow an added or removed line",
            ));
        }

        let change_start = index;
        while index < body.len() && !matches!(body[index], HunkLine::Context(_)) {
            index += 1;
        }
        let change_end = index;

        let before_context = context_before(body, change_start);
        let after_context = context_after(body, change_end);
        changes.push(build_change(
            &body[change_start..change_end],
            before_context,
            after_context,
        )?);
    }

    Ok(changes)
}

fn context_before(body: &[HunkLine], change_start: usize) -> Vec<String> {
    let start = body[..change_start]
        .iter()
        .rposition(|line| !matches!(line, HunkLine::Context(_)))
        .map_or(0, |index| index + 1);
    body[start..change_start]
        .iter()
        .filter_map(|line| match line {
            HunkLine::Context(content) => Some(content.clone()),
            _ => None,
        })
        .collect()
}

fn context_after(body: &[HunkLine], change_end: usize) -> Vec<String> {
    body[change_end..]
        .iter()
        .take_while(|line| matches!(line, HunkLine::Context(_)))
        .filter_map(|line| match line {
            HunkLine::Context(content) => Some(content.clone()),
            _ => None,
        })
        .collect()
}

fn build_change(
    lines: &[HunkLine],
    before_context: Vec<String>,
    after_context: Vec<String>,
) -> Result<FailedDiffChange, FailedDiffError> {
    let mut old_lines = Vec::new();
    let mut new_lines = Vec::new();
    let mut old_no_final_newline = false;
    let mut new_no_final_newline = false;
    let mut previous_side = None::<ChangedSide>;

    for line in lines {
        match line {
            HunkLine::Removed(content) => {
                old_lines.push(content.clone());
                previous_side = Some(ChangedSide::Old);
            }
            HunkLine::Added(content) => {
                new_lines.push(content.clone());
                previous_side = Some(ChangedSide::New);
            }
            HunkLine::NoFinalNewline => match previous_side {
                Some(ChangedSide::Old) => old_no_final_newline = true,
                Some(ChangedSide::New) => new_no_final_newline = true,
                None => {
                    return Err(FailedDiffError::new(
                        "no-newline marker must follow an added or removed line",
                    ));
                }
            },
            HunkLine::Context(_) => unreachable!("change block excludes context lines"),
        }
    }

    if old_lines.is_empty() && new_lines.is_empty() {
        return Err(FailedDiffError::new(
            "diff hunk does not contain an added or removed line",
        ));
    }
    if old_no_final_newline != new_no_final_newline {
        return Err(FailedDiffError::new(
            "changing only the final-newline state is not supported by failed-diff handoff",
        ));
    }

    Ok(FailedDiffChange {
        source_hunk_index: 0,
        block_index: 0,
        old_lines,
        new_lines,
        before_context,
        after_context,
    })
}
