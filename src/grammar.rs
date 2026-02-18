use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use libloading::Library;
use serde::{Deserialize, Serialize};

use crate::error::IdenteditError;

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

#[derive(Debug, Clone, Copy)]
enum ResolutionSource {
    Builtin,
    Convention,
}

#[derive(Debug, Clone)]
struct InstallResolution {
    source: ResolutionSource,
    lang: String,
    repo_candidates: Vec<String>,
    symbol_candidates: Vec<String>,
    extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct GrammarManifest {
    grammars: Vec<InstalledGrammar>,
}

#[derive(Debug, Clone, Copy)]
struct BuiltinGrammar {
    lang: &'static str,
    extensions: &'static [&'static str],
}

const BUILTIN_GRAMMARS: &[BuiltinGrammar] = &[
    BuiltinGrammar {
        lang: "toml",
        extensions: &["toml"],
    },
    BuiltinGrammar {
        lang: "yaml",
        extensions: &["yaml", "yml"],
    },
    BuiltinGrammar {
        lang: "bash",
        extensions: &["sh", "bash"],
    },
    BuiltinGrammar {
        lang: "c",
        extensions: &["c", "h"],
    },
    BuiltinGrammar {
        lang: "cpp",
        extensions: &["cc", "cpp", "cxx", "hpp", "hxx"],
    },
    BuiltinGrammar {
        lang: "css",
        extensions: &["css"],
    },
    BuiltinGrammar {
        lang: "dockerfile",
        extensions: &["dockerfile"],
    },
    BuiltinGrammar {
        lang: "elixir",
        extensions: &["ex", "exs"],
    },
    BuiltinGrammar {
        lang: "elm",
        extensions: &["elm"],
    },
    BuiltinGrammar {
        lang: "erlang",
        extensions: &["erl", "hrl"],
    },
    BuiltinGrammar {
        lang: "gitcommit",
        extensions: &["gitcommit"],
    },
    BuiltinGrammar {
        lang: "gitignore",
        extensions: &["gitignore"],
    },
    BuiltinGrammar {
        lang: "gleam",
        extensions: &["gleam"],
    },
    BuiltinGrammar {
        lang: "graphql",
        extensions: &["graphql", "gql"],
    },
    BuiltinGrammar {
        lang: "haskell",
        extensions: &["hs"],
    },
    BuiltinGrammar {
        lang: "hcl",
        extensions: &["hcl", "tf"],
    },
    BuiltinGrammar {
        lang: "html",
        extensions: &["html", "htm"],
    },
    BuiltinGrammar {
        lang: "ini",
        extensions: &["ini", "cfg", "conf"],
    },
    BuiltinGrammar {
        lang: "java",
        extensions: &["java"],
    },
    BuiltinGrammar {
        lang: "json",
        extensions: &["json"],
    },
    BuiltinGrammar {
        lang: "json5",
        extensions: &["json5"],
    },
    BuiltinGrammar {
        lang: "julia",
        extensions: &["jl"],
    },
    BuiltinGrammar {
        lang: "kotlin",
        extensions: &["kt", "kts"],
    },
    BuiltinGrammar {
        lang: "lua",
        extensions: &["lua"],
    },
    BuiltinGrammar {
        lang: "make",
        extensions: &["mk", "makefile"],
    },
    BuiltinGrammar {
        lang: "markdown",
        extensions: &["md", "markdown"],
    },
    BuiltinGrammar {
        lang: "meson",
        extensions: &["meson", "meson.build", "meson_options.txt"],
    },
    BuiltinGrammar {
        lang: "nix",
        extensions: &["nix"],
    },
    BuiltinGrammar {
        lang: "ocaml",
        extensions: &["ml", "mli"],
    },
    BuiltinGrammar {
        lang: "perl",
        extensions: &["pl", "pm"],
    },
    BuiltinGrammar {
        lang: "php",
        extensions: &["php"],
    },
    BuiltinGrammar {
        lang: "proto",
        extensions: &["proto"],
    },
    BuiltinGrammar {
        lang: "python",
        extensions: &["py", "pyi", "pyw"],
    },
    BuiltinGrammar {
        lang: "r",
        extensions: &["r", "R"],
    },
    BuiltinGrammar {
        lang: "regex",
        extensions: &["regex"],
    },
    BuiltinGrammar {
        lang: "ruby",
        extensions: &["rb"],
    },
    BuiltinGrammar {
        lang: "rust",
        extensions: &["rs"],
    },
    BuiltinGrammar {
        lang: "scala",
        extensions: &["scala"],
    },
    BuiltinGrammar {
        lang: "sql",
        extensions: &["sql"],
    },
    BuiltinGrammar {
        lang: "svelte",
        extensions: &["svelte"],
    },
    BuiltinGrammar {
        lang: "swift",
        extensions: &["swift"],
    },
    BuiltinGrammar {
        lang: "tsx",
        extensions: &["tsx"],
    },
    BuiltinGrammar {
        lang: "typescript",
        extensions: &["ts"],
    },
    BuiltinGrammar {
        lang: "javascript",
        extensions: &["js", "jsx", "mjs", "cjs"],
    },
    BuiltinGrammar {
        lang: "vue",
        extensions: &["vue"],
    },
    BuiltinGrammar {
        lang: "xml",
        extensions: &["xml"],
    },
    BuiltinGrammar {
        lang: "zig",
        extensions: &["zig"],
    },
    BuiltinGrammar {
        lang: "astro",
        extensions: &["astro"],
    },
    BuiltinGrammar {
        lang: "clojure",
        extensions: &["clj", "cljs", "cljc"],
    },
    BuiltinGrammar {
        lang: "cmake",
        extensions: &["cmake", "cmakelists.txt"],
    },
    BuiltinGrammar {
        lang: "commonlisp",
        extensions: &["lisp", "cl", "el"],
    },
    BuiltinGrammar {
        lang: "cuda",
        extensions: &["cu", "cuh"],
    },
    BuiltinGrammar {
        lang: "dart",
        extensions: &["dart"],
    },
    BuiltinGrammar {
        lang: "fsharp",
        extensions: &["fs", "fsi", "fsx"],
    },
    BuiltinGrammar {
        lang: "fortran",
        extensions: &["f", "f90", "f95"],
    },
    BuiltinGrammar {
        lang: "go",
        extensions: &["go"],
    },
    BuiltinGrammar {
        lang: "groovy",
        extensions: &["groovy"],
    },
    BuiltinGrammar {
        lang: "hack",
        extensions: &["hack", "hh", "hhi"],
    },
    BuiltinGrammar {
        lang: "latex",
        extensions: &["tex"],
    },
    BuiltinGrammar {
        lang: "liquid",
        extensions: &["liquid"],
    },
    BuiltinGrammar {
        lang: "matlab",
        extensions: &["m"],
    },
    BuiltinGrammar {
        lang: "org",
        extensions: &["org"],
    },
    BuiltinGrammar {
        lang: "pascal",
        extensions: &["pas", "pp"],
    },
    BuiltinGrammar {
        lang: "purescript",
        extensions: &["purs"],
    },
    BuiltinGrammar {
        lang: "racket",
        extensions: &["rkt"],
    },
    BuiltinGrammar {
        lang: "scheme",
        extensions: &["scm", "ss"],
    },
    BuiltinGrammar {
        lang: "solidity",
        extensions: &["sol"],
    },
    BuiltinGrammar {
        lang: "sparql",
        extensions: &["sparql", "rq"],
    },
    BuiltinGrammar {
        lang: "terraform",
        extensions: &["tf", "tfvars"],
    },
    BuiltinGrammar {
        lang: "todotxt",
        extensions: &["todo"],
    },
    BuiltinGrammar {
        lang: "typst",
        extensions: &["typ"],
    },
    BuiltinGrammar {
        lang: "wgsl",
        extensions: &["wgsl"],
    },
];

