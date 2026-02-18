use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;

use libloading::Library;
use tree_sitter_language::LanguageFn;

use crate::error::IdenteditError;

use super::BundledLanguageLoader;
#[cfg(test)]
use super::DynamicLanguageLoader;

type RawLanguageFn = unsafe extern "C" fn() -> *const ();

pub(super) struct LoadedGrammar {
    _library: Library,
    language_fn: RawLanguageFn,
}

impl LoadedGrammar {
    pub(super) fn load(path: &Path, symbol: &str) -> Result<Self, IdenteditError> {
        let library =
            unsafe { Library::new(path) }.map_err(|error| IdenteditError::LanguageSetup {
                message: format!(
                    "failed to load dynamic grammar library '{}': {error}",
                    path.display()
                ),
            })?;
        let language_fn = unsafe { library.get::<RawLanguageFn>(symbol.as_bytes()) }
            .map_err(|error| IdenteditError::LanguageSetup {
                message: format!(
                    "failed to load symbol '{symbol}' from '{}': {error}",
                    path.display()
                ),
            })
            .map(|symbol| *symbol)?;

        Ok(Self {
            _library: library,
            language_fn,
        })
    }

    fn language(&self) -> tree_sitter::Language {
        let builder = unsafe { LanguageFn::from_raw(self.language_fn) };
        tree_sitter::Language::new(builder)
    }
}

#[derive(Clone)]
pub(super) enum LanguageSource {
    Bundled(BundledLanguageLoader),
    Dynamic(Arc<LoadedGrammar>),
    #[cfg(test)]
    DynamicLoader(DynamicLanguageLoader),
}

impl LanguageSource {
    pub(super) fn load(&self) -> Result<tree_sitter::Language, IdenteditError> {
        match self {
            Self::Bundled(load_bundled) => {
                catch_unwind(AssertUnwindSafe(load_bundled)).map_err(|payload| {
                    IdenteditError::LanguageSetup {
                        message: format!(
                            "panic while loading bundled tree-sitter language: {}",
                            panic_payload_to_string(payload)
                        ),
                    }
                })
            }
            Self::Dynamic(grammar) => Ok(grammar.language()),
            #[cfg(test)]
            Self::DynamicLoader(load_dynamic) => catch_unwind(AssertUnwindSafe(load_dynamic))
                .map_err(|payload| IdenteditError::LanguageSetup {
                    message: format!(
                        "panic while loading dynamic tree-sitter language: {}",
                        panic_payload_to_string(payload)
                    ),
                })?,
        }
    }
}

#[derive(Clone)]
pub(super) struct LanguageSpec {
    pub(super) name: &'static str,
    pub(super) extensions: &'static [&'static str],
    pub(super) source: LanguageSource,
    pub(super) syntax_error_message: &'static str,
    pub(super) normalize_bare_cr: bool,
}

