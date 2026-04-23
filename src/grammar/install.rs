use std::fs;

use crate::error::IdenteditError;

use super::catalog::{InstallResolution, ResolutionSource, resolve_install_request};
use super::compile::{
    InstallWorkspace, clone_repo, compile_grammar_repository, ensure_grammar_install_supported,
    resolve_symbol, shared_library_filename,
};
use super::manifest::{ensure_grammars_dir, upsert_manifest_entry};
use super::{InstallGrammarRequest, InstalledGrammar};

pub(super) fn install_grammar(
    request: InstallGrammarRequest,
) -> Result<InstalledGrammar, IdenteditError> {
    ensure_grammar_install_supported()?;
    let resolution = resolve_install_request(&request)?;
    let grammars_dir = ensure_grammars_dir()?;
    let mut failures = Vec::new();

    for repo in &resolution.repo_candidates {
        let workspace = InstallWorkspace::new(&resolution.lang)?;
        let source_dir = workspace.path().join("source");
        let build_output = workspace
            .path()
            .join(shared_library_filename(&resolution.lang));

        if let Err(error) = clone_repo(repo, &source_dir) {
            failures.push(format!("{repo}: {error}"));
            continue;
        }

        if let Err(error) = compile_grammar_repository(&source_dir, &build_output) {
            failures.push(format!("{repo}: {error}"));
            continue;
        }

        let resolved_symbol = match resolve_symbol(&build_output, &resolution.symbol_candidates) {
            Ok(symbol) => symbol,
            Err(error) => {
                failures.push(format!("{repo}: {error}"));
                continue;
            }
        };

        let installed_path = grammars_dir.join(shared_library_filename(&resolution.lang));
        fs::copy(&build_output, &installed_path).map_err(|error| {
            IdenteditError::GrammarInstall {
                message: format!(
                    "failed to copy compiled grammar to '{}': {error}",
                    installed_path.display()
                ),
            }
        })?;

        let installed = InstalledGrammar {
            lang: resolution.lang.clone(),
            repo: repo.clone(),
            symbol: resolved_symbol,
            extensions: resolution.extensions.clone(),
            library_path: installed_path,
        };
        upsert_manifest_entry(&installed)?;
        return Ok(installed);
    }

    Err(IdenteditError::GrammarInstall {
        message: format!(
            "failed to install grammar '{}'. Attempts:\n{}\n{}",
            resolution.lang,
            failures.join("\n"),
            install_guidance(&resolution)
        ),
    })
}

fn install_guidance(resolution: &InstallResolution) -> String {
    match resolution.source {
        ResolutionSource::Builtin => {
            "You can override source details with --repo, --symbol, and --ext.".to_string()
        }
        ResolutionSource::Convention => {
            "Convention fallback failed. Retry with --repo and --symbol for explicit source details."
                .to_string()
        }
    }
}
