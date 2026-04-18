use std::io::Read;
use std::path::PathBuf;

use crate::error::IdenteditError;

use super::PatchArgs;

#[derive(Debug, Clone)]
pub(super) enum PatchTextSource {
    File(PathBuf),
    Stdin,
}

pub(super) fn resolve_patch_text_source(
    args: &PatchArgs,
) -> Result<Option<PatchTextSource>, IdenteditError> {
    match (args.text_file.clone(), args.stdin_text) {
        (Some(_), true) => Err(IdenteditError::InvalidRequest {
            message: "Choose exactly one external text source: --text-file <path> or --stdin-text."
                .to_string(),
        }),
        (Some(path), false) => Ok(Some(PatchTextSource::File(path))),
        (None, true) => Ok(Some(PatchTextSource::Stdin)),
        (None, false) => Ok(None),
    }
}

pub(super) fn read_patch_text_source(source: PatchTextSource) -> Result<String, IdenteditError> {
    match source {
        PatchTextSource::File(path) => {
            std::fs::read_to_string(&path).map_err(|error| IdenteditError::io(&path, error))
        }
        PatchTextSource::Stdin => {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|error| IdenteditError::StdinRead { source: error })?;
            Ok(buffer)
        }
    }
}

pub(super) fn resolve_patch_text_payload(
    flag_name: &str,
    raw_value: Option<Option<String>>,
    text_source: Option<PatchTextSource>,
) -> Result<Option<String>, IdenteditError> {
    match raw_value {
        None => Ok(None),
        Some(Some(inline)) => {
            if text_source.is_some() {
                Err(IdenteditError::InvalidRequest {
                    message: format!(
                        "{flag_name} accepts either inline text or one external text source. Use {flag_name} <text>, {flag_name} --text-file <path>, or {flag_name} --stdin-text."
                    ),
                })
            } else {
                Ok(Some(inline))
            }
        }
        Some(None) => {
            let source = text_source.ok_or_else(|| IdenteditError::InvalidRequest {
                message: format!(
                    "{flag_name} requires text. Provide {flag_name} <text>, {flag_name} --text-file <path>, or {flag_name} --stdin-text."
                ),
            })?;
            read_patch_text_source(source).map(Some)
        }
    }
}

pub(super) fn text_arg_present(raw_value: &Option<Option<String>>) -> bool {
    raw_value.is_some()
}

pub(super) fn reject_unused_text_source(
    text_source: Option<PatchTextSource>,
    valid_operations: &str,
) -> Result<(), IdenteditError> {
    if text_source.is_some() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "External text sources require a text-taking operation. Use {valid_operations} with --text-file <path> or --stdin-text."
            ),
        });
    }
    Ok(())
}
