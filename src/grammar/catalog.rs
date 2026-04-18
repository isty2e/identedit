use crate::error::IdenteditError;

use super::InstallGrammarRequest;

#[derive(Debug, Clone, Copy)]
pub(super) enum ResolutionSource {
    Builtin,
    Convention,
}

#[derive(Debug, Clone)]
pub(super) struct InstallResolution {
    pub source: ResolutionSource,
    pub lang: String,
    pub repo_candidates: Vec<String>,
    pub symbol_candidates: Vec<String>,
    pub extensions: Vec<String>,
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

pub(super) fn resolve_install_request(
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

#[cfg(test)]
mod tests {
    use super::{InstallResolution, ResolutionSource, resolve_install_request};
    use crate::grammar::InstallGrammarRequest;

    #[test]
    fn resolve_builtin_language_uses_default_extensions() {
        let request = InstallGrammarRequest {
            lang: "toml".to_string(),
            repo: None,
            symbol: None,
            extensions: Vec::new(),
        };
        let resolved: InstallResolution =
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
}
