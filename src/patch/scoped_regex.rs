use std::path::Path;

use regex::Regex;

use crate::changeset::TransformTarget;
use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::transform::resolve::resolve_target_in_handles;

#[derive(Debug, Clone)]
pub(crate) struct ScopedRegexRewrite {
    pub(crate) new_text: String,
    pub(crate) replacements: usize,
}

pub(crate) fn rewrite_node_target_with_scoped_regex(
    file: &Path,
    target: &TransformTarget,
    pattern: &str,
    replacement: &str,
    context: &ExecutionContext,
) -> Result<ScopedRegexRewrite, IdenteditError> {
    let regex = Regex::new(pattern).map_err(|error| IdenteditError::InvalidRequest {
        message: format!("Invalid scoped regex pattern: {error}"),
    })?;
    let source = context.read_file_bytes(file)?;
    let observation = context.observe_source(file, &source)?;
    let resolved = resolve_target_in_handles(file, observation.handles(), target)?;

    let replacements = regex.find_iter(&resolved.text).count();
    if replacements == 0 {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "Scoped regex matched 0 occurrences inside the resolved target span: /{pattern}/"
            ),
        });
    }

    let new_text = regex.replace_all(&resolved.text, replacement).into_owned();
    Ok(ScopedRegexRewrite {
        new_text,
        replacements,
    })
}