const PYTHON_EXTENSIONS: &[&str] = &["py"];
const C_EXTENSIONS: &[&str] = &["c"];
const CPP_EXTENSIONS: &[&str] = &["cpp", "cc", "cxx", "hpp", "hh", "hxx"];
pub(super) const C_CPP_HEADER_EXTENSIONS: &[&str] = &["h"];
const JAVASCRIPT_EXTENSIONS: &[&str] = &["js", "jsx"];
const TYPESCRIPT_EXTENSIONS: &[&str] = &["ts"];
const TSX_EXTENSIONS: &[&str] = &["tsx"];
const RUST_EXTENSIONS: &[&str] = &["rs"];
const GO_EXTENSIONS: &[&str] = &["go"];
const DOCKERFILE_EXTENSIONS: &[&str] = &["dockerfile", "containerfile"];
const BASH_EXTENSIONS: &[&str] = &["sh", "bash"];
const ZSH_EXTENSIONS: &[&str] = &["zsh"];
const FISH_EXTENSIONS: &[&str] = &["fish"];
const PHP_EXTENSIONS: &[&str] = &["php"];
const PERL_EXTENSIONS: &[&str] = &["pl", "pm"];
const RUBY_EXTENSIONS: &[&str] = &["rb"];
const HTML_EXTENSIONS: &[&str] = &["html", "htm"];
const CSS_EXTENSIONS: &[&str] = &["css"];
const SCSS_EXTENSIONS: &[&str] = &["scss"];
const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown"];
const JAVA_EXTENSIONS: &[&str] = &["java"];
const KOTLIN_EXTENSIONS: &[&str] = &["kt", "kts"];
const HCL_EXTENSIONS: &[&str] = &["hcl", "tf"];
const LUA_EXTENSIONS: &[&str] = &["lua"];
const CSHARP_EXTENSIONS: &[&str] = &["cs"];
const SWIFT_EXTENSIONS: &[&str] = &["swift"];
const SQL_EXTENSIONS: &[&str] = &["sql"];
const PROTOBUF_EXTENSIONS: &[&str] = &["proto"];
const XML_EXTENSIONS: &[&str] = &["xml"];
const TOML_EXTENSIONS: &[&str] = &["toml"];
const YAML_EXTENSIONS: &[&str] = &["yaml", "yml"];
const DOCKERFILE_BASENAME_ALIASES: &[&str] = &["Dockerfile", "Containerfile"];
const BASH_BASENAME_ALIASES: &[&str] = &[
    ".bashrc",
    ".bash_profile",
    ".bash_login",
    ".bash_logout",
    ".bash_aliases",
];
const ZSH_BASENAME_ALIASES: &[&str] = &[".zshrc", ".zprofile", ".zshenv", ".zlogin", ".zlogout"];
const EMPTY_BASENAME_ALIASES: &[&str] = &[];

const PYTHON_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-python",
    extensions: PYTHON_EXTENSIONS,
    source: LanguageSource::Bundled(load_python_language),
    syntax_error_message: "Syntax errors detected in Python source",
    normalize_bare_cr: true,
};

const C_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-c",
    extensions: C_EXTENSIONS,
    source: LanguageSource::Bundled(load_c_language),
    syntax_error_message: "Syntax errors detected in C source",
    normalize_bare_cr: true,
};

const CPP_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-cpp",
    extensions: CPP_EXTENSIONS,
    source: LanguageSource::Bundled(load_cpp_language),
    syntax_error_message: "Syntax errors detected in C++ source",
    normalize_bare_cr: true,
};

const JAVASCRIPT_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-javascript",
    extensions: JAVASCRIPT_EXTENSIONS,
    source: LanguageSource::Bundled(load_javascript_language),
    syntax_error_message: "Syntax errors detected in JavaScript source",
    normalize_bare_cr: true,
};

const TYPESCRIPT_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-typescript",
    extensions: TYPESCRIPT_EXTENSIONS,
    source: LanguageSource::Bundled(load_typescript_language),
    syntax_error_message: "Syntax errors detected in TypeScript source",
    normalize_bare_cr: true,
};

const TSX_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-tsx",
    extensions: TSX_EXTENSIONS,
    source: LanguageSource::Bundled(load_tsx_language),
    syntax_error_message: "Syntax errors detected in TSX source",
    normalize_bare_cr: true,
};

const RUST_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-rust",
    extensions: RUST_EXTENSIONS,
    source: LanguageSource::Bundled(load_rust_language),
    syntax_error_message: "Syntax errors detected in Rust source",
    normalize_bare_cr: true,
};

const GO_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-go",
    extensions: GO_EXTENSIONS,
    source: LanguageSource::Bundled(load_go_language),
    syntax_error_message: "Syntax errors detected in Go source",
    normalize_bare_cr: true,
};

const DOCKERFILE_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-dockerfile",
    extensions: DOCKERFILE_EXTENSIONS,
    source: LanguageSource::Bundled(load_dockerfile_language),
    syntax_error_message: "Syntax errors detected in Dockerfile source",
    normalize_bare_cr: true,
};

