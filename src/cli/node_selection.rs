use std::path::Path;

use crate::error::{IdenteditError, TargetCandidateContext};
use crate::handle::SelectionHandle;

const CANDIDATE_PREVIEW_MAX_CHARS: usize = 120;
const QUALIFIED_NAME_SEPARATOR: &str = ".";

pub(super) fn resolve_symbol(
    file: &Path,
    source_text: &str,
    handles: &[SelectionHandle],
    symbol: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let query = symbol.trim();
    if query.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "--symbol must not be empty".to_string(),
        });
    }

    let matches = handles
        .iter()
        .filter(|handle| symbol_matches(handle, handles, query))
        .cloned()
        .collect::<Vec<_>>();
    let selector_description = format!("symbol='{query}'");

    match matches.as_slice() {
        [] => Err(IdenteditError::TargetMissingSelector {
            selector: selector_description,
            file: file.display().to_string(),
        }),
        [single] => Ok(single.clone()),
        candidates => Err(IdenteditError::AmbiguousTargetSelector {
            selector: selector_description,
            file: file.display().to_string(),
            candidates: candidates.len(),
            candidate_contexts: build_candidate_contexts(candidates, handles, source_text),
        }),
    }
}

pub(super) fn build_candidate_contexts(
    candidates: &[SelectionHandle],
    all_handles: &[SelectionHandle],
    source_text: &str,
) -> Vec<TargetCandidateContext> {
    candidates
        .iter()
        .map(|candidate| TargetCandidateContext {
            identity: candidate.identity.clone(),
            expected_old_hash: candidate.expected_old_hash.to_string(),
            kind: candidate.kind.clone(),
            name: candidate.name.clone(),
            qualified_name: qualified_symbol_name(candidate, all_handles),
            span: candidate.span,
            line: line_number_for_offset(source_text, candidate.span.start),
            preview: preview_for_candidate(candidate),
        })
        .collect()
}

fn symbol_matches(handle: &SelectionHandle, handles: &[SelectionHandle], query: &str) -> bool {
    let Some(name) = handle.name.as_deref() else {
        return false;
    };

    // A qualified match must end with the separator and the entire opaque name.
    // This is only a candidate filter; full qualification still decides equality.
    name == query
        || (query
            .strip_suffix(name)
            .is_some_and(|prefix| prefix.ends_with(QUALIFIED_NAME_SEPARATOR))
            && qualified_symbol_name(handle, handles).is_some_and(|qualified| qualified == query))
}

fn qualified_symbol_name(handle: &SelectionHandle, handles: &[SelectionHandle]) -> Option<String> {
    let name = handle.name.as_ref()?;
    let mut parts = handles
        .iter()
        .filter(|candidate| is_named_ancestor(candidate, handle))
        .collect::<Vec<_>>();
    parts.sort_by_key(|candidate| (candidate.span.start, std::cmp::Reverse(candidate.span.end)));

    let mut qualified = parts
        .into_iter()
        .filter_map(|candidate| candidate.name.as_deref())
        .map(str::to_string)
        .collect::<Vec<_>>();
    qualified.push(name.clone());
    Some(qualified.join(QUALIFIED_NAME_SEPARATOR))
}

fn line_number_for_offset(source_text: &str, offset: usize) -> usize {
    let bytes = source_text.as_bytes();
    let limit = offset.min(bytes.len());
    let mut line = 1;
    let mut index = 0;

    while index < limit {
        match bytes[index] {
            b'\n' => line += 1,
            b'\r' => {
                line += 1;
                if index + 1 < limit && bytes[index + 1] == b'\n' {
                    index += 1;
                }
            }
            _ => {}
        }
        index += 1;
    }

    line
}

fn preview_for_candidate(candidate: &SelectionHandle) -> String {
    let line = logical_lines(&candidate.text)
        .into_iter()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    truncate_chars(line, CANDIDATE_PREVIEW_MAX_CHARS)
}

