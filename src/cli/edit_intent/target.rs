use std::path::Path;

use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::handle::SelectionHandle;
use crate::hash::HASH_HEX_LEN;
use crate::hashline::{HASHLINE_PUBLIC_HEX_LEN, LineAnchor};
use crate::selector::Selector;

use super::super::node_selection::{build_candidate_contexts, resolve_symbol};
use super::EditIntentArgs;

#[derive(Debug, Clone)]
pub(super) enum EditTargetIngress {
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
            Self::Identity(identity) => resolve_unique_identity_handle(file, &identity),
            Self::Selector { kind, name_pattern } => {
                resolve_unique_selector_handle(file, &kind, &name_pattern)
            }
            Self::Symbol(symbol) => {
                let (source_text, handles) = parse_target_handles(file)?;
                resolve_symbol(file, &source_text, &handles, &symbol)
            }
        }
    }
}

pub(super) fn resolve_edit_target_ingress(
    args: &EditIntentArgs,
) -> Result<EditTargetIngress, IdenteditError> {
    let selector_present = args.kind.is_some() || args.name.is_some();
    let symbol_present = args.symbol.is_some();

    if let Some(path) = args.config_path.clone() {
        if args.at.is_some() || selector_present || symbol_present {
            return Err(IdenteditError::InvalidRequest {
                message: "--config-path cannot be combined with --at, --kind, --name, or --symbol. Config path mode supports --set-value, --append-value, or --delete. Use --create-missing only with --set-value.".to_string(),
            });
        }
        return Ok(EditTargetIngress::ConfigPath(path));
    }

    if let Some(at) = args.at.as_deref() {
        if selector_present || symbol_present {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "Choose exactly one target selector. Use --at <target> by itself, or use --symbol or --kind with --name."
                        .to_string(),
            });
        }
        return parse_at_target(at);
    }

    match (args.kind.clone(), args.name.clone(), args.symbol.clone()) {
        (Some(kind), Some(name_pattern), None) => {
            Ok(EditTargetIngress::NodeSelector { kind, name_pattern })
        }
        (None, None, Some(symbol)) => Ok(EditTargetIngress::NodeSymbol(symbol)),
        (Some(_), None, None) | (None, Some(_), None) => {
            Err(IdenteditError::InvalidRequest {
                message:
                    "Direct symbol targeting requires both --kind and --name. Example: --kind function_definition --name process_*."
                        .to_string(),
            })
        }
        _ => Err(IdenteditError::InvalidRequest {
            message:
                "Choose exactly one target selector in flag mode: --at <target>, --symbol <name>, or --kind <kind> --name <glob>."
                    .to_string(),
        }),
    }
}

fn parse_at_target(raw: &str) -> Result<EditTargetIngress, IdenteditError> {
    let normalized = raw.trim();
    if normalized.eq_ignore_ascii_case("file-start") {
        return Ok(EditTargetIngress::FileStart);
    }
    if normalized.eq_ignore_ascii_case("file-end") {
        return Ok(EditTargetIngress::FileEnd);
    }

    if is_hex_with_len(normalized, HASH_HEX_LEN) {
        return Ok(EditTargetIngress::NodeIdentity(
            normalized.to_ascii_lowercase(),
        ));
    }

    if normalized.contains(':') {
        let anchor =
            LineAnchor::parse(normalized).map_err(|error| IdenteditError::InvalidRequest {
                message: error.to_string(),
            })?;
        return Ok(EditTargetIngress::LineAnchor(anchor));
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

fn resolve_unique_identity_handle(
    file: &Path,
    identity: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let (source_text, handles) = parse_target_handles(file)?;
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

fn resolve_unique_selector_handle(
    file: &Path,
    kind: &str,
    name_pattern: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let (source_text, handles) = parse_target_handles(file)?;
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

fn parse_target_handles(file: &Path) -> Result<(String, Vec<SelectionHandle>), IdenteditError> {
    let context = ExecutionContext::new();
    let source_text = context.read_file_utf8(file)?;
    let handles = context.parse_handles_for_source(file, source_text.as_bytes())?;
    Ok((source_text, handles))
}
