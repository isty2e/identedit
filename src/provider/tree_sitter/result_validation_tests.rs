use std::path::Path;

use super::TreeSitterProvider;
use super::catalog::{LanguageSource, LanguageSpec, load_python_language};
use crate::error::IdenteditError;
use crate::provider::StructureProvider;

#[test]
fn original_empty_blocks_remain_readable_but_proposed_blocks_name_the_owner_line() {
    let provider = TreeSitterProvider::python();
    let path = Path::new("example.py");
    for (source, owner_line) in [
        ("def target():\n", 1),
        ("class Outer:\n    def target():\n", 2),
        ("class Outer:\r    def target():\r", 2),
        ("if flag:\n    pass\nelse:\n", 3),
        ("match value:\n", 1),
        ("\u{feff}def target():\r\n", 1),
    ] {
        assert!(provider.parse_source(path, source.as_bytes()).is_ok());
        let error = provider
            .parse_proposed_source(path, source.as_bytes())
            .err()
            .expect("an empty required result block must be rejected");
        let IdenteditError::ParseFailure { provider, message } = error else {
            panic!("expected provider parse failure, got {error:?}");
        };
        assert_eq!(provider, "tree-sitter-python");
        assert!(
            message.ends_with(&format!("line {owner_line}")),
            "{message}"
        );
    }
}

#[test]
fn empty_modules_and_valid_python_suites_are_admitted() {
    let provider = TreeSitterProvider::python();
    let path = Path::new("example.py");
    for source in [
        "",
        "# comment only\n",
        "def target(): pass\n",
        "def target():\n    pass\n",
        "class Outer:\n    pass\n",
        "match value:\n    case _: pass\n",
        "def target(): \\\n a(); \\\n        b()\n",
        "def target():\n    \\\n        a()\n    b()\n",
        "def target():\n    a()\n\x0c    b()\n",
        "\u{feff}def target():\r\n    pass\r\n",
    ] {
        assert!(
            provider
                .parse_proposed_source(path, source.as_bytes())
                .is_ok(),
            "valid suite rejected: {source:?}"
        );
    }
}

fn load_dynamic_python() -> Result<tree_sitter::Language, IdenteditError> {
    Ok(load_python_language())
}

#[test]
fn grammar_identity_not_display_name_or_extension_controls_the_result_check() {
    const ALIAS: LanguageSpec = LanguageSpec {
        name: "unrelated-display-name",
        extensions: &["other"],
        source: LanguageSource::Bundled(load_python_language),
        syntax_error_message: "invalid source",
        normalize_bare_cr: true,
    };
    const DYNAMIC: LanguageSpec = LanguageSpec {
        name: "tree-sitter-python",
        extensions: &["py"],
        source: LanguageSource::DynamicLoader(load_dynamic_python),
        syntax_error_message: "invalid source",
        normalize_bare_cr: true,
    };
    let source = b"def target():\n";
    assert!(
        TreeSitterProvider::from_spec(&ALIAS)
            .parse_proposed_source(Path::new("example.other"), source)
            .is_err()
    );
    assert!(
        TreeSitterProvider::from_spec(&DYNAMIC)
            .parse_proposed_source(Path::new("example.py"), source)
            .is_ok()
    );
}
