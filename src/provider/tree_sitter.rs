use std::path::Path;
use std::sync::Arc;

use crate::error::IdenteditError;
use crate::grammar::InstalledGrammar;
use crate::handle::SelectionHandle;
use crate::provider::{StructureProvider, normalize_extension};

mod catalog;
mod header;
mod parser;

pub type BundledLanguageLoader = fn() -> tree_sitter::Language;
#[cfg(test)]
pub type DynamicLanguageLoader = fn() -> Result<tree_sitter::Language, IdenteditError>;

#[cfg(test)]
use catalog::load_python_language;
use catalog::{
    C_CPP_HEADER_EXTENSIONS, C_CPP_HEADER_PROVIDER_NAME, LanguageSource, LanguageSpec,
    LoadedGrammar, basename_aliases_for_provider, bundled_language_specs, leak_extensions,
    leak_string, python_language_spec,
};
#[cfg(test)]
use header::HeaderDialect;
use header::parse_c_cpp_header_with_dialect;
use parser::parse_with_spec;

pub struct TreeSitterProvider {
    spec: &'static LanguageSpec,
    basename_aliases: &'static [&'static str],
}

pub struct HeaderTreeSitterProvider;

impl TreeSitterProvider {
    pub fn bundled() -> Vec<Self> {
        bundled_language_specs()
            .iter()
            .map(Self::from_spec)
            .collect()
    }

    pub fn python() -> Self {
        Self::from_spec(python_language_spec())
    }

    pub fn dynamic_from_manifest() -> Vec<Self> {
        let mut providers = Vec::new();

        for grammar in crate::grammar::installed_grammars_for_runtime() {
            if let Ok(provider) = Self::from_installed_grammar(&grammar) {
                providers.push(provider);
            }
        }

        providers
    }

    pub fn from_installed_grammar(grammar: &InstalledGrammar) -> Result<Self, IdenteditError> {
        let loaded = Arc::new(LoadedGrammar::load(&grammar.library_path, &grammar.symbol)?);
        let name = leak_string(format!("tree-sitter-{}", grammar.lang));
        let syntax_error_message =
            leak_string(format!("Syntax errors detected in {} source", grammar.lang));
        let extensions = leak_extensions(&grammar.extensions);
        let spec = Box::leak(Box::new(LanguageSpec {
            name,
            extensions,
            source: LanguageSource::Dynamic(loaded),
            syntax_error_message,
            normalize_bare_cr: true,
        }));

        Ok(Self::from_spec(spec))
    }

    fn from_spec(spec: &'static LanguageSpec) -> Self {
        Self {
            spec,
            basename_aliases: basename_aliases_for_provider(spec.name),
        }
    }
}

impl HeaderTreeSitterProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HeaderTreeSitterProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_basename_alias(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(trimmed.to_lowercase())
}

impl StructureProvider for HeaderTreeSitterProvider {
    fn parse(&self, path: &Path, source: &[u8]) -> Result<Vec<SelectionHandle>, IdenteditError> {
        let (handles, _) = parse_c_cpp_header_with_dialect(path, source)?;
        Ok(handles)
    }

    fn can_handle(&self, path: &Path) -> bool {
        let requested = path
            .extension()
            .and_then(|value| value.to_str())
            .and_then(normalize_extension);

        let Some(requested) = requested else {
            return false;
        };

        C_CPP_HEADER_EXTENSIONS
            .iter()
            .filter_map(|supported| normalize_extension(supported))
            .any(|supported| supported == requested)
    }

    fn name(&self) -> &'static str {
        C_CPP_HEADER_PROVIDER_NAME
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        C_CPP_HEADER_EXTENSIONS
    }
}

impl StructureProvider for TreeSitterProvider {
    fn parse(&self, path: &Path, source: &[u8]) -> Result<Vec<SelectionHandle>, IdenteditError> {
        parse_with_spec(self.spec, path, source)
    }

