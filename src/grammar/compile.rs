use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use libloading::Library;

use crate::error::IdenteditError;

pub(super) fn clone_repo(repo: &str, destination: &Path) -> Result<(), IdenteditError> {
    let output = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(repo)
        .arg(destination)
        .output()
        .map_err(|error| IdenteditError::GrammarInstall {
            message: format!("failed to invoke git clone for '{repo}': {error}"),
        })?;

    if output.status.success() {
        return Ok(());
    }

    Err(IdenteditError::GrammarInstall {
        message: format!(
            "git clone failed for '{repo}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    })
}

pub(super) fn compile_grammar_repository(
    source_dir: &Path,
    output_path: &Path,
) -> Result<(), IdenteditError> {
    ensure_grammar_install_supported()?;
    let src_dir = source_dir.join("src");
    let parser_path = src_dir.join("parser.c");
    if !parser_path.is_file() {
        return Err(IdenteditError::GrammarInstall {
            message: format!(
                "grammar source '{}' does not contain src/parser.c",
                source_dir.display()
            ),
        });
    }

    let scanner_c = src_dir.join("scanner.c");
    let scanner_cc = src_dir.join("scanner.cc");
    let scanner_cpp = src_dir.join("scanner.cpp");
    let has_cpp_scanner = scanner_cc.is_file() || scanner_cpp.is_file();
    let compiler = if has_cpp_scanner { "c++" } else { "cc" };

    let mut command = Command::new(compiler);
    command.arg("-O2");
    command.arg("-fPIC");
    command.arg("-I");
    command.arg(&src_dir);
    command.arg(&parser_path);

    if scanner_c.is_file() {
        command.arg(&scanner_c);
    }
    if scanner_cc.is_file() {
        command.arg("-std=c++17");
        command.arg(&scanner_cc);
    }
    if scanner_cpp.is_file() {
        command.arg("-std=c++17");
        command.arg(&scanner_cpp);
    }

    append_shared_library_link_flags(&mut command);

    command.arg("-o");
    command.arg(output_path);

    let output = command
        .output()
        .map_err(|error| IdenteditError::GrammarInstall {
            message: format!("failed to invoke '{compiler}' while building grammar: {error}"),
        })?;

    if output.status.success() {
        return Ok(());
    }

    Err(IdenteditError::GrammarInstall {
        message: format!(
            "grammar compilation failed with '{compiler}': {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    })
}

pub(super) fn resolve_symbol(
    library_path: &Path,
    candidates: &[String],
) -> Result<String, IdenteditError> {
    let library =
        unsafe { Library::new(library_path) }.map_err(|error| IdenteditError::GrammarInstall {
            message: format!(
                "failed to open compiled grammar library '{}': {error}",
                library_path.display()
            ),
        })?;

    for candidate in candidates {
        let symbol =
            unsafe { library.get::<unsafe extern "C" fn() -> *const ()>(candidate.as_bytes()) };
        if symbol.is_ok() {
            return Ok(candidate.clone());
        }
    }

    Err(IdenteditError::GrammarInstall {
        message: format!(
            "none of the symbol candidates were found in '{}': {}",
            library_path.display(),
            candidates.join(", ")
        ),
    })
}

pub(super) fn ensure_grammar_install_supported() -> Result<(), IdenteditError> {
    #[cfg(target_os = "windows")]
    {
        Err(IdenteditError::GrammarInstall {
            message: "grammar install is not yet supported on Windows hosts. Use bundled grammars or run install on macOS/Linux and copy the compiled library and manifest entry.".to_string(),
        })
    }

    #[cfg(all(
        not(target_os = "macos"),
        not(target_os = "linux"),
        not(target_os = "windows")
    ))]
    {
        Err(IdenteditError::GrammarInstall {
            message: "grammar install is currently supported only on macOS and Linux hosts"
                .to_string(),
        })
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn append_shared_library_link_flags(command: &mut Command) {
    command.arg("-dynamiclib");
}

#[cfg(target_os = "linux")]
fn append_shared_library_link_flags(command: &mut Command) {
    command.arg("-shared");
}

#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
fn append_shared_library_link_flags(_command: &mut Command) {}

pub(super) fn shared_library_filename(lang: &str) -> String {
    let sanitized = sanitize_filename(lang);
    format!("{sanitized}.{}", shared_library_extension())
}

fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn shared_library_extension() -> &'static str {
    "dylib"
}

#[cfg(target_os = "linux")]
fn shared_library_extension() -> &'static str {
    "so"
}

#[cfg(target_os = "windows")]
fn shared_library_extension() -> &'static str {
    "dll"
}

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "linux"),
    not(target_os = "windows")
))]
fn shared_library_extension() -> &'static str {
    "so"
}

pub(super) struct InstallWorkspace {
    path: PathBuf,
}

impl InstallWorkspace {
    pub(super) fn new(lang: &str) -> Result<Self, IdenteditError> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| IdenteditError::GrammarInstall {
                message: format!("system clock error while preparing install workspace: {error}"),
            })?
            .as_nanos();
        let path = env::temp_dir().join(format!("identedit-grammar-install-{lang}-{nonce}"));
        fs::create_dir_all(&path).map_err(|error| IdenteditError::GrammarInstall {
            message: format!(
                "failed to create install workspace '{}': {error}",
                path.display()
            ),
        })?;
        Ok(Self { path })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for InstallWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_grammar_install_supported, shared_library_extension};

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn grammar_install_support_check_allows_supported_hosts() {
        ensure_grammar_install_supported().expect("host should support grammar install");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn grammar_install_support_check_rejects_windows_hosts() {
        let error =
            ensure_grammar_install_supported().expect_err("Windows hosts should be rejected");
        match error {
            crate::error::IdenteditError::GrammarInstall { message } => {
                assert!(message.contains("not yet supported on Windows"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn shared_library_extension_is_dll_on_windows() {
        assert_eq!(shared_library_extension(), "dll");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn shared_library_extension_is_dylib_on_macos() {
        assert_eq!(shared_library_extension(), "dylib");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn shared_library_extension_is_so_on_linux() {
        assert_eq!(shared_library_extension(), "so");
    }
}
