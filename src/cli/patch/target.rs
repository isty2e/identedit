use std::path::Path;

use crate::error::{IdenteditError, TargetCandidateContext};
use crate::execution_context::ExecutionContext;
use crate::handle::SelectionHandle;
use crate::hash::HASH_HEX_LEN;
use crate::hashline::{HASHLINE_PUBLIC_HEX_LEN, LineAnchor};
use crate::selector::Selector;

use super::PatchArgs;

const CANDIDATE_PREVIEW_MAX_CHARS: usize = 120;

#[derive(Debug, Clone)]
pub(super) enum PatchTargetIngress {
    NodeIdentity(String),
    NodeSelector { kind: String, name_pattern: String },
    NodeSymbol(String),
    LineAnchor(LineAnchor),
    FileStart,
    FileEnd,
    ConfigPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NodeTargetSelector {
    Identity(String),
    Selector { kind: String, name_pattern: String },
    Symbol(String),
}

impl NodeTargetSelector {
    pub(super) fn resolve(self, file: &Path) -> Result<SelectionHandle, IdenteditError> {
        match self {
            Self::Identity(identity) => resolve_unique_identity_handle_for_patch(file, &identity),
            Self::Selector { kind, name_pattern } => {
                resolve_unique_selector_handle_for_patch(file, &kind, &name_pattern)
            }
            Self::Symbol(symbol) => resolve_unique_symbol_handle_for_patch(file, &symbol),
        }
    }
}

pub(super) fn resolve_patch_target_ingress(
    args: &PatchArgs,
) -> Result<PatchTargetIngress, IdenteditError> {
    let selector_present = args.kind.is_some() || args.name.is_some();
    let symbol_present = args.symbol.is_some();

    if let Some(path) = args.config_path.clone() {
        if args.at.is_some()
            || args.identity.is_some()
            || args.anchor.is_some()
            || selector_present
            || symbol_present
        {
            return Err(IdenteditError::InvalidRequest {
                message: "--config-path cannot be combined with --at, --identity, --anchor, --kind, --name, or --symbol. Config path mode supports --set-value, --append-value, or --delete. Use --create-missing only with --set-value.".to_string(),
            });
        }
        return Ok(PatchTargetIngress::ConfigPath(path));
    }

    if let Some(at) = args.at.as_deref() {
        if args.identity.is_some() || args.anchor.is_some() || selector_present || symbol_present {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "Choose exactly one target selector. Use --at <target> by itself, or use --identity, --anchor, --symbol, or --kind with --name."
                        .to_string(),
            });
        }
        return parse_patch_at_target(at);
    }

    match (
        args.identity.clone(),
        args.anchor.clone(),
        args.kind.clone(),
        args.name.clone(),
        args.symbol.clone(),
    ) {
        (Some(identity), None, None, None, None) => Ok(PatchTargetIngress::NodeIdentity(identity)),
        (None, Some(anchor), None, None, None) => LineAnchor::parse(&anchor)
            .map(PatchTargetIngress::LineAnchor)
            .map_err(|error| IdenteditError::InvalidRequest {
                message: error.to_string(),
            }),
        (None, None, Some(kind), Some(name_pattern), None) => {
            Ok(PatchTargetIngress::NodeSelector { kind, name_pattern })
        }
        (None, None, None, None, Some(symbol)) => Ok(PatchTargetIngress::NodeSymbol(symbol)),
        (None, None, Some(_), None, None) | (None, None, None, Some(_), None) => {
            Err(IdenteditError::InvalidRequest {
                message:
                    "Direct symbol targeting requires both --kind and --name. Example: --kind function_definition --name process_*."
                        .to_string(),
            })
        }
        _ => Err(IdenteditError::InvalidRequest {
            message:
                "Choose exactly one target selector in flag mode: --at <target>, --identity <hex16>, --anchor <line:hash>, --symbol <name>, or --kind <kind> --name <glob>."
                    .to_string(),
        }),
    }
}

fn parse_patch_at_target(raw: &str) -> Result<PatchTargetIngress, IdenteditError> {
    let normalized = raw.trim();
    if normalized.eq_ignore_ascii_case("file-start") {
        return Ok(PatchTargetIngress::FileStart);
    }
    if normalized.eq_ignore_ascii_case("file-end") {
        return Ok(PatchTargetIngress::FileEnd);
    }

    if is_hex_with_len(normalized, HASH_HEX_LEN) {
        return Ok(PatchTargetIngress::NodeIdentity(
            normalized.to_ascii_lowercase(),
        ));
    }

    if normalized.contains(':') {
        let anchor =
            LineAnchor::parse(normalized).map_err(|error| IdenteditError::InvalidRequest {
                message: error.to_string(),
            })?;
        return Ok(PatchTargetIngress::LineAnchor(anchor));
    }

    Err(IdenteditError::InvalidRequest {
        message: format!(
            "Invalid --at target '{}': expected hex{} identity, <line>:<hex{}> anchor, file-start, or file-end",
            raw, HASH_HEX_LEN, HASHLINE_PUBLIC_HEX_LEN
        ),
    })
}

fn is_hex_with_len(value: &str, len: usize) -> bool {
    value.len() == len && value.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

fn resolve_unique_identity_handle_for_patch(
    file: &Path,
    identity: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let (source_text, handles) = parse_patch_handles(file)?;
    let matches = handles
        .iter()
        .filter(|handle| handle.identity == identity)
        .cloned()
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => Err(IdenteditError::TargetMissing {
            identity: identity.to_string(),
            file: file.display().to_string(),
        }),
        [single] => Ok(single.clone()),
        candidates => Err(IdenteditError::AmbiguousTarget {
            identity: identity.to_string(),
            file: file.display().to_string(),
            candidates: candidates.len(),
            candidate_contexts: build_candidate_contexts(candidates, &handles, &source_text),
        }),
    }
}

fn resolve_unique_selector_handle_for_patch(
    file: &Path,
    kind: &str,
    name_pattern: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let (source_text, handles) = parse_patch_handles(file)?;
    let selector = Selector {
        kind: kind.to_string(),
        name_pattern: Some(name_pattern.to_string()),
        exclude_kinds: vec![],
    };
    let selector_description = format!("kind='{kind}', name='{name_pattern}'");
    let matches = selector.filter(handles.clone())?;

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
            candidate_contexts: build_candidate_contexts(candidates, &handles, &source_text),
        }),
    }
}

fn resolve_unique_symbol_handle_for_patch(
    file: &Path,
    symbol: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let query = symbol.trim();
    if query.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "--symbol must not be empty".to_string(),
        });
    }

    let (source_text, handles) = parse_patch_handles(file)?;
    let matches = handles
        .iter()
        .filter(|handle| symbol_matches(handle, &handles, query))
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
            candidate_contexts: build_candidate_contexts(candidates, &handles, &source_text),
        }),
    }
}

fn parse_patch_handles(file: &Path) -> Result<(String, Vec<SelectionHandle>), IdenteditError> {
    let context = ExecutionContext::new();
    let source_text = context.read_file_utf8(file)?;
    let handles = context.parse_handles_for_source(file, source_text.as_bytes())?;
    Ok((source_text, handles))
}

fn build_candidate_contexts(
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

    name == query
        || qualified_symbol_name(handle, handles).is_some_and(|qualified| qualified == query)
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
    Some(qualified.join("."))
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

    use crate::handle::{SelectionHandle, Span};

    use super::{line_number_for_offset, preview_for_candidate};

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