pub fn install_grammar(request: InstallGrammarRequest) -> Result<InstalledGrammar, IdenteditError> {
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

    let guidance = match resolution.source {
        ResolutionSource::Builtin => {
            "You can override source details with --repo, --symbol, and --ext.".to_string()
        }
        ResolutionSource::Convention => {
            "Convention fallback failed. Retry with --repo and --symbol for explicit source details."
                .to_string()
        }
    };

    Err(IdenteditError::GrammarInstall {
        message: format!(
            "failed to install grammar '{}'. Attempts:\n{}\n{}",
            resolution.lang,
            failures.join("\n"),
            guidance
        ),
    })
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

fn resolve_install_request(
    request: &InstallGrammarRequest,
) -> Result<InstallResolution, IdenteditError> {
    let lang = normalize_language_name(&request.lang)?;
    let maybe_builtin = BUILTIN_GRAMMARS.iter().find(|entry| entry.lang == lang);

    let source = if maybe_builtin.is_some() {
        ResolutionSource::Builtin
    } else {
        ResolutionSource::Convention
    };

    if maybe_builtin.is_none() && request.extensions.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: format!(
                "--ext is required for convention fallback languages ('{}' is not in the built-in registry)",
                lang
            ),
        });
    }

    let extensions = if request.extensions.is_empty() {
        maybe_builtin
            .expect("builtin must exist when extension override is empty")
            .extensions
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    } else {
        normalize_extensions(&request.extensions)?
    };

    let repo_candidates = if let Some(repo) = &request.repo {
        vec![repo.clone()]
    } else {
        default_repository_candidates(&lang)
    };

    let symbol_candidates = if let Some(symbol) = &request.symbol {
        vec![symbol.clone()]
    } else {
        default_symbol_candidates(&lang)
    };

    Ok(InstallResolution {
        source,
        lang,
        repo_candidates,
        symbol_candidates,
        extensions,
    })
}