const BASH_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-bash",
    extensions: BASH_EXTENSIONS,
    source: LanguageSource::Bundled(load_bash_language),
    syntax_error_message: "Syntax errors detected in Bash source",
    normalize_bare_cr: true,
};

const ZSH_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-zsh",
    extensions: ZSH_EXTENSIONS,
    source: LanguageSource::Bundled(load_zsh_language),
    syntax_error_message: "Syntax errors detected in Zsh source",
    normalize_bare_cr: true,
};

const FISH_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-fish",
    extensions: FISH_EXTENSIONS,
    source: LanguageSource::Bundled(load_fish_language),
    syntax_error_message: "Syntax errors detected in Fish source",
    normalize_bare_cr: true,
};

const PHP_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-php",
    extensions: PHP_EXTENSIONS,
    source: LanguageSource::Bundled(load_php_language),
    syntax_error_message: "Syntax errors detected in PHP source",
    normalize_bare_cr: true,
};

const PERL_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-perl",
    extensions: PERL_EXTENSIONS,
    source: LanguageSource::Bundled(load_perl_language),
    syntax_error_message: "Syntax errors detected in Perl source",
    normalize_bare_cr: true,
};

const RUBY_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-ruby",
    extensions: RUBY_EXTENSIONS,
    source: LanguageSource::Bundled(load_ruby_language),
    syntax_error_message: "Syntax errors detected in Ruby source",
    normalize_bare_cr: true,
};

const HTML_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-html",
    extensions: HTML_EXTENSIONS,
    source: LanguageSource::Bundled(load_html_language),
    syntax_error_message: "Syntax errors detected in HTML source",
    normalize_bare_cr: true,
};

const CSS_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-css",
    extensions: CSS_EXTENSIONS,
    source: LanguageSource::Bundled(load_css_language),
    syntax_error_message: "Syntax errors detected in CSS source",
    normalize_bare_cr: true,
};

const SCSS_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-scss",
    extensions: SCSS_EXTENSIONS,
    source: LanguageSource::Bundled(load_scss_language),
    syntax_error_message: "Syntax errors detected in SCSS source",
    normalize_bare_cr: true,
};

const MARKDOWN_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-markdown",
    extensions: MARKDOWN_EXTENSIONS,
    source: LanguageSource::Bundled(load_markdown_language),
    syntax_error_message: "Syntax errors detected in Markdown source",
    normalize_bare_cr: true,
};

const JAVA_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-java",
    extensions: JAVA_EXTENSIONS,
    source: LanguageSource::Bundled(load_java_language),
    syntax_error_message: "Syntax errors detected in Java source",
    normalize_bare_cr: true,
};

const KOTLIN_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-kotlin",
    extensions: KOTLIN_EXTENSIONS,
    source: LanguageSource::Bundled(load_kotlin_language),
    syntax_error_message: "Syntax errors detected in Kotlin source",
    normalize_bare_cr: true,
};

const HCL_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-hcl",
    extensions: HCL_EXTENSIONS,
    source: LanguageSource::Bundled(load_hcl_language),
    syntax_error_message: "Syntax errors detected in HCL source",
    normalize_bare_cr: true,
};

const LUA_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-lua",
    extensions: LUA_EXTENSIONS,
    source: LanguageSource::Bundled(load_lua_language),
    syntax_error_message: "Syntax errors detected in Lua source",
    normalize_bare_cr: true,
};

const CSHARP_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-c-sharp",
    extensions: CSHARP_EXTENSIONS,
    source: LanguageSource::Bundled(load_csharp_language),
    syntax_error_message: "Syntax errors detected in C# source",
    normalize_bare_cr: true,
};

const SWIFT_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-swift",
    extensions: SWIFT_EXTENSIONS,
    source: LanguageSource::Bundled(load_swift_language),
    syntax_error_message: "Syntax errors detected in Swift source",
    normalize_bare_cr: true,
};

