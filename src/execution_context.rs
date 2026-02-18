use std::path::Path;

use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::provider::ProviderRegistry;

/// Request-scoped execution context that owns shared runtime dependencies.
pub(crate) struct ExecutionContext {
    registry: ProviderRegistry,
}

impl ExecutionContext {
    pub(crate) fn new() -> Self {
        Self {
            registry: ProviderRegistry::default(),
        }
    }

    pub(crate) fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    pub(crate) fn parse_handles_for_file(
        &self,
        file: &Path,
    ) -> Result<Vec<SelectionHandle>, IdenteditError> {
        let source = self.read_file_bytes(file)?;
        self.parse_handles_for_source(file, &source)
    }

    pub(crate) fn parse_handles_for_source(
        &self,
        file: &Path,
        source: &[u8],
    ) -> Result<Vec<SelectionHandle>, IdenteditError> {
        let provider = self.registry.provider_for(file)?;
        provider.parse(file, source)
    }

    pub(crate) fn read_file_bytes(&self, file: &Path) -> Result<Vec<u8>, IdenteditError> {
        std::fs::read(file).map_err(|error| IdenteditError::io(file, error))
    }

    pub(crate) fn read_file_utf8(&self, file: &Path) -> Result<String, IdenteditError> {
        let source = self.read_file_bytes(file)?;
        String::from_utf8(source).map_err(|error| {
            IdenteditError::io(
                file,
                std::io::Error::new(std::io::ErrorKind::InvalidData, error),
            )
        })
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}