    fn can_handle(&self, path: &Path) -> bool {
        let requested = path
            .extension()
            .and_then(|value| value.to_str())
            .and_then(normalize_extension);

        if let Some(requested) = requested
            && self
                .spec
                .extensions
                .iter()
                .filter_map(|supported| normalize_extension(supported))
                .any(|supported| supported == requested)
        {
            return true;
        }

        let requested_basename = path
            .file_name()
            .and_then(|value| value.to_str())
            .and_then(normalize_basename_alias);

        let Some(requested_basename) = requested_basename else {
            return false;
        };

        self.basename_aliases
            .iter()
            .filter_map(|supported| normalize_basename_alias(supported))
            .any(|supported| supported == requested_basename)
    }

    fn name(&self) -> &'static str {
        self.spec.name
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        self.spec.extensions
    }
}

#[cfg(test)]
mod tests {
    use std::panic::panic_any;
    use std::path::Path;

    use super::{
        HeaderDialect, HeaderTreeSitterProvider, LanguageSource, LanguageSpec, TreeSitterProvider,
        load_python_language, parse_c_cpp_header_with_dialect,
    };
    use crate::error::IdenteditError;
    use crate::provider::StructureProvider;

    const DYNAMIC_FAILURE_EXTENSIONS: &[&str] = &["dyn"];
    const CUSTOM_SYNTAX_MESSAGE_EXTENSIONS: &[&str] = &["pyspec"];
    const MULTI_EXTENSION_EXTENSIONS: &[&str] = &["py", "pyi", "pyw"];
    const INVALID_EXTENSION_TOKENS: &[&str] = &["", "   ", ".PYI", "Py"];

    const DYNAMIC_FAILURE_SPEC: LanguageSpec = LanguageSpec {
        name: "tree-sitter-dynamic-failure-test",
        extensions: DYNAMIC_FAILURE_EXTENSIONS,
        source: LanguageSource::DynamicLoader(load_dynamic_failing_language),
        syntax_error_message: "unused syntax message",
        normalize_bare_cr: false,
    };

    const CUSTOM_SYNTAX_MESSAGE_SPEC: LanguageSpec = LanguageSpec {
        name: "tree-sitter-python-custom-message-test",
        extensions: CUSTOM_SYNTAX_MESSAGE_EXTENSIONS,
        source: LanguageSource::Bundled(load_python_language),
        syntax_error_message: "Custom syntax error emitted from language spec",
        normalize_bare_cr: true,
    };

    const MULTI_EXTENSION_SPEC: LanguageSpec = LanguageSpec {
        name: "tree-sitter-python-multi-extension-test",
        extensions: MULTI_EXTENSION_EXTENSIONS,
        source: LanguageSource::Bundled(load_python_language),
        syntax_error_message: "Syntax errors detected in Python source",
        normalize_bare_cr: true,
    };

    const INVALID_EXTENSION_SPEC: LanguageSpec = LanguageSpec {
        name: "tree-sitter-python-invalid-extension-test",
        extensions: INVALID_EXTENSION_TOKENS,
        source: LanguageSource::Bundled(load_python_language),
        syntax_error_message: "Syntax errors detected in Python source",
        normalize_bare_cr: true,
    };

    fn load_dynamic_failing_language() -> Result<tree_sitter::Language, IdenteditError> {
        Err(IdenteditError::LanguageSetup {
            message: "dynamic language loader unavailable".to_string(),
        })
    }

    fn load_dynamic_panicking_language() -> Result<tree_sitter::Language, IdenteditError> {
        panic!("dynamic loader exploded");
    }

    fn load_bundled_panicking_language() -> tree_sitter::Language {
        panic!("bundled loader exploded");
    }

    fn load_dynamic_non_string_panicking_language() -> Result<tree_sitter::Language, IdenteditError>
    {
        panic_any(42usize);
    }

    fn load_bundled_non_string_panicking_language() -> tree_sitter::Language {
        panic_any(1337usize);
    }

    fn bundled_provider_for(path: &Path) -> TreeSitterProvider {
        TreeSitterProvider::bundled()
            .into_iter()
            .find(|provider| provider.can_handle(path))
            .unwrap_or_else(|| panic!("expected bundled provider for '{}'", path.display()))
    }

