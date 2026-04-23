use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::IdenteditError;

mod catalog;
mod compile;
mod install;
mod manifest;

pub use manifest::installed_grammars_for_runtime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstalledGrammar {
    pub lang: String,
    pub repo: String,
    pub symbol: String,
    pub extensions: Vec<String>,
    pub library_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct GrammarInstallResponse {
    pub installed: InstalledGrammar,
}

#[derive(Debug, Clone)]
pub struct InstallGrammarRequest {
    pub lang: String,
    pub repo: Option<String>,
    pub symbol: Option<String>,
    pub extensions: Vec<String>,
}

pub fn install_grammar(request: InstallGrammarRequest) -> Result<InstalledGrammar, IdenteditError> {
    install::install_grammar(request)
}
