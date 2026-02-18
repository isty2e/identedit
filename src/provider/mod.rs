use std::collections::BTreeSet;
use std::path::Path;

use crate::error::IdenteditError;
use crate::handle::SelectionHandle;

mod fallback;
mod json;
mod tree_sitter;
mod util;

pub use fallback::FallbackProvider;
pub use json::JsonProvider;
pub use tree_sitter::{HeaderTreeSitterProvider, TreeSitterProvider};
pub(crate) use util::{node_text, normalize_bare_cr_for_parser};

pub(crate) fn normalize_extension(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let without_dot = trimmed.strip_prefix('.').unwrap_or(trimmed);
    if without_dot.is_empty() {
        return None;
    }

    Some(without_dot.to_lowercase())
}

pub trait StructureProvider {
    fn parse(&self, path: &Path, source: &[u8]) -> Result<Vec<SelectionHandle>, IdenteditError>;
    fn can_handle(&self, path: &Path) -> bool;
    fn name(&self) -> &'static str;
    fn supported_extensions(&self) -> &'static [&'static str];
}

pub struct ProviderRegistry {
    providers: Vec<Box<dyn StructureProvider>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        let mut providers: Vec<Box<dyn StructureProvider>> = TreeSitterProvider::bundled()
            .into_iter()
            .map(|provider| Box::new(provider) as Box<dyn StructureProvider>)
            .collect();
        providers.push(Box::new(HeaderTreeSitterProvider::new()));
        providers.extend(
            TreeSitterProvider::dynamic_from_manifest()
                .into_iter()
                .map(|provider| Box::new(provider) as Box<dyn StructureProvider>),
        );
        providers.push(Box::new(JsonProvider));
        providers.push(Box::new(FallbackProvider));

        Self { providers }
    }
}

impl ProviderRegistry {
    pub fn provider_for(&self, path: &Path) -> Result<&dyn StructureProvider, IdenteditError> {
        for provider in &self.providers {
            if provider.can_handle(path) {
                return Ok(provider.as_ref());
            }
        }

        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .and_then(normalize_extension)
            .unwrap_or_else(|| "<none>".to_string());

        Err(IdenteditError::NoProvider {
            extension,
            supported_extensions: self.supported_extensions(),
        })
    }

    fn supported_extensions(&self) -> Vec<String> {
        let mut unique = BTreeSet::new();

        for provider in &self.providers {
            for extension in provider.supported_extensions() {
                if let Some(normalized) = normalize_extension(extension) {
                    unique.insert(normalized);
                }
            }
        }

        unique.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    use std::path::Path;
    use std::path::PathBuf;

    use super::{ProviderRegistry, StructureProvider};
    use crate::error::IdenteditError;
    use crate::handle::SelectionHandle;

    struct StubProvider {
        provider_name: &'static str,
        extensions: &'static [&'static str],
    }

    impl StructureProvider for StubProvider {
        fn parse(
            &self,
            _path: &Path,
            _source: &[u8],
        ) -> Result<Vec<SelectionHandle>, IdenteditError> {
            Ok(Vec::new())
        }

        fn can_handle(&self, path: &Path) -> bool {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| {
                    self.extensions
                        .iter()
                        .any(|supported| extension.eq_ignore_ascii_case(supported))
                })
        }