fn normalize_language_name(value: &str) -> Result<String, IdenteditError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: "language name must not be empty".to_string(),
        });
    }

    if trimmed.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(IdenteditError::InvalidRequest {
            message: format!("language name '{}' must not contain whitespace", trimmed),
        });
    }

    Ok(trimmed.to_ascii_lowercase())
}

fn normalize_extensions(values: &[String]) -> Result<Vec<String>, IdenteditError> {
    let mut normalized = Vec::new();

    for value in values {
        let trimmed = value.trim().trim_start_matches('.');
        if trimmed.is_empty() {
            return Err(IdenteditError::InvalidRequest {
                message: "extension values passed to --ext must not be empty".to_string(),
            });
        }
        normalized.push(trimmed.to_ascii_lowercase());
    }

    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn default_repository_candidates(lang: &str) -> Vec<String> {
    vec![
        format!("https://github.com/tree-sitter/tree-sitter-{lang}.git"),
        format!("https://github.com/tree-sitter-grammars/tree-sitter-{lang}.git"),
    ]
}

fn default_symbol_candidates(lang: &str) -> Vec<String> {
    let raw = format!("tree_sitter_{lang}");
    let underscored = format!("tree_sitter_{}", lang.replace('-', "_"));
    if raw == underscored {
        vec![raw]
    } else {
        vec![raw, underscored]
    }
}

fn resolve_symbol(library_path: &Path, candidates: &[String]) -> Result<String, IdenteditError> {
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

fn clone_repo(repo: &str, destination: &Path) -> Result<(), IdenteditError> {
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

fn compile_grammar_repository(source_dir: &Path, output_path: &Path) -> Result<(), IdenteditError> {
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

fn ensure_grammar_install_supported() -> Result<(), IdenteditError> {
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

fn ensure_grammars_dir() -> Result<PathBuf, IdenteditError> {
    let path = grammars_dir()?;
    fs::create_dir_all(&path).map_err(|error| IdenteditError::GrammarInstall {
        message: format!(
            "failed to create grammar directory '{}': {error}",
            path.display()
        ),
    })?;
    Ok(path)
}

fn upsert_manifest_entry(entry: &InstalledGrammar) -> Result<(), IdenteditError> {
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

fn shared_library_filename(lang: &str) -> String {
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

struct InstallWorkspace {
    path: PathBuf,
}

impl InstallWorkspace {
    fn new(lang: &str) -> Result<Self, IdenteditError> {
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

    fn path(&self) -> &Path {
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
    use super::{
        InstallGrammarRequest, ResolutionSource, ensure_grammar_install_supported,
        resolve_install_request, shared_library_extension,
    };

    #[test]
    fn resolve_builtin_language_uses_default_extensions() {
        let request = InstallGrammarRequest {
            lang: "toml".to_string(),
            repo: None,
            symbol: None,
            extensions: Vec::new(),
        };
        let resolved =
            resolve_install_request(&request).expect("builtin resolution should succeed");

        assert!(matches!(resolved.source, ResolutionSource::Builtin));
        assert_eq!(resolved.lang, "toml");
        assert_eq!(resolved.extensions, vec!["toml".to_string()]);
        assert_eq!(resolved.repo_candidates.len(), 2);
        assert_eq!(resolved.symbol_candidates[0], "tree_sitter_toml");
    }

    #[test]
    fn resolve_convention_fallback_requires_extension_override() {
        let request = InstallGrammarRequest {
            lang: "unknownlang".to_string(),
            repo: None,
            symbol: None,
            extensions: Vec::new(),
        };
        let error = resolve_install_request(&request)
            .expect_err("convention fallback without --ext should fail");

        match error {
            crate::error::IdenteditError::InvalidRequest { message } => {
                assert!(message.contains("--ext is required"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn resolve_convention_fallback_uses_repo_and_symbol_candidates() {
        let request = InstallGrammarRequest {
            lang: "foo-bar".to_string(),
            repo: None,
            symbol: None,
            extensions: vec!["foo".to_string()],
        };
        let resolved = resolve_install_request(&request).expect("resolution should succeed");

        assert!(matches!(resolved.source, ResolutionSource::Convention));
        assert_eq!(
            resolved.repo_candidates,
            vec![
                "https://github.com/tree-sitter/tree-sitter-foo-bar.git".to_string(),
                "https://github.com/tree-sitter-grammars/tree-sitter-foo-bar.git".to_string()
            ]
        );
        assert_eq!(
            resolved.symbol_candidates,
            vec![
                "tree_sitter_foo-bar".to_string(),
                "tree_sitter_foo_bar".to_string()
            ]
        );
    }

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
