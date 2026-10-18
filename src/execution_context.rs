use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::changeset::FileChange;
use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::provider::ProviderRegistry;

/// Runtime dependencies and source observations scoped to one execution.
pub(crate) struct ExecutionContext {
    registry: ProviderRegistry,
    last_source: RefCell<Option<Rc<SourceObservation>>>,
}

pub(crate) struct SourceObservation {
    file: PathBuf,
    source: Box<[u8]>,
    handles: Vec<SelectionHandle>,
}

impl SourceObservation {
    fn matches(&self, file: &Path, source: &[u8]) -> bool {
        self.file.as_os_str() == file.as_os_str() && self.source.as_ref() == source
    }

    pub(crate) fn handles(&self) -> &[SelectionHandle] {
        &self.handles
    }
}

impl ExecutionContext {
    pub(crate) fn new() -> Self {
        Self::with_registry(ProviderRegistry::default())
    }

    pub(crate) fn with_registry(registry: ProviderRegistry) -> Self {
        Self {
            registry,
            last_source: RefCell::new(None),
        }
    }

    pub(crate) fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    pub(crate) fn observe_source(
        &self,
        file: &Path,
        source: &[u8],
    ) -> Result<Rc<SourceObservation>, IdenteditError> {
        // The registry is fixed for this context; no observation crosses that boundary.
        if let Some(observation) = self.last_source.borrow().as_ref()
            && observation.matches(file, source)
        {
            return Ok(Rc::clone(observation));
        }

        self.last_source.replace(None);
        let handles = self.parse_handles_for_source(file, source)?;
        let observation = Rc::new(SourceObservation {
            file: file.to_path_buf(),
            source: source.into(),
            handles,
        });
        self.last_source.replace(Some(Rc::clone(&observation)));
        Ok(observation)
    }

    pub(crate) fn clear_source_observation(&self) {
        self.last_source.replace(None);
    }

    #[cfg(test)]
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
        Self::decode_file_utf8(file, source)
    }

    pub(crate) fn read_changeset_text(
        &self,
        changeset: &FileChange,
    ) -> Result<String, IdenteditError> {
        let source = self.read_file_bytes(&changeset.file)?;
        changeset.verify_source(&source)?;
        Self::decode_file_utf8(&changeset.file, source)
    }

    fn decode_file_utf8(file: &Path, source: Vec<u8>) -> Result<String, IdenteditError> {
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

#[cfg(test)]
mod tests;
