use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::{Builder, TempDir};

use crate::error::IdenteditError;

use super::catalog::{InstallResolution, InstallSources, resolve_install_request};
use super::compile::{
    compile_grammar_repository, ensure_grammar_install_supported, resolve_symbol,
    shared_library_filename,
};
use super::manifest::{ensure_grammars_dir, upsert_manifest_entry};
use super::source::acquire_source;
use super::{InstallGrammarRequest, InstalledGrammar};

pub(super) fn install_grammar(
    request: InstallGrammarRequest,
) -> Result<InstalledGrammar, IdenteditError> {
    ensure_grammar_install_supported()?;
    let resolution = resolve_install_request(&request)?;
    let grammars_dir = ensure_grammars_dir()?;

    let mut failures = Vec::new();

    for source in resolution.sources.candidates() {
        let repo = source.repo();
        let workspace = create_install_workspace()?;
        let source_dir = workspace.path().join("source");
        let build_output = workspace
            .path()
            .join(shared_library_filename(&resolution.lang));

        if let Err(error) = acquire_source(source, workspace.path(), &source_dir) {
            failures.push(format!("{repo}: {error}"));
            continue;
        }

        if let Err(error) = compile_grammar_repository(&source_dir, &build_output) {
            failures.push(format!("{repo}: build: {error}"));
            continue;
        }

        let resolved_symbol = match resolve_symbol(&build_output, &resolution.symbol_candidates) {
            Ok(symbol) => symbol,
            Err(error) => {
                failures.push(format!("{repo}: symbol: {error}"));
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
            repo: repo.to_owned(),
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

fn create_install_workspace() -> Result<TempDir, IdenteditError> {
    let mut builder = Builder::new();
    builder.prefix("identedit-grammar-install-");
    #[cfg(unix)]
    builder.permissions(fs::Permissions::from_mode(0o700));

    builder
        .tempdir()
        .map_err(|error| IdenteditError::GrammarInstall {
            message: format!("failed to create install workspace: {error}"),
        })
}

fn install_guidance(resolution: &InstallResolution<'_>) -> &'static str {
    match resolution.sources {
        InstallSources::Pinned(_) => "Use --repo to install from a different repository you trust.",
        InstallSources::Explicit(_) => {
            "Check the --repo source and use --symbol if it exports a different language symbol."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_workspaces_are_private_and_cleaned_up_independently() {
        let first = create_install_workspace().unwrap();
        let second = create_install_workspace().unwrap();
        let first_path = first.path().to_owned();
        let second_file = second.path().join("kept");

        assert!(first.path().is_absolute());
        assert_ne!(first.path(), second.path());
        #[cfg(unix)]
        for workspace in [&first, &second] {
            let mode = fs::metadata(workspace.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0);
        }
        fs::write(first.path().join("staged"), b"first attempt").unwrap();
        fs::write(&second_file, b"second attempt").unwrap();

        drop(first);

        assert!(!first_path.try_exists().unwrap());
        assert_eq!(fs::read(second_file).unwrap(), b"second attempt");
    }
}