    #[test]
    fn parse_extracts_function_and_class_symbol_names() {
        let provider = TreeSitterProvider::python();
        let source =
            b"class Processor:\n    pass\n\n\ndef process_data(value):\n    return value + 1\n";

        let handles = provider
            .parse(Path::new("fixture.py"), source)
            .expect("python parse should succeed");

        let class_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "class_definition" && handle.name.as_deref() == Some("Processor")
            })
            .expect("class handle should exist");
        assert!(class_handle.text.contains("class Processor"));

        let function_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "function_definition"
                    && handle.name.as_deref() == Some("process_data")
            })
            .expect("function handle should exist");
        assert!(function_handle.text.contains("def process_data"));
    }

    #[test]
    fn parse_extracts_unicode_identifier_name() {
        let provider = TreeSitterProvider::python();
        let source = "def 변수(value):\n    return value\n".as_bytes();

        let handles = provider
            .parse(Path::new("fixture.py"), source)
            .expect("python parse should succeed");

        let function_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "function_definition" && handle.name.as_deref() == Some("변수")
            })
            .expect("unicode-named function handle should exist");
        assert!(function_handle.text.contains("def 변수"));
    }

    #[test]
    fn parse_extracts_javascript_function_and_class_symbol_names() {
        let provider = bundled_provider_for(Path::new("fixture.js"));
        let source =
            b"class Processor {\n}\n\nfunction processData(value) {\n  return value + 1;\n}\n";

        let handles = provider
            .parse(Path::new("fixture.js"), source)
            .expect("javascript parse should succeed");

        let class_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "class_declaration" && handle.name.as_deref() == Some("Processor")
            })
            .expect("class handle should exist");
        assert!(class_handle.text.contains("class Processor"));

        let function_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "function_declaration"
                    && handle.name.as_deref() == Some("processData")
            })
            .expect("function handle should exist");
        assert!(function_handle.text.contains("function processData"));
    }

    #[test]
    fn parse_extracts_typescript_symbol_names() {
        let provider = bundled_provider_for(Path::new("fixture.ts"));
        let source =
            b"class Processor<T> {\n}\n\nfunction processData(value: number): number {\n  return value + 1;\n}\n";

        let handles = provider
            .parse(Path::new("fixture.ts"), source)
            .expect("typescript parse should succeed");

        let class_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "class_declaration" && handle.name.as_deref() == Some("Processor")
            })
            .expect("class handle should exist");
        assert!(class_handle.text.contains("class Processor"));

        let function_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "function_declaration"
                    && handle.name.as_deref() == Some("processData")
            })
            .expect("function handle should exist");
        assert!(function_handle.text.contains("function processData"));
    }

    #[test]
    fn parse_accepts_tsx_function_with_jsx_return() {
        let provider = bundled_provider_for(Path::new("fixture.tsx"));
        let source = b"export function View(): JSX.Element {\n  return <div>Hello</div>;\n}\n";

        let handles = provider
            .parse(Path::new("fixture.tsx"), source)
            .expect("tsx parse should succeed");

        let function_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "function_declaration" && handle.name.as_deref() == Some("View")
            })
            .expect("function handle should exist");
        assert!(function_handle.text.contains("return <div>Hello</div>;"));
    }

    #[test]
    fn parse_accepts_jsx_function_with_jsx_return() {
        let provider = bundled_provider_for(Path::new("fixture.jsx"));
        let source = b"function View() {\n  return <div>Hello</div>;\n}\n";

        let handles = provider
            .parse(Path::new("fixture.jsx"), source)
            .expect("jsx parse should succeed");

        let function_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "function_declaration" && handle.name.as_deref() == Some("View")
            })
            .expect("function handle should exist");
        assert!(function_handle.text.contains("return <div>Hello</div>;"));
    }

    #[test]
    fn parse_extracts_rust_function_and_struct_symbol_names() {
        let provider = bundled_provider_for(Path::new("fixture.rs"));
        let source = b"struct Processor {\n    value: i32,\n}\n\nfn process_data(value: i32) -> i32 {\n    value + 1\n}\n";

        let handles = provider
            .parse(Path::new("fixture.rs"), source)
            .expect("rust parse should succeed");

        let struct_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "struct_item" && handle.name.as_deref() == Some("Processor")
            })
            .expect("struct handle should exist");
        assert!(struct_handle.text.contains("struct Processor"));

        let function_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "function_item" && handle.name.as_deref() == Some("process_data")
            })
            .expect("function handle should exist");
        assert!(function_handle.text.contains("fn process_data"));
    }

    #[test]
    fn parse_extracts_go_function_and_method_symbol_names() {
        let provider = bundled_provider_for(Path::new("fixture.go"));
        let source = b"type Processor struct {\n    value int\n}\n\nfunc processData(value int) int {\n    return value + 1\n}\n\nfunc (p Processor) helper() int {\n    return p.value + 1\n}\n";

        let handles = provider
            .parse(Path::new("fixture.go"), source)
            .expect("go parse should succeed");

        let function_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "function_declaration"
                    && handle.name.as_deref() == Some("processData")
            })
            .expect("function handle should exist");
        assert!(function_handle.text.contains("func processData"));

        let method_handle = handles
            .iter()
            .find(|handle| {
                handle.kind == "method_declaration" && handle.name.as_deref() == Some("helper")
            })
            .expect("method handle should exist");
        assert!(method_handle.text.contains("func (p Processor) helper"));
    }

    #[test]
    fn parse_extracts_html_start_tag_text() {
        let provider = bundled_provider_for(Path::new("fixture.html"));
        let source =
            b"<html>\n  <body>\n    <section id=\"main\">Hello</section>\n  </body>\n</html>\n";

        let handles = provider
            .parse(Path::new("fixture.html"), source)
            .expect("html parse should succeed");

        let section_start_tag = handles
            .iter()
            .find(|handle| {
                handle.kind == "start_tag" && handle.text.starts_with("<section id=\"main\">")
            })
            .expect("section start_tag handle should exist");
        assert!(section_start_tag.text.contains("<section id=\"main\">"));
    }

    #[test]
    fn parse_extracts_css_rule_text() {
        let provider = bundled_provider_for(Path::new("fixture.css"));
        let source = b"body {\n  color: red;\n}\n";

        let handles = provider
            .parse(Path::new("fixture.css"), source)
            .expect("css parse should succeed");

        let stylesheet_handle = handles
            .iter()
            .find(|handle| handle.kind == "stylesheet")
            .expect("stylesheet handle should exist");
        assert!(stylesheet_handle.text.contains("color: red;"));
    }

    #[test]
    fn parse_extracts_markdown_heading_and_list_item_text() {
        let provider = bundled_provider_for(Path::new("fixture.md"));
        let source = b"# Overview\n\n- item one\n- item two\n";

        let handles = provider
            .parse(Path::new("fixture.md"), source)
            .expect("markdown parse should succeed");

        let heading = handles
            .iter()
            .find(|handle| handle.kind == "atx_heading")
            .expect("atx_heading handle should exist");
        assert!(heading.text.starts_with("# Overview"));

        let list_item = handles
            .iter()
            .find(|handle| handle.kind == "list_item")
            .expect("list_item handle should exist");
        assert!(list_item.text.contains("item one"));
    }

    #[test]
    fn parse_extracts_protobuf_message_and_service_text() {
        let provider = bundled_provider_for(Path::new("fixture.proto"));
        let source = b"syntax = \"proto3\";\n\nmessage Request {\n  string id = 1;\n}\n\nservice Api {\n  rpc Get(Request) returns (Request);\n}\n";

        let handles = provider
            .parse(Path::new("fixture.proto"), source)
            .expect("protobuf parse should succeed");

        let message = handles
            .iter()
            .find(|handle| handle.kind == "message")
            .expect("message handle should exist");
        assert!(message.text.contains("message Request"));

        let service = handles
            .iter()
            .find(|handle| handle.kind == "service")
            .expect("service handle should exist");
        assert!(service.text.contains("service Api"));
    }

    #[test]
    fn parse_extracts_xml_element_and_attribute_text() {
        let provider = bundled_provider_for(Path::new("fixture.xml"));
        let source = b"<?xml version=\"1.0\"?><root><item id=\"first\">value</item></root>";

        let handles = provider
            .parse(Path::new("fixture.xml"), source)
            .expect("xml parse should succeed");

        let element = handles
            .iter()
            .find(|handle| handle.kind == "element")
            .expect("element handle should exist");
        assert!(element.text.contains("<item id=\"first\">value</item>"));

        let attribute = handles
            .iter()
            .find(|handle| handle.kind == "Attribute")
            .expect("Attribute handle should exist");
        assert!(attribute.text.contains("id=\"first\""));
    }

    #[test]
    fn parse_rejects_invalid_python_syntax() {
        let provider = TreeSitterProvider::python();
        let source = b"def broken(:\n    return 1\n";

        let error = provider
            .parse(Path::new("fixture.py"), source)
            .expect_err("invalid python should fail parse");

        match error {
            IdenteditError::ParseFailure { provider, message } => {
                assert_eq!(provider, "tree-sitter-python");
                assert!(message.contains("Syntax errors"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn can_handle_accepts_mixed_case_py_extension() {
        let provider = TreeSitterProvider::python();
        assert!(provider.can_handle(Path::new("fixture.PY")));
        assert!(provider.can_handle(Path::new("fixture.Py")));
    }

    #[test]
    fn can_handle_rejects_extensionless_paths() {
        let provider = TreeSitterProvider::python();
        assert!(!provider.can_handle(Path::new("fixture")));
    }

    #[test]
    fn bundled_tree_sitter_providers_include_python_spec() {
        let providers = TreeSitterProvider::bundled();
        assert_eq!(providers.len(), 30);

        let python = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-python")
            .expect("python provider");
        assert!(python.can_handle(Path::new("fixture.py")));

        let javascript = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-javascript")
            .expect("javascript provider");
        assert!(javascript.can_handle(Path::new("fixture.js")));
        assert!(javascript.can_handle(Path::new("fixture.jsx")));

        let c = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-c")
            .expect("c provider");
        assert!(c.can_handle(Path::new("fixture.c")));
        assert!(c.can_handle(Path::new("fixture.C")));
        assert!(!c.can_handle(Path::new("fixture.h")));

        let cpp = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-cpp")
            .expect("cpp provider");
        assert!(cpp.can_handle(Path::new("fixture.cpp")));
        assert!(cpp.can_handle(Path::new("fixture.cc")));
        assert!(cpp.can_handle(Path::new("fixture.cxx")));
        assert!(cpp.can_handle(Path::new("fixture.hpp")));
        assert!(cpp.can_handle(Path::new("fixture.hh")));
        assert!(cpp.can_handle(Path::new("fixture.hxx")));
        assert!(cpp.can_handle(Path::new("fixture.HPP")));
        assert!(!cpp.can_handle(Path::new("fixture.h")));

        let typescript = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-typescript")
            .expect("typescript provider");
        assert!(typescript.can_handle(Path::new("fixture.ts")));

        let tsx = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-tsx")
            .expect("tsx provider");
        assert!(tsx.can_handle(Path::new("fixture.tsx")));

        let rust = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-rust")
            .expect("rust provider");
        assert!(rust.can_handle(Path::new("fixture.rs")));

        let go = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-go")
            .expect("go provider");
        assert!(go.can_handle(Path::new("fixture.go")));

        let bash = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-bash")
            .expect("bash provider");
        assert!(bash.can_handle(Path::new("fixture.sh")));
        assert!(bash.can_handle(Path::new("fixture.bash")));
        assert!(bash.can_handle(Path::new("fixture.SH")));
        assert!(bash.can_handle(Path::new(".bashrc")));
        assert!(bash.can_handle(Path::new(".BASH_PROFILE")));

        let zsh = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-zsh")
            .expect("zsh provider");
        assert!(zsh.can_handle(Path::new("fixture.zsh")));
        assert!(zsh.can_handle(Path::new("fixture.ZSH")));
        assert!(zsh.can_handle(Path::new(".zshrc")));
        assert!(zsh.can_handle(Path::new(".ZSHENV")));

        let fish = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-fish")
            .expect("fish provider");
        assert!(fish.can_handle(Path::new("fixture.fish")));
        assert!(fish.can_handle(Path::new("fixture.FISH")));

        let php = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-php")
            .expect("php provider");
        assert!(php.can_handle(Path::new("fixture.php")));
        assert!(php.can_handle(Path::new("fixture.PHP")));

        let perl = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-perl")
            .expect("perl provider");
        assert!(perl.can_handle(Path::new("fixture.pl")));
        assert!(perl.can_handle(Path::new("fixture.pm")));
        assert!(perl.can_handle(Path::new("fixture.PM")));

        let ruby = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-ruby")
            .expect("ruby provider");
        assert!(ruby.can_handle(Path::new("fixture.rb")));
        assert!(ruby.can_handle(Path::new("fixture.RB")));

        let html = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-html")
            .expect("html provider");
        assert!(html.can_handle(Path::new("fixture.html")));
        assert!(html.can_handle(Path::new("fixture.HTM")));

        let css = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-css")
            .expect("css provider");
        assert!(css.can_handle(Path::new("fixture.css")));
        assert!(css.can_handle(Path::new("fixture.CSS")));

        let scss = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-scss")
            .expect("scss provider");
        assert!(scss.can_handle(Path::new("fixture.scss")));
        assert!(scss.can_handle(Path::new("fixture.SCSS")));

        let markdown = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-markdown")
            .expect("markdown provider");
        assert!(markdown.can_handle(Path::new("fixture.md")));
        assert!(markdown.can_handle(Path::new("fixture.markdown")));
        assert!(markdown.can_handle(Path::new("fixture.MARKDOWN")));

        let java = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-java")
            .expect("java provider");
        assert!(java.can_handle(Path::new("fixture.java")));
        assert!(java.can_handle(Path::new("fixture.JAVA")));

        let kotlin = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-kotlin")
            .expect("kotlin provider");
        assert!(kotlin.can_handle(Path::new("fixture.kt")));
        assert!(kotlin.can_handle(Path::new("fixture.kts")));
        assert!(kotlin.can_handle(Path::new("fixture.KTS")));

        let hcl = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-hcl")
            .expect("hcl provider");
        assert!(hcl.can_handle(Path::new("fixture.hcl")));
        assert!(hcl.can_handle(Path::new("fixture.tf")));
        assert!(hcl.can_handle(Path::new("fixture.TF")));

        let lua = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-lua")
            .expect("lua provider");
        assert!(lua.can_handle(Path::new("fixture.lua")));
        assert!(lua.can_handle(Path::new("fixture.LUA")));

        let csharp = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-c-sharp")
            .expect("csharp provider");
        assert!(csharp.can_handle(Path::new("fixture.cs")));
        assert!(csharp.can_handle(Path::new("fixture.CS")));

        let swift = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-swift")
            .expect("swift provider");
        assert!(swift.can_handle(Path::new("fixture.swift")));
        assert!(swift.can_handle(Path::new("fixture.SWIFT")));

        let dockerfile = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-dockerfile")
            .expect("dockerfile provider");
        assert!(dockerfile.can_handle(Path::new("fixture.dockerfile")));
        assert!(dockerfile.can_handle(Path::new("fixture.CONTAINERFILE")));
        assert!(dockerfile.can_handle(Path::new("Dockerfile")));
        assert!(dockerfile.can_handle(Path::new("CONTAINERFILE")));

        let sql = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-sql")
            .expect("sql provider");
        assert!(sql.can_handle(Path::new("fixture.sql")));
        assert!(sql.can_handle(Path::new("fixture.SQL")));

        let protobuf = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-proto")
            .expect("protobuf provider");
        assert!(protobuf.can_handle(Path::new("fixture.proto")));
        assert!(protobuf.can_handle(Path::new("fixture.PROTO")));

        let xml = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-xml")
            .expect("xml provider");
        assert!(xml.can_handle(Path::new("fixture.xml")));
        assert!(xml.can_handle(Path::new("fixture.XML")));

        let toml = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-toml")
            .expect("toml provider");
        assert!(toml.can_handle(Path::new("fixture.toml")));
        assert!(toml.can_handle(Path::new("fixture.TOML")));

        let yaml = providers
            .iter()
            .find(|provider| provider.name() == "tree-sitter-yaml")
            .expect("yaml provider");
        assert!(yaml.can_handle(Path::new("fixture.yaml")));
        assert!(yaml.can_handle(Path::new("fixture.yml")));
        assert!(yaml.can_handle(Path::new("fixture.YML")));
    }

    #[test]
    fn header_provider_handles_h_extension_case_insensitively() {
        let provider = HeaderTreeSitterProvider::new();
        assert!(provider.can_handle(Path::new("fixture.h")));
        assert!(provider.can_handle(Path::new("fixture.H")));
        assert!(!provider.can_handle(Path::new("fixture.hpp")));
        assert!(!provider.can_handle(Path::new("fixture.c")));
    }

    #[test]
    fn parse_c_cpp_header_prefers_cpp_when_both_parsers_succeed() {
        let source = b"int configure(int value);\n";
        let (_handles, dialect) = parse_c_cpp_header_with_dialect(Path::new("fixture.h"), source)
            .expect("dual parser should succeed when both grammars accept source");
        assert_eq!(dialect, HeaderDialect::Cpp);
    }

    #[test]
    fn parse_c_cpp_header_falls_back_to_c_when_cpp_has_errors() {
        let source = b"int configure(value)\nint value;\n{\n    return value;\n}\n";
        let (_handles, dialect) = parse_c_cpp_header_with_dialect(Path::new("fixture.h"), source)
            .expect("dual parser should fall back to C for C-only header syntax");
        assert_eq!(dialect, HeaderDialect::C);
    }

    #[test]
    fn parse_c_cpp_header_returns_parse_failure_when_both_grammars_fail() {
        let source = b"int broken( {\n";
        let error = parse_c_cpp_header_with_dialect(Path::new("fixture.h"), source)
            .expect_err("invalid header source should fail under both grammars");

        match error {
            IdenteditError::ParseFailure { provider, message } => {
                assert_eq!(provider, "tree-sitter-c-cpp-header");
                assert!(message.contains("C/C++ header"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn parse_propagates_dynamic_loader_failure_without_rewrapping() {
        let provider = TreeSitterProvider::from_spec(&DYNAMIC_FAILURE_SPEC);
        let error = provider
            .parse(Path::new("fixture.dyn"), b"ignored")
            .expect_err("dynamic language loader failure should bubble up");

        match error {
            IdenteditError::LanguageSetup { message } => {
                assert_eq!(message, "dynamic language loader unavailable");
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn parse_converts_dynamic_loader_panic_into_language_setup_error() {
        const PANIC_EXTENSIONS: &[&str] = &["panic"];
        const PANIC_SPEC: LanguageSpec = LanguageSpec {
            name: "tree-sitter-dynamic-panic-test",
            extensions: PANIC_EXTENSIONS,
            source: LanguageSource::DynamicLoader(load_dynamic_panicking_language),
            syntax_error_message: "unused syntax message",
            normalize_bare_cr: false,
        };

        let provider = TreeSitterProvider::from_spec(&PANIC_SPEC);
        let error = provider
            .parse(Path::new("fixture.panic"), b"ignored")
            .expect_err("dynamic panic should be converted to structured error");

        match error {
            IdenteditError::LanguageSetup { message } => {
                assert!(message.contains("panic while loading dynamic tree-sitter language"));
                assert!(message.contains("dynamic loader exploded"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn parse_converts_bundled_loader_panic_into_language_setup_error() {
        const PANIC_EXTENSIONS: &[&str] = &["panicbundled"];
        const PANIC_SPEC: LanguageSpec = LanguageSpec {
            name: "tree-sitter-bundled-panic-test",
            extensions: PANIC_EXTENSIONS,
            source: LanguageSource::Bundled(load_bundled_panicking_language),
            syntax_error_message: "unused syntax message",
            normalize_bare_cr: false,
        };

        let provider = TreeSitterProvider::from_spec(&PANIC_SPEC);
        let error = provider
            .parse(Path::new("fixture.panicbundled"), b"ignored")
            .expect_err("bundled panic should be converted to structured error");

        match error {
            IdenteditError::LanguageSetup { message } => {
                assert!(message.contains("panic while loading bundled tree-sitter language"));
                assert!(message.contains("bundled loader exploded"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn parse_converts_dynamic_non_string_panic_into_language_setup_error() {
        const PANIC_EXTENSIONS: &[&str] = &["panicnonstrdyn"];
        const PANIC_SPEC: LanguageSpec = LanguageSpec {
            name: "tree-sitter-dynamic-non-string-panic-test",
            extensions: PANIC_EXTENSIONS,
            source: LanguageSource::DynamicLoader(load_dynamic_non_string_panicking_language),
            syntax_error_message: "unused syntax message",
            normalize_bare_cr: false,
        };

        let provider = TreeSitterProvider::from_spec(&PANIC_SPEC);
        let error = provider
            .parse(Path::new("fixture.panicnonstrdyn"), b"ignored")
            .expect_err("dynamic non-string panic should be converted to structured error");

        match error {
            IdenteditError::LanguageSetup { message } => {
                assert!(message.contains("panic while loading dynamic tree-sitter language"));
                assert!(message.contains("unknown panic payload"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn parse_converts_bundled_non_string_panic_into_language_setup_error() {
        const PANIC_EXTENSIONS: &[&str] = &["panicnonstrbundled"];
        const PANIC_SPEC: LanguageSpec = LanguageSpec {
            name: "tree-sitter-bundled-non-string-panic-test",
            extensions: PANIC_EXTENSIONS,
            source: LanguageSource::Bundled(load_bundled_non_string_panicking_language),
            syntax_error_message: "unused syntax message",
            normalize_bare_cr: false,
        };

        let provider = TreeSitterProvider::from_spec(&PANIC_SPEC);
        let error = provider
            .parse(Path::new("fixture.panicnonstrbundled"), b"ignored")
            .expect_err("bundled non-string panic should be converted to structured error");

        match error {
            IdenteditError::LanguageSetup { message } => {
                assert!(message.contains("panic while loading bundled tree-sitter language"));
                assert!(message.contains("unknown panic payload"));
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn parse_uses_language_spec_specific_syntax_error_message() {
        let provider = TreeSitterProvider::from_spec(&CUSTOM_SYNTAX_MESSAGE_SPEC);
        let source = b"def broken(:\n    return 1\n";
        let error = provider
            .parse(Path::new("fixture.pyspec"), source)
            .expect_err("invalid python should fail parse");

        match error {
            IdenteditError::ParseFailure { provider, message } => {
                assert_eq!(provider, CUSTOM_SYNTAX_MESSAGE_SPEC.name);
                assert_eq!(
                    message,
                    "Custom syntax error emitted from language spec".to_string()
                );
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn can_handle_matches_multiple_extensions_case_insensitively() {
        let provider = TreeSitterProvider::from_spec(&MULTI_EXTENSION_SPEC);

        assert!(provider.can_handle(Path::new("fixture.py")));
        assert!(provider.can_handle(Path::new("fixture.PYI")));
        assert!(provider.can_handle(Path::new("fixture.PyW")));
        assert!(!provider.can_handle(Path::new("fixture.rs")));
    }

    #[test]
    fn can_handle_ignores_invalid_extension_tokens_in_language_spec() {
        let provider = TreeSitterProvider::from_spec(&INVALID_EXTENSION_SPEC);

        assert!(provider.can_handle(Path::new("fixture.py")));
        assert!(provider.can_handle(Path::new("fixture.PYI")));
        assert!(!provider.can_handle(Path::new("fixture.txt")));
    }
}
