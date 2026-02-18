use std::path::Path;

use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::handle::SelectionHandle;
use crate::provider::ProviderRegistry;

pub(super) fn parse_handles_for_file(file: &Path) -> Result<Vec<SelectionHandle>, IdenteditError> {
    let context = ExecutionContext::new();
    parse_handles_for_file_with_context(file, &context)
}

pub(super) fn parse_handles_for_file_with_context(
    file: &Path,
    context: &ExecutionContext,
) -> Result<Vec<SelectionHandle>, IdenteditError> {
    context.parse_handles_for_file(file)
}

pub(super) fn parse_handles_for_source(
    file: &Path,
    source: &[u8],
) -> Result<Vec<SelectionHandle>, IdenteditError> {
    let context = ExecutionContext::new();
    parse_handles_for_source_with_context(file, source, &context)
}

pub(super) fn parse_handles_for_source_with_context(
    file: &Path,
    source: &[u8],
    context: &ExecutionContext,
) -> Result<Vec<SelectionHandle>, IdenteditError> {
    context.parse_handles_for_source(file, source)
}

pub(super) fn parse_handles_for_source_with_registry(
    file: &Path,
    source: &[u8],
    registry: &ProviderRegistry,
) -> Result<Vec<SelectionHandle>, IdenteditError> {
    let provider = registry.provider_for(file)?;
    provider.parse(file, source)
}
