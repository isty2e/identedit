use std::path::Path;

use crate::error::IdenteditError;
use crate::handle::{SelectionHandle, Span};
use crate::provider::StructureProvider;

mod boundary;
mod brace_lex;
mod detection;
mod javascript_mask;
mod patterns;
mod python_mask;
mod source_lines;

use boundary::infer_candidate_end;
use detection::detect_candidates;
use source_lines::collect_lines;

pub struct FallbackProvider;

impl StructureProvider for FallbackProvider {
    fn parse(&self, path: &Path, source: &[u8]) -> Result<Vec<SelectionHandle>, IdenteditError> {
        let source_text =
            std::str::from_utf8(source).map_err(|_| IdenteditError::ParseFailure {
                provider: self.name(),
                message: "Fallback provider requires UTF-8 text input".to_string(),
            })?;
        let lines = collect_lines(source_text);
        let mut handles = Vec::new();

        for candidate in detect_candidates(source_text.as_bytes(), &lines) {
            let start = lines[candidate.start_line_index].start;
            let line_end = lines[candidate.boundary_line_index].end;
            let end = infer_candidate_end(
                source_text.as_bytes(),
                &lines,
                candidate.boundary_line_index,
                candidate.boundary,
                line_end,
            );
            if end <= start {
                continue;
            }

            let Some(text) = source_text.get(start..end) else {
                continue;
            };
            if text.trim().is_empty() {
                continue;
            }

            handles.push(SelectionHandle::from_parts(
                path.to_path_buf(),
                Span { start, end },
                candidate.kind.to_string(),
                Some(candidate.name),
                text.to_string(),
            ));
        }

        Ok(handles)
    }

    fn can_handle(&self, _path: &Path) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "fallback"
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod tests;