        fn name(&self) -> &'static str {
            self.provider_name
        }

        fn supported_extensions(&self) -> &'static [&'static str] {
            self.extensions
        }
    }

    #[test]
    fn provider_for_selects_python_and_json_by_extension() {
        let registry = ProviderRegistry::default();

        let python = registry
            .provider_for(Path::new("example.py"))
            .expect("python provider");
        let c = registry
            .provider_for(Path::new("example.c"))
            .expect("c provider");
        let cpp = registry
            .provider_for(Path::new("example.cpp"))
            .expect("cpp provider");
        let h = registry
            .provider_for(Path::new("example.h"))
            .expect("header provider");
        let javascript = registry
            .provider_for(Path::new("example.js"))
            .expect("javascript provider");
        let jsx = registry
            .provider_for(Path::new("example.jsx"))
            .expect("jsx provider");
        let typescript = registry
            .provider_for(Path::new("example.ts"))
            .expect("typescript provider");
        let tsx = registry
            .provider_for(Path::new("example.tsx"))
            .expect("tsx provider");
        let rust = registry
            .provider_for(Path::new("example.rs"))
            .expect("rust provider");
        let go = registry
            .provider_for(Path::new("example.go"))
            .expect("go provider");
        let bash = registry
            .provider_for(Path::new("example.sh"))
            .expect("bash provider");
        let bashrc = registry
            .provider_for(Path::new(".bashrc"))
            .expect("bashrc provider");
        let zsh = registry
            .provider_for(Path::new("example.zsh"))
            .expect("zsh provider");
        let zshrc = registry
            .provider_for(Path::new(".zshrc"))
            .expect("zshrc provider");
        let fish = registry
            .provider_for(Path::new("example.fish"))
            .expect("fish provider");
        let php = registry
            .provider_for(Path::new("example.php"))
            .expect("php provider");
        let perl = registry
            .provider_for(Path::new("example.pl"))
            .expect("perl provider");
        let ruby = registry
            .provider_for(Path::new("example.rb"))
            .expect("ruby provider");
        let scss = registry
            .provider_for(Path::new("example.scss"))
            .expect("scss provider");
        let markdown = registry
            .provider_for(Path::new("example.md"))
            .expect("markdown provider");
        let markdown_long = registry
            .provider_for(Path::new("example.markdown"))
            .expect("markdown extension provider");
        let html = registry
            .provider_for(Path::new("example.html"))
            .expect("html provider");
        let css = registry
            .provider_for(Path::new("example.css"))
            .expect("css provider");
        let java = registry
            .provider_for(Path::new("example.java"))
            .expect("java provider");
        let kotlin = registry
            .provider_for(Path::new("example.kt"))
            .expect("kotlin provider");
        let kotlin_script = registry
            .provider_for(Path::new("example.kts"))
            .expect("kotlin script provider");
        let hcl = registry
            .provider_for(Path::new("example.hcl"))
            .expect("hcl provider");
        let terraform = registry
            .provider_for(Path::new("example.tf"))
            .expect("terraform provider");
        let lua = registry
            .provider_for(Path::new("example.lua"))
            .expect("lua provider");
        let csharp = registry
            .provider_for(Path::new("example.cs"))
            .expect("csharp provider");
        let swift = registry
            .provider_for(Path::new("example.swift"))
            .expect("swift provider");
        let dockerfile = registry
            .provider_for(Path::new("example.dockerfile"))
            .expect("dockerfile provider");
        let dockerfile_basename = registry
            .provider_for(Path::new("Dockerfile"))
            .expect("dockerfile basename provider");
        let containerfile_basename = registry
            .provider_for(Path::new("Containerfile"))
            .expect("containerfile basename provider");
        let sql = registry
            .provider_for(Path::new("example.sql"))
            .expect("sql provider");
        let proto = registry
            .provider_for(Path::new("example.proto"))
            .expect("protobuf provider");
        let xml = registry
            .provider_for(Path::new("example.xml"))
            .expect("xml provider");
        let toml = registry
            .provider_for(Path::new("example.toml"))
            .expect("toml provider");
        let yaml = registry
            .provider_for(Path::new("example.yaml"))
            .expect("yaml provider");
        let yml = registry
            .provider_for(Path::new("example.yml"))
            .expect("yml provider");
        let json = registry
            .provider_for(Path::new("example.json"))
            .expect("json provider");

        assert_eq!(python.name(), "tree-sitter-python");
        assert_eq!(c.name(), "tree-sitter-c");
        assert_eq!(cpp.name(), "tree-sitter-cpp");
        assert_eq!(h.name(), "tree-sitter-c-cpp-header");
        assert_eq!(javascript.name(), "tree-sitter-javascript");
        assert_eq!(jsx.name(), "tree-sitter-javascript");
        assert_eq!(typescript.name(), "tree-sitter-typescript");
        assert_eq!(tsx.name(), "tree-sitter-tsx");
        assert_eq!(rust.name(), "tree-sitter-rust");
        assert_eq!(go.name(), "tree-sitter-go");
        assert_eq!(bash.name(), "tree-sitter-bash");
        assert_eq!(bashrc.name(), "tree-sitter-bash");
        assert_eq!(zsh.name(), "tree-sitter-zsh");
        assert_eq!(zshrc.name(), "tree-sitter-zsh");
        assert_eq!(fish.name(), "tree-sitter-fish");
        assert_eq!(php.name(), "tree-sitter-php");
        assert_eq!(perl.name(), "tree-sitter-perl");
        assert_eq!(ruby.name(), "tree-sitter-ruby");
        assert_eq!(scss.name(), "tree-sitter-scss");
        assert_eq!(markdown.name(), "tree-sitter-markdown");
        assert_eq!(markdown_long.name(), "tree-sitter-markdown");
        assert_eq!(html.name(), "tree-sitter-html");
        assert_eq!(css.name(), "tree-sitter-css");
        assert_eq!(java.name(), "tree-sitter-java");
        assert_eq!(kotlin.name(), "tree-sitter-kotlin");
        assert_eq!(kotlin_script.name(), "tree-sitter-kotlin");
        assert_eq!(hcl.name(), "tree-sitter-hcl");
        assert_eq!(terraform.name(), "tree-sitter-hcl");
        assert_eq!(lua.name(), "tree-sitter-lua");
        assert_eq!(csharp.name(), "tree-sitter-c-sharp");
        assert_eq!(swift.name(), "tree-sitter-swift");
        assert_eq!(dockerfile.name(), "tree-sitter-dockerfile");
        assert_eq!(dockerfile_basename.name(), "tree-sitter-dockerfile");
        assert_eq!(containerfile_basename.name(), "tree-sitter-dockerfile");
        assert_eq!(sql.name(), "tree-sitter-sql");
        assert_eq!(proto.name(), "tree-sitter-proto");
        assert_eq!(xml.name(), "tree-sitter-xml");
        assert_eq!(toml.name(), "tree-sitter-toml");
        assert_eq!(yaml.name(), "tree-sitter-yaml");
        assert_eq!(yml.name(), "tree-sitter-yaml");
        assert_eq!(json.name(), "json");
    }

    #[test]
    fn provider_for_is_case_insensitive_for_supported_extensions() {
        let registry = ProviderRegistry::default();

        let python = registry
            .provider_for(Path::new("example.PY"))
            .expect("python provider");
        let c = registry
            .provider_for(Path::new("example.C"))
            .expect("c provider");
        let cpp = registry
            .provider_for(Path::new("example.HPP"))
            .expect("cpp provider");
        let h = registry
            .provider_for(Path::new("example.H"))
            .expect("header provider");
        let javascript = registry
            .provider_for(Path::new("example.JS"))
            .expect("javascript provider");
        let jsx = registry
            .provider_for(Path::new("example.Jsx"))
            .expect("jsx provider");
        let typescript = registry
            .provider_for(Path::new("example.tS"))
            .expect("typescript provider");
        let tsx = registry
            .provider_for(Path::new("example.TsX"))
            .expect("tsx provider");
        let rust = registry
            .provider_for(Path::new("example.RS"))
            .expect("rust provider");
        let go = registry
            .provider_for(Path::new("example.GO"))
            .expect("go provider");
        let bash = registry
            .provider_for(Path::new("example.SH"))
            .expect("bash provider");
        let bashrc = registry
            .provider_for(Path::new(".BASHRC"))
            .expect("bashrc provider");
        let zsh = registry
            .provider_for(Path::new("example.ZSH"))
            .expect("zsh provider");
        let zshenv = registry
            .provider_for(Path::new(".ZSHENV"))
            .expect("zshenv provider");
        let fish = registry
            .provider_for(Path::new("example.FISH"))
            .expect("fish provider");
        let php = registry
            .provider_for(Path::new("example.PHP"))
            .expect("php provider");
        let perl = registry
            .provider_for(Path::new("example.PM"))
            .expect("perl provider");
        let ruby = registry
            .provider_for(Path::new("example.RB"))
            .expect("ruby provider");
        let scss = registry
            .provider_for(Path::new("example.SCSS"))
            .expect("scss provider");
        let markdown = registry
            .provider_for(Path::new("example.MD"))
            .expect("markdown provider");
        let markdown_long = registry
            .provider_for(Path::new("example.MaRkDoWn"))
            .expect("markdown extension provider");
        let html = registry
            .provider_for(Path::new("example.HtM"))
            .expect("html provider");
        let css = registry
            .provider_for(Path::new("example.CsS"))
            .expect("css provider");
        let java = registry
            .provider_for(Path::new("example.JaVa"))
            .expect("java provider");
        let kotlin = registry
            .provider_for(Path::new("example.KT"))
            .expect("kotlin provider");
        let kotlin_script = registry
            .provider_for(Path::new("example.KtS"))
            .expect("kotlin script provider");
        let hcl = registry
            .provider_for(Path::new("example.HCL"))
            .expect("hcl provider");
        let terraform = registry
            .provider_for(Path::new("example.Tf"))
            .expect("terraform provider");
        let lua = registry
            .provider_for(Path::new("example.LuA"))
            .expect("lua provider");
        let csharp = registry
            .provider_for(Path::new("example.CS"))
            .expect("csharp provider");
        let swift = registry
            .provider_for(Path::new("example.SwIfT"))
            .expect("swift provider");
        let dockerfile = registry
            .provider_for(Path::new("example.DoCkErFiLe"))
            .expect("dockerfile provider");
        let dockerfile_basename = registry
            .provider_for(Path::new("DOCKERFILE"))
            .expect("dockerfile basename provider");
        let containerfile_basename = registry
            .provider_for(Path::new("CONTAINERFILE"))
            .expect("containerfile basename provider");
        let sql = registry
            .provider_for(Path::new("example.SqL"))
            .expect("sql provider");
        let proto = registry
            .provider_for(Path::new("example.PrOtO"))
            .expect("protobuf provider");
        let xml = registry
            .provider_for(Path::new("example.XmL"))
            .expect("xml provider");
        let toml = registry
            .provider_for(Path::new("example.ToMl"))
            .expect("toml provider");
        let yaml = registry
            .provider_for(Path::new("example.YaMl"))
            .expect("yaml provider");
        let yml = registry
            .provider_for(Path::new("example.YmL"))
            .expect("yml provider");
        let json = registry
            .provider_for(Path::new("example.JsOn"))
            .expect("json provider");

        assert_eq!(python.name(), "tree-sitter-python");
        assert_eq!(c.name(), "tree-sitter-c");
        assert_eq!(cpp.name(), "tree-sitter-cpp");
        assert_eq!(h.name(), "tree-sitter-c-cpp-header");
        assert_eq!(javascript.name(), "tree-sitter-javascript");
        assert_eq!(jsx.name(), "tree-sitter-javascript");
        assert_eq!(typescript.name(), "tree-sitter-typescript");
        assert_eq!(tsx.name(), "tree-sitter-tsx");
        assert_eq!(rust.name(), "tree-sitter-rust");
        assert_eq!(go.name(), "tree-sitter-go");
        assert_eq!(bash.name(), "tree-sitter-bash");
        assert_eq!(bashrc.name(), "tree-sitter-bash");
        assert_eq!(zsh.name(), "tree-sitter-zsh");
        assert_eq!(zshenv.name(), "tree-sitter-zsh");
        assert_eq!(fish.name(), "tree-sitter-fish");
        assert_eq!(php.name(), "tree-sitter-php");
        assert_eq!(perl.name(), "tree-sitter-perl");
        assert_eq!(ruby.name(), "tree-sitter-ruby");
        assert_eq!(scss.name(), "tree-sitter-scss");
        assert_eq!(markdown.name(), "tree-sitter-markdown");
        assert_eq!(markdown_long.name(), "tree-sitter-markdown");
        assert_eq!(html.name(), "tree-sitter-html");
        assert_eq!(css.name(), "tree-sitter-css");
        assert_eq!(java.name(), "tree-sitter-java");
        assert_eq!(kotlin.name(), "tree-sitter-kotlin");
        assert_eq!(kotlin_script.name(), "tree-sitter-kotlin");
        assert_eq!(hcl.name(), "tree-sitter-hcl");
        assert_eq!(terraform.name(), "tree-sitter-hcl");
        assert_eq!(lua.name(), "tree-sitter-lua");
        assert_eq!(csharp.name(), "tree-sitter-c-sharp");
        assert_eq!(swift.name(), "tree-sitter-swift");
        assert_eq!(dockerfile.name(), "tree-sitter-dockerfile");
        assert_eq!(dockerfile_basename.name(), "tree-sitter-dockerfile");
        assert_eq!(containerfile_basename.name(), "tree-sitter-dockerfile");
        assert_eq!(sql.name(), "tree-sitter-sql");
        assert_eq!(proto.name(), "tree-sitter-proto");
        assert_eq!(xml.name(), "tree-sitter-xml");
        assert_eq!(toml.name(), "tree-sitter-toml");
        assert_eq!(yaml.name(), "tree-sitter-yaml");
        assert_eq!(yml.name(), "tree-sitter-yaml");
        assert_eq!(json.name(), "json");
    }

    #[test]
    fn provider_for_unknown_extension_routes_to_fallback() {
        let registry = ProviderRegistry::default();
        let provider = registry
            .provider_for(Path::new("example.ini"))
            .expect("unknown extension should fall back");

        assert_eq!(provider.name(), "fallback");
    }

    #[test]
    fn provider_for_h_extension_routes_to_dual_header_provider() {
        let registry = ProviderRegistry::default();
        let provider = registry
            .provider_for(Path::new("example.h"))
            .expect(".h should route to dual C/C++ header provider");

        assert_eq!(provider.name(), "tree-sitter-c-cpp-header");
    }

    #[test]
    fn provider_for_extensionless_path_routes_to_fallback() {
        let registry = ProviderRegistry::default();
        let provider = registry
            .provider_for(Path::new("README"))
            .expect("extensionless path should fall back");

        assert_eq!(provider.name(), "fallback");
    }

    #[test]
    fn provider_for_multi_dot_path_routes_to_last_extension() {
        let registry = ProviderRegistry::default();

        let json = registry
            .provider_for(Path::new("archive.backup.JsOn"))
            .expect("json provider");

        assert_eq!(json.name(), "json");
    }

    #[test]
    fn provider_for_trailing_dot_and_hidden_dotfile_route_to_fallback() {
        let registry = ProviderRegistry::default();

        for path in [Path::new("fixture."), Path::new(".json")] {
            let provider = registry
                .provider_for(path)
                .expect("trailing-dot and hidden dotfiles should fall back");
            assert_eq!(provider.name(), "fallback");
        }
    }

    #[test]
    fn provider_for_whitespace_only_extension_routes_to_fallback() {
        let registry = ProviderRegistry::default();
        let provider = registry
            .provider_for(Path::new("fixture.   "))
            .expect("whitespace-only extension should fall back");

        assert_eq!(provider.name(), "fallback");
    }

    #[test]
    fn provider_for_trims_trailing_whitespace_in_extension_before_matching() {
        let registry = ProviderRegistry::default();
        let provider = registry
            .provider_for(Path::new("fixture.PY "))
            .expect("trailing whitespace extension should normalize to python");

        assert_eq!(provider.name(), "tree-sitter-python");
    }

    #[test]
    fn provider_for_prefers_first_registered_provider_for_duplicate_extension() {
        let registry = ProviderRegistry {
            providers: vec![
                Box::new(StubProvider {
                    provider_name: "first-dup-provider",
                    extensions: &["dup"],
                }),
                Box::new(StubProvider {
                    provider_name: "second-dup-provider",
                    extensions: &["dup"],
                }),
            ],
        };

        for _ in 0..8 {
            let provider = registry
                .provider_for(Path::new("fixture.DUP"))
                .expect("duplicate extension should resolve deterministically");
            assert_eq!(provider.name(), "first-dup-provider");
        }
    }

    #[test]
    fn supported_extensions_are_deduplicated_and_sorted_with_multiple_providers() {
        let registry = ProviderRegistry {
            providers: vec![
                Box::new(StubProvider {
                    provider_name: "provider-a",
                    extensions: &["py", "json", "ts"],
                }),
                Box::new(StubProvider {
                    provider_name: "provider-b",
                    extensions: &["go", "py", "ts"],
                }),
                Box::new(StubProvider {
                    provider_name: "provider-c",
                    extensions: &["json", "go"],
                }),
            ],
        };

        let error = match registry.provider_for(Path::new("fixture.unknown")) {
            Ok(_) => panic!("unknown extension should fail"),
            Err(error) => error,
        };
        match error {
            IdenteditError::NoProvider {
                supported_extensions,
                ..
            } => {
                assert_eq!(
                    supported_extensions,
                    vec![
                        "go".to_string(),
                        "json".to_string(),
                        "py".to_string(),
                        "ts".to_string()
                    ]
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn supported_extensions_deduplicate_case_insensitively_and_trim_dot_prefix() {
        let registry = ProviderRegistry {
            providers: vec![
                Box::new(StubProvider {
                    provider_name: "provider-a",
                    extensions: &["py", "PY", ".Py", " ts "],
                }),
                Box::new(StubProvider {
                    provider_name: "provider-b",
                    extensions: &["TS", ".ts", "json"],
                }),
            ],
        };

        let error = match registry.provider_for(Path::new("fixture.unknown")) {
            Ok(_) => panic!("unknown extension should fail"),
            Err(error) => error,
        };
        match error {
            IdenteditError::NoProvider {
                supported_extensions,
                ..
            } => {
                assert_eq!(
                    supported_extensions,
                    vec!["json".to_string(), "py".to_string(), "ts".to_string()]
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn no_provider_none_extension_contract_holds_in_multi_provider_setup() {
        let registry = ProviderRegistry {
            providers: vec![
                Box::new(StubProvider {
                    provider_name: "provider-a",
                    extensions: &["py"],
                }),
                Box::new(StubProvider {
                    provider_name: "provider-b",
                    extensions: &["go"],
                }),
            ],
        };

        for path in [Path::new("README"), Path::new("fixture."), Path::new(".go")] {
            let error = match registry.provider_for(path) {
                Ok(_) => panic!("dotfile/extensionless variants should fail"),
                Err(error) => error,
            };
            match error {
                IdenteditError::NoProvider {
                    extension,
                    supported_extensions,
                } => {
                    assert_eq!(extension, "<none>");
                    assert_eq!(
                        supported_extensions,
                        vec!["go".to_string(), "py".to_string()]
                    );
                }
                other => panic!("unexpected error: {other}"),
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn provider_for_non_utf8_extension_routes_to_fallback() {
        let registry = ProviderRegistry::default();
        let mut bytes = b"fixture.".to_vec();
        bytes.push(0xFF);
        let path = PathBuf::from(OsString::from_vec(bytes));

        let provider = registry
            .provider_for(&path)
            .expect("non-utf8 extension path should fall back");

        assert_eq!(provider.name(), "fallback");
    }

    #[test]
    fn empty_registry_returns_no_provider_with_empty_supported_extensions() {
        let registry = ProviderRegistry {
            providers: Vec::new(),
        };

        let error = match registry.provider_for(Path::new("fixture.py")) {
            Ok(_) => panic!("empty registry should fail"),
            Err(error) => error,
        };
        match error {
            IdenteditError::NoProvider {
                extension,
                supported_extensions,
            } => {
                assert_eq!(extension, "py");
                assert!(
                    supported_extensions.is_empty(),
                    "empty registry should report empty supported extension list"
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn no_provider_error_extension_uses_normalized_trailing_whitespace_token() {
        let registry = ProviderRegistry {
            providers: Vec::new(),
        };

        let error = match registry.provider_for(Path::new("fixture.ToMl ")) {
            Ok(_) => panic!("empty registry should fail"),
            Err(error) => error,
        };
        match error {
            IdenteditError::NoProvider { extension, .. } => {
                assert_eq!(extension, "toml");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn supported_extensions_normalize_non_ascii_tokens_deterministically() {
        let registry = ProviderRegistry {
            providers: vec![
                Box::new(StubProvider {
                    provider_name: "provider-a",
                    extensions: &["ÄXT", ".äxt", "ßETA", " λ "],
                }),
                Box::new(StubProvider {
                    provider_name: "provider-b",
                    extensions: &["äxt", ".ßeta", "Λ"],
                }),
            ],
        };

        let error = match registry.provider_for(Path::new("fixture.none")) {
            Ok(_) => panic!("unknown extension should fail"),
            Err(error) => error,
        };
        match error {
            IdenteditError::NoProvider {
                supported_extensions,
                ..
            } => {
                assert_eq!(
                    supported_extensions,
                    vec!["ßeta".to_string(), "äxt".to_string(), "λ".to_string()]
                );
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
