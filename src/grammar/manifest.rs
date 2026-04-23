use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::IdenteditError;

use super::InstalledGrammar;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct GrammarManifest {
    grammars: Vec<InstalledGrammar>,
}

pub fn installed_grammars_for_runtime() -> Vec<InstalledGrammar> {
    let Ok(path) = manifest_path() else {
        return Vec::new();
    };
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<GrammarManifest>(&content) else {
        return Vec::new();
    };

    manifest
        .grammars
        .into_iter()
        .filter(|entry| entry.library_path.is_file())
        .collect()
}

pub(super) fn ensure_grammars_dir() -> Result<PathBuf, IdenteditError> {
    let path = grammars_dir()?;
    fs::create_dir_all(&path).map_err(|error| IdenteditError::GrammarInstall {
        message: format!(
            "failed to create grammar directory '{}': {error}",
            path.display()
        ),
    })?;
    Ok(path)
}

pub(super) fn upsert_manifest_entry(entry: &InstalledGrammar) -> Result<(), IdenteditError> {
    let path = manifest_path()?;
    let mut manifest = if path.is_file() {
        let content =
            fs::read_to_string(&path).map_err(|error| IdenteditError::GrammarInstall {
                message: format!(
                    "failed to read grammar manifest '{}': {error}",
                    path.display()
                ),
            })?;
        serde_json::from_str::<GrammarManifest>(&content).map_err(|error| {
            IdenteditError::GrammarInstall {
                message: format!(
                    "failed to parse grammar manifest '{}': {error}",
                    path.display()
                ),
            }
        })?
    } else {
        GrammarManifest::default()
    };

    if let Some(position) = manifest
        .grammars
        .iter()
        .position(|item| item.lang == entry.lang)
    {
        manifest.grammars[position] = entry.clone();
    } else {
        manifest.grammars.push(entry.clone());
    }

    manifest
        .grammars
        .sort_by(|left, right| left.lang.cmp(&right.lang));

    let serialized = serde_json::to_string_pretty(&manifest).map_err(|error| {
        IdenteditError::GrammarInstall {
            message: format!("failed to serialize grammar manifest: {error}"),
        }
    })?;
    fs::write(&path, serialized).map_err(|error| IdenteditError::GrammarInstall {
        message: format!(
            "failed to write grammar manifest '{}': {error}",
            path.display()
        ),
    })?;

    Ok(())
}

fn manifest_path() -> Result<PathBuf, IdenteditError> {
    Ok(grammars_dir()?.join("manifest.json"))
}

fn grammars_dir() -> Result<PathBuf, IdenteditError> {
    if let Some(value) = env::var_os("IDENTEDIT_HOME") {
        return Ok(PathBuf::from(value).join("grammars"));
    }

    let home = default_home_dir().ok_or_else(|| IdenteditError::GrammarInstall {
        message: "home directory is not set (expected HOME on Unix or USERPROFILE/HOMEDRIVE+HOMEPATH on Windows) and IDENTEDIT_HOME override was not provided".to_string(),
    })?;
    Ok(PathBuf::from(home).join(".identedit").join("grammars"))
}

#[cfg(not(target_os = "windows"))]
fn default_home_dir() -> Option<std::ffi::OsString> {
    env::var_os("HOME")
}

#[cfg(target_os = "windows")]
fn default_home_dir() -> Option<std::ffi::OsString> {
    if let Some(value) = env::var_os("USERPROFILE") {
        return Some(value);
    }

    let home_drive = env::var_os("HOMEDRIVE")?;
    let home_path = env::var_os("HOMEPATH")?;
    let mut combined = PathBuf::from(home_drive);
    combined.push(home_path);
    Some(combined.into_os_string())
}