const SQL_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-sql",
    extensions: SQL_EXTENSIONS,
    source: LanguageSource::Bundled(load_sql_language),
    syntax_error_message: "Syntax errors detected in SQL source",
    normalize_bare_cr: true,
};

const PROTOBUF_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-proto",
    extensions: PROTOBUF_EXTENSIONS,
    source: LanguageSource::Bundled(load_protobuf_language),
    syntax_error_message: "Syntax errors detected in Protobuf source",
    normalize_bare_cr: true,
};

const XML_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-xml",
    extensions: XML_EXTENSIONS,
    source: LanguageSource::Bundled(load_xml_language),
    syntax_error_message: "Syntax errors detected in XML source",
    normalize_bare_cr: true,
};

const TOML_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-toml",
    extensions: TOML_EXTENSIONS,
    source: LanguageSource::Bundled(load_toml_language),
    syntax_error_message: "Syntax errors detected in TOML source",
    normalize_bare_cr: true,
};

const YAML_LANGUAGE_SPEC: LanguageSpec = LanguageSpec {
    name: "tree-sitter-yaml",
    extensions: YAML_EXTENSIONS,
    source: LanguageSource::Bundled(load_yaml_language),
    syntax_error_message: "Syntax errors detected in YAML source",
    normalize_bare_cr: true,
};

pub(super) const C_CPP_HEADER_PROVIDER_NAME: &str = "tree-sitter-c-cpp-header";
pub(super) const C_CPP_HEADER_SYNTAX_ERROR_MESSAGE: &str =
    "Syntax errors detected in C/C++ header source";

const BUNDLED_LANGUAGE_SPECS: &[LanguageSpec] = &[
    PYTHON_LANGUAGE_SPEC,
    C_LANGUAGE_SPEC,
    CPP_LANGUAGE_SPEC,
    JAVASCRIPT_LANGUAGE_SPEC,
    TYPESCRIPT_LANGUAGE_SPEC,
    TSX_LANGUAGE_SPEC,
    RUST_LANGUAGE_SPEC,
    GO_LANGUAGE_SPEC,
    DOCKERFILE_LANGUAGE_SPEC,
    BASH_LANGUAGE_SPEC,
    ZSH_LANGUAGE_SPEC,
    FISH_LANGUAGE_SPEC,
    PHP_LANGUAGE_SPEC,
    PERL_LANGUAGE_SPEC,
    RUBY_LANGUAGE_SPEC,
    HTML_LANGUAGE_SPEC,
    CSS_LANGUAGE_SPEC,
    SCSS_LANGUAGE_SPEC,
    MARKDOWN_LANGUAGE_SPEC,
    JAVA_LANGUAGE_SPEC,
    KOTLIN_LANGUAGE_SPEC,
    HCL_LANGUAGE_SPEC,
    LUA_LANGUAGE_SPEC,
    CSHARP_LANGUAGE_SPEC,
    SWIFT_LANGUAGE_SPEC,
    SQL_LANGUAGE_SPEC,
    PROTOBUF_LANGUAGE_SPEC,
    XML_LANGUAGE_SPEC,
    TOML_LANGUAGE_SPEC,
    YAML_LANGUAGE_SPEC,
];

pub(super) fn bundled_language_specs() -> &'static [LanguageSpec] {
    BUNDLED_LANGUAGE_SPECS
}

pub(super) fn python_language_spec() -> &'static LanguageSpec {
    &PYTHON_LANGUAGE_SPEC
}

pub(super) fn c_language_spec() -> &'static LanguageSpec {
    &C_LANGUAGE_SPEC
}

pub(super) fn cpp_language_spec() -> &'static LanguageSpec {
    &CPP_LANGUAGE_SPEC
}

pub(super) fn leak_string(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

pub(super) fn leak_extensions(extensions: &[String]) -> &'static [&'static str] {
    let leaked = extensions
        .iter()
        .map(|value| leak_string(value.clone()))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    Box::leak(leaked)
}

