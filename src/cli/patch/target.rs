use std::path::Path;

use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::hash::HASH_HEX_LEN;
use crate::hashline::{HASHLINE_PUBLIC_HEX_LEN, parse_line_ref};
use crate::selector::Selector;
use crate::transform::parse::parse_handles_for_file;

use super::PatchArgs;

#[derive(Debug, Clone)]
pub(super) enum PatchTargetIngress {
    NodeIdentity(String),
    NodeSelector { kind: String, name_pattern: String },
    LineAnchor(String),
    FileStart,
    FileEnd,
    ConfigPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NodeTargetSelector {
    Identity(String),
    Selector { kind: String, name_pattern: String },
}

impl NodeTargetSelector {
    pub(super) fn resolve(self, file: &Path) -> Result<SelectionHandle, IdenteditError> {
        match self {
            Self::Identity(identity) => resolve_unique_identity_handle_for_patch(file, &identity),
            Self::Selector { kind, name_pattern } => {
                resolve_unique_selector_handle_for_patch(file, &kind, &name_pattern)
            }
        }
    }
}

pub(super) fn resolve_patch_target_ingress(
    args: &PatchArgs,
) -> Result<PatchTargetIngress, IdenteditError> {
    let selector_present = args.kind.is_some() || args.name.is_some();

    if let Some(path) = args.config_path.clone() {
        if args.at.is_some() || args.identity.is_some() || args.anchor.is_some() || selector_present
        {
            return Err(IdenteditError::InvalidRequest {
                message: "--config-path cannot be combined with --at, --identity, --anchor, --kind, or --name. Config path mode supports --set-value, --append-value, or --delete. Use --create-missing only with --set-value.".to_string(),
            });
        }
        return Ok(PatchTargetIngress::ConfigPath(path));
    }

    if let Some(at) = args.at.as_deref() {
        if args.identity.is_some() || args.anchor.is_some() || selector_present {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "Choose exactly one target selector. Use --at <target> by itself, or use --identity, --anchor, or --kind with --name."
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
    ) {
        (Some(identity), None, None, None) => Ok(PatchTargetIngress::NodeIdentity(identity)),
        (None, Some(anchor), None, None) => Ok(PatchTargetIngress::LineAnchor(anchor)),
        (None, None, Some(kind), Some(name_pattern)) => {
            Ok(PatchTargetIngress::NodeSelector { kind, name_pattern })
        }
        (None, None, Some(_), None) | (None, None, None, Some(_)) => {
            Err(IdenteditError::InvalidRequest {
                message:
                    "Direct symbol targeting requires both --kind and --name. Example: --kind function_definition --name process_*."
                        .to_string(),
            })
        }
        _ => Err(IdenteditError::InvalidRequest {
            message:
                "Choose exactly one target selector in flag mode: --at <target>, --identity <hex16>, --anchor <line:hash>, or --kind <kind> --name <glob>."
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

    if is_line_anchor_with_hash_len(normalized, HASHLINE_PUBLIC_HEX_LEN) {
        let parsed =
            parse_line_ref(normalized).map_err(|error| IdenteditError::InvalidRequest {
                message: error.to_string(),
            })?;
        return Ok(PatchTargetIngress::LineAnchor(format!(
            "{}:{}",
            parsed.line, parsed.hash
        )));
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

fn is_line_anchor_with_hash_len(value: &str, hash_len: usize) -> bool {
    let Some((line, hash)) = value.split_once(':') else {
        return false;
    };
    !line.is_empty()
        && line.as_bytes().iter().all(u8::is_ascii_digit)
        && hash.len() == hash_len
        && hash.as_bytes().iter().all(u8::is_ascii_hexdigit)
}

fn resolve_unique_identity_handle_for_patch(
    file: &Path,
    identity: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let handles = parse_handles_for_file(file)?;
    let matches = handles
        .into_iter()
        .filter(|handle| handle.identity == identity)
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
        }),
    }
}

fn resolve_unique_selector_handle_for_patch(
    file: &Path,
    kind: &str,
    name_pattern: &str,
) -> Result<SelectionHandle, IdenteditError> {
    let selector = Selector {
        kind: kind.to_string(),
        name_pattern: Some(name_pattern.to_string()),
        exclude_kinds: vec![],
    };
    let selector_description = format!("kind='{kind}', name='{name_pattern}'");
    let matches = selector.filter(parse_handles_for_file(file)?)?;

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
        }),
    }
}
