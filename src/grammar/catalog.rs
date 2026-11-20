use crate::error::IdenteditError;

use super::InstallGrammarRequest;

#[derive(Debug, Clone, Copy)]
pub(super) struct PinnedSource<'a> {
    pub repo: &'a str,
    pub commit: &'a str,
}

#[derive(Debug)]
pub(super) enum InstallSources<'a> {
    Pinned(&'a [PinnedSource<'a>]),
    Explicit(&'a str),
}

#[derive(Clone, Copy)]
pub(super) enum InstallSource<'a> {
    Pinned(PinnedSource<'a>),
    Explicit(&'a str),
}

impl<'a> InstallSource<'a> {
    pub fn repo(self) -> &'a str {
        match self {
            Self::Pinned(pin) => pin.repo,
            Self::Explicit(repo) => repo,
        }
    }
}

impl<'a> InstallSources<'a> {
    pub fn candidates(&self) -> impl Iterator<Item = InstallSource<'a>> + use<'a> {
        let (explicit, pins): (_, &[_]) = match *self {
            Self::Explicit(repo) => (Some(InstallSource::Explicit(repo)), &[]),
            Self::Pinned(pins) => (None, pins),
        };
        explicit
            .into_iter()
            .chain(pins.iter().copied().map(InstallSource::Pinned))
    }
}

#[derive(Debug)]
pub(super) struct InstallResolution<'a> {
    pub sources: InstallSources<'a>,
    pub lang: String,
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
) -> Result<InstallResolution<'_>, IdenteditError> {
    let lang = normalize_language_name(&request.lang)?;
    let maybe_builtin = BUILTIN_GRAMMARS.iter().find(|entry| entry.lang == lang);

    if maybe_builtin.is_none() && request.extensions.is_empty() {
        return Err(IdenteditError::InvalidRequest {
            message: format!("--ext is required for '{lang}' because it has no default extensions"),
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

    let sources = if let Some(repo) = request.repo.as_deref() {
        InstallSources::Explicit(repo)
    } else {
        let pins = pinned_sources(&lang);
        if pins.is_empty() {
            return Err(IdenteditError::InvalidRequest {
                message: format!(
                    "No pinned grammar for '{lang}'; use --repo with a repository you trust. \
                     Convention-based suggestions (not verified): \
                     https://github.com/tree-sitter/tree-sitter-{lang}, \
                     https://github.com/tree-sitter-grammars/tree-sitter-{lang}."
                ),
            });
        }
        InstallSources::Pinned(pins)
    };

    let symbol_candidates = if let Some(symbol) = &request.symbol {
        vec![symbol.clone()]
    } else {
        default_symbol_candidates(&lang)
    };

    Ok(InstallResolution {
        sources,
        lang,
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

fn pinned_sources(lang: &str) -> &'static [PinnedSource<'static>] {
    match lang {
        "elixir" => &[PinnedSource {
            repo: "https://github.com/elixir-lang/tree-sitter-elixir.git",
            commit: "cb32ccbe17c67037b09e31bf9beb22cdae6ed8e9",
        }],
        "zig" => &[PinnedSource {
            repo: "https://github.com/tree-sitter-grammars/tree-sitter-zig.git",
            commit: "6479aa13f32f701c383083d8b28360ebd682fb7d",
        }],
        "dart" => &[PinnedSource {
            repo: "https://github.com/UserNobody14/tree-sitter-dart.git",
            commit: "0038280b774b65c1b89ba7fec50624d8baed9fe1",
        }],
        _ => &[],
    }
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
    use super::{InstallSource, InstallSources, PinnedSource, resolve_install_request};
    use crate::grammar::InstallGrammarRequest;

    #[test]
    fn resolve_pinned_language_uses_default_extensions() {
        let request = InstallGrammarRequest {
            lang: "Elixir".to_string(),
            repo: None,
            symbol: None,
            extensions: Vec::new(),
        };
        let resolved = resolve_install_request(&request).unwrap();

        assert!(matches!(resolved.sources, InstallSources::Pinned([_])));
        assert_eq!(resolved.lang, "elixir");
        assert_eq!(resolved.extensions, ["ex", "exs"]);
        assert_eq!(resolved.symbol_candidates, ["tree_sitter_elixir"]);
    }

    #[test]
    fn resolve_unknown_language_requires_extension_override() {
        let request = InstallGrammarRequest {
            lang: "unknownlang".to_string(),
            repo: None,
            symbol: None,
            extensions: Vec::new(),
        };
        let error = resolve_install_request(&request)
            .expect_err("a language without default extensions needs --ext");

        match error {
            crate::error::IdenteditError::InvalidRequest { message } => {
                assert!(message.contains("--ext is required"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn explicit_repository_overrides_pins_without_discarding_routing_defaults() {
        let request = InstallGrammarRequest {
            lang: "zig".to_string(),
            repo: Some("local grammar".to_string()),
            symbol: None,
            extensions: Vec::new(),
        };
        let resolved = resolve_install_request(&request).unwrap();

        let mut candidates = resolved.sources.candidates();
        assert!(matches!(
            candidates.next(),
            Some(InstallSource::Explicit("local grammar"))
        ));
        assert!(candidates.next().is_none());
        assert_eq!(resolved.extensions, ["zig"]);
        assert_eq!(resolved.symbol_candidates, ["tree_sitter_zig"]);
    }

    #[test]
    fn catalog_fallback_keeps_each_candidate_with_its_pin() {
        let pins = [
            PinnedSource {
                repo: "first grammar",
                commit: "1111111111111111111111111111111111111111",
            },
            PinnedSource {
                repo: "second grammar",
                commit: "2222222222222222222222222222222222222222",
            },
        ];
        let sources = InstallSources::Pinned(&pins);
        let resolved = sources
            .candidates()
            .map(|source| match source {
                InstallSource::Pinned(pin) => (pin.repo, pin.commit),
                InstallSource::Explicit(_) => panic!("only pins may enter catalog fallback"),
            })
            .collect::<Vec<_>>();
        assert_eq!(resolved, pins.map(|pin| (pin.repo, pin.commit)));
    }
}