pub(super) fn basename_aliases_for_provider(provider_name: &str) -> &'static [&'static str] {
    match provider_name {
        "tree-sitter-dockerfile" => DOCKERFILE_BASENAME_ALIASES,
        "tree-sitter-bash" => BASH_BASENAME_ALIASES,
        "tree-sitter-zsh" => ZSH_BASENAME_ALIASES,
        _ => EMPTY_BASENAME_ALIASES,
    }
}

pub(super) fn load_python_language() -> tree_sitter::Language {
    tree_sitter_python::LANGUAGE.into()
}

fn load_c_language() -> tree_sitter::Language {
    tree_sitter_c::LANGUAGE.into()
}

fn load_cpp_language() -> tree_sitter::Language {
    tree_sitter_cpp::LANGUAGE.into()
}

fn load_javascript_language() -> tree_sitter::Language {
    tree_sitter_javascript::LANGUAGE.into()
}

fn load_typescript_language() -> tree_sitter::Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

fn load_tsx_language() -> tree_sitter::Language {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

fn load_rust_language() -> tree_sitter::Language {
    tree_sitter_rust::LANGUAGE.into()
}

fn load_go_language() -> tree_sitter::Language {
    tree_sitter_go::LANGUAGE.into()
}

fn load_dockerfile_language() -> tree_sitter::Language {
    tree_sitter_dockerfile::language()
}

fn load_bash_language() -> tree_sitter::Language {
    tree_sitter_bash::LANGUAGE.into()
}

fn load_zsh_language() -> tree_sitter::Language {
    tree_sitter_zsh::LANGUAGE.into()
}

fn load_fish_language() -> tree_sitter::Language {
    tree_sitter_fish::language()
}

fn load_php_language() -> tree_sitter::Language {
    tree_sitter_php::LANGUAGE_PHP.into()
}

fn load_perl_language() -> tree_sitter::Language {
    tree_sitter_perl::LANGUAGE.into()
}

fn load_ruby_language() -> tree_sitter::Language {
    tree_sitter_ruby::LANGUAGE.into()
}

fn load_html_language() -> tree_sitter::Language {
    tree_sitter_html::LANGUAGE.into()
}

fn load_css_language() -> tree_sitter::Language {
    tree_sitter_css::LANGUAGE.into()
}

fn load_scss_language() -> tree_sitter::Language {
    tree_sitter_scss::language()
}

fn load_markdown_language() -> tree_sitter::Language {
    tree_sitter_markdown::LANGUAGE.into()
}

fn load_java_language() -> tree_sitter::Language {
    tree_sitter_java::LANGUAGE.into()
}

fn load_kotlin_language() -> tree_sitter::Language {
    tree_sitter_kotlin::LANGUAGE.into()
}

fn load_hcl_language() -> tree_sitter::Language {
    tree_sitter_hcl::LANGUAGE.into()
}

fn load_lua_language() -> tree_sitter::Language {
    tree_sitter_lua::LANGUAGE.into()
}

fn load_csharp_language() -> tree_sitter::Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}

fn load_swift_language() -> tree_sitter::Language {
    tree_sitter_swift::LANGUAGE.into()
}

fn load_sql_language() -> tree_sitter::Language {
    tree_sitter_sequel::LANGUAGE.into()
}

fn load_protobuf_language() -> tree_sitter::Language {
    tree_sitter_proto::LANGUAGE.into()
}

fn load_xml_language() -> tree_sitter::Language {
    tree_sitter_xml::LANGUAGE_XML.into()
}

fn load_toml_language() -> tree_sitter::Language {
    tree_sitter_toml::LANGUAGE.into()
}

fn load_yaml_language() -> tree_sitter::Language {
    tree_sitter_yaml::LANGUAGE.into()
}

fn panic_payload_to_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }

    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }

    "unknown panic payload".to_string()
}