fn logical_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                let end = if index > start && bytes[index - 1] == b'\r' {
                    index - 1
                } else {
                    index
                };
                lines.push(&text[start..end]);
                start = index + 1;
            }
            b'\r' => {
                lines.push(&text[start..index]);
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    index += 1;
                }
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }

    lines.push(&text[start..]);
    lines
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn is_named_ancestor(candidate: &SelectionHandle, handle: &SelectionHandle) -> bool {
    candidate.name.is_some()
        && candidate.span.start <= handle.span.start
        && handle.span.end <= candidate.span.end
        && !(candidate.span.start == handle.span.start
            && candidate.span.end == handle.span.end
            && candidate.kind == handle.kind
            && candidate.name == handle.name)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use proptest::prelude::*;

    use crate::handle::{SelectionHandle, Span};

    use super::{
        line_number_for_offset, preview_for_candidate, qualified_symbol_name, symbol_matches,
    };

    fn named_handle(name: Option<&str>, kind: &str, start: usize, end: usize) -> SelectionHandle {
        SelectionHandle::from_parts(
            PathBuf::from("fixture"),
            Span { start, end },
            kind.to_string(),
            name.map(str::to_string),
            format!("{kind} {name:?} {start}:{end}"),
        )
    }

    #[test]
    fn symbol_matches_preserves_opaque_names_and_full_container_paths() {
        for name in ["a.b", "a::b", "a:b", "한글.이름", "", ".", ".a", "a."] {
            let handles = [
                named_handle(Some("Outer.part"), "outer", 0, 100),
                named_handle(Some("Inner::scope"), "inner", 10, 90),
                named_handle(Some(name), "target", 20, 30),
                named_handle(None, "unnamed", 21, 22),
            ];
            let target = &handles[2];

            assert!(symbol_matches(target, &handles, name));
            assert!(symbol_matches(
                target,
                &handles,
                &format!("Outer.part.Inner::scope.{name}")
            ));
            for query in [
                format!("Wrong.part.Inner::scope.{name}"),
                format!("Inner::scope.{name}"),
                format!("Outer.part.Inner::scope.not{name}"),
                format!("Outer.part.Inner::scope.{name}.extra"),
            ] {
                assert!(
                    !symbol_matches(target, &handles, &query),
                    "{name:?}: {query:?}"
                );
            }
            assert!(!symbol_matches(&handles[3], &handles, name));
        }
    }

    proptest! {
        #[test]
        fn symbol_matches_is_equivalent_to_unfiltered_qualification(
            observations in prop::collection::vec(
                (0usize..64, 0usize..64, 0u8..3, prop::option::of("[a-zA-Z0-9_.: 한글]{0,8}")),
                0..16,
            ),
            query in "[a-zA-Z0-9_.: 한글]{0,24}",
        ) {
            let handles = observations
                .iter()
                .map(|(start, end, kind, name)| {
                    named_handle(
                        name.as_deref(),
                        &format!("kind{kind}"),
                        (*start).min(*end),
                        (*start).max(*end),
                    )
                })
                .collect::<Vec<_>>();
            let mut queries = vec![query];
            for handle in &handles {
                if let Some(name) = &handle.name {
                    queries.push(name.clone());
                    queries.push(format!("wrong.{name}"));
                    queries.push(qualified_symbol_name(handle, &handles).unwrap());
                }
            }

            for query in queries {
                let legacy = handles
                    .iter()
                    .filter(|handle| {
                        handle.name.as_deref().is_some_and(|name| {
                            name == query
                                || qualified_symbol_name(handle, &handles).as_deref() == Some(&query)
                        })
                    })
                    .collect::<Vec<_>>();
                let actual = handles
                    .iter()
                    .filter(|handle| symbol_matches(handle, &handles, &query))
                    .collect::<Vec<_>>();

                prop_assert_eq!(actual, legacy, "query: {:?}", query);
            }
        }
    }

    #[test]
    fn line_number_for_offset_handles_lf_crlf_and_cr_boundaries() {
        assert_eq!(line_number_for_offset("a\nb\nc", 0), 1);
        assert_eq!(line_number_for_offset("a\nb\nc", 2), 2);
        assert_eq!(line_number_for_offset("a\r\nb\r\nc", 3), 2);
        assert_eq!(line_number_for_offset("a\rb\rc", 2), 2);
        assert_eq!(line_number_for_offset("a\rb\rc", 99), 3);
    }

    #[test]
    fn preview_for_candidate_uses_first_non_empty_logical_line_and_truncates() {
        let long_line = format!("{}()", "x".repeat(140));
        let handle = SelectionHandle::from_parts(
            PathBuf::from("fixture.py"),
            Span { start: 0, end: 140 },
            "function_definition".to_string(),
            Some("long_name".to_string()),
            format!("\r\n\r{long_line}\r    pass"),
        );

        let preview = preview_for_candidate(&handle);

        assert!(preview.starts_with("xxxxxxxx"));
        assert!(preview.ends_with("..."));
        assert!(preview.len() < long_line.len());
    }
}
