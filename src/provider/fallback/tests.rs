use std::path::Path;

use super::FallbackProvider;
use crate::error::IdenteditError;
use crate::provider::StructureProvider;

#[test]
fn commonjs_exports_top_level_mask_marks_exported_property_lines() {
    let source = "module.exports = {\n  parse(value) {\n    return value + 4;\n  },\n  build(value) {\n    return value + 5;\n  },\n};\n";
    let lines = super::collect_lines(source);
    let mask = super::build_commonjs_exports_top_level_mask(source.as_bytes(), &lines);

    assert_eq!(mask.len(), lines.len());
    assert!(mask[1], "first exported property line should be marked");
    assert!(mask[4], "second exported property line should be marked");
}

#[test]
fn parse_extracts_python_indentation_blocks() {
    let source = "class Worker:\n    def run(self):\n        return 1\n\ndef helper(value):\n    return value + 1\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "class_definition"
                && handle.name.as_deref() == Some("Worker")
                && handle.text.contains("def run(self)")
        }),
        "class block should include nested method body"
    );
    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("run")
                && handle.text.contains("return 1")
        }),
        "nested function should be extracted with indentation-based span"
    );
    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("helper")
                && handle.text.contains("return value + 1")
        }),
        "top-level function should be extracted"
    );
}

#[test]
fn parse_extracts_python_async_function_block() {
    let source = "async def load_data(value):\n    await process(value)\n    return value\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("load_data")
                && handle.text.contains("await process(value)")
                && handle.text.contains("return value")
        }),
        "async def should be extracted with indentation-based body span"
    );
}

#[test]
fn parse_extracts_python_multiline_signature_function_block() {
    let source = "def compute(\n    left,\n    right,\n):\n    return left + right\n\ndef tail():\n    return 0\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let compute = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("compute"))
        .expect("multiline Python function should be detected");
    assert_eq!(compute.kind, "function_definition");
    assert!(
        compute.text.contains("return left + right"),
        "multiline Python function body should be included in the span"
    );
    assert!(
        !compute.text.contains("def tail"),
        "multiline Python function span should stop before the next declaration"
    );
}

#[test]
fn parse_extracts_brace_blocks_for_class_and_function() {
    let source = "\
class Box {\n\
  value() {\n\
    return 1;\n\
  }\n\
}\n\
\n\
function process() {\n\
  return 2;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "class_definition"
                && handle.name.as_deref() == Some("Box")
                && handle.text.contains("value()")
                && handle.text.contains("return 1;")
        }),
        "brace-delimited class should include nested body"
    );
    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("process")
                && handle.text.contains("return 2;")
        }),
        "brace-delimited function should include full body"
    );
}

#[test]
fn parse_extracts_export_default_function_block() {
    let source = "\
export default function parse() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("parse")
                && handle.text.contains("return 1;")
        }),
        "export default function should be extracted"
    );
}

#[test]
fn parse_extracts_export_default_class_block() {
    let source = "\
export default class Box {\n\
  value() {\n\
    return 1;\n\
  }\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "class_definition"
                && handle.name.as_deref() == Some("Box")
                && handle.text.contains("value()")
                && handle.text.contains("return 1;")
        }),
        "export default class should be extracted with full body"
    );
}

#[test]
fn parse_extracts_generator_function_block() {
    let source = "\
function* iter() {\n\
  yield 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("iter")
                && handle.text.contains("yield 1;")
        }),
        "generator function should be extracted"
    );
}

#[test]
fn parse_extracts_async_generator_function_block() {
    let source = "\
async function* iter() {\n\
  yield 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("iter")
                && handle.text.contains("yield 1;")
        }),
        "async generator function should be extracted"
    );
}

#[test]
fn parse_extracts_abstract_class_block() {
    let source = "\
abstract class Box {\n\
  value() {\n\
    return 1;\n\
  }\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "class_definition"
                && handle.name.as_deref() == Some("Box")
                && handle.text.contains("value()")
                && handle.text.contains("return 1;")
        }),
        "abstract class should be extracted with full body"
    );
}

#[test]
fn parse_extracts_rust_unsafe_function_block() {
    let source = "\
unsafe fn run() {\n\
    let _ = 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("run")
                && handle.text.contains("let _ = 1;")
        }),
        "unsafe fn should be extracted"
    );
}

#[test]
fn parse_extracts_rust_const_function_block() {
    let source = "\
const fn run() {\n\
    let _ = 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("run")
                && handle.text.contains("let _ = 1;")
        }),
        "const fn should be extracted"
    );
}

#[test]
fn parse_extracts_rust_extern_function_block() {
    let source = "\
extern \"C\" fn run() {\n\
    let _ = 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("run")
                && handle.text.contains("let _ = 1;")
        }),
        "extern fn should be extracted"
    );
}

#[test]
fn parse_extracts_rust_raw_identifier_function_block() {
    let source = "\
pub(crate) unsafe extern \"C\" fn r#match() {\n\
    let _ = 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("r#match")
                && handle.text.contains("let _ = 1;")
        }),
        "rust raw identifier function should be extracted"
    );
}

#[test]
fn parse_extracts_rust_function_with_inline_attribute_prefix() {
    let source = "\
#[inline(always)] pub fn fast(value: i32) -> i32 {\n\
    value + 1\n\
}\n\
const tail = 0;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let fast = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("fast"))
        .expect("rust function preceded by inline attribute should be detected");
    assert_eq!(fast.kind, "function_definition");
    assert!(
        fast.text.contains("value + 1"),
        "rust function span should include body"
    );
    assert!(
        !fast.text.contains("const tail"),
        "rust function boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_python_unicode_function_name() {
    let source = "def 함수명(x):\n    return x\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("함수명")
                && handle.text.contains("return x")
        }),
        "python unicode function name should be extracted"
    );
}

#[test]
fn parse_extracts_python_combining_mark_function_name() {
    let name = "cafe\u{301}";
    let source = format!("def {name}(x):\n    return x\n");
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some(name)
                && handle.text.contains("return x")
        }),
        "python combining-mark function name should be extracted without truncation"
    );
}

#[test]
fn parse_extracts_js_unicode_function_name() {
    let source = "\
function 함수명() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("함수명")
                && handle.text.contains("return 1;")
        }),
        "js unicode function name should be extracted"
    );
}

#[test]
fn parse_extracts_js_combining_mark_function_name() {
    let name = "cafe\u{301}";
    let source = format!("function {name}() {{\n  return 1;\n}}\n");
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some(name)
                && handle.text.contains("return 1;")
        }),
        "js combining-mark function name should be extracted without truncation"
    );
}

#[test]
fn parse_extracts_js_zwnj_function_name() {
    let name = "a\u{200C}b";
    let source = format!("function {name}() {{\n  return 1;\n}}\n");
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some(name)
                && handle.text.contains("return 1;")
        }),
        "js function name containing ZWNJ should be extracted"
    );
}

#[test]
fn parse_extracts_js_zwj_arrow_binding_name() {
    let name = "a\u{200D}b";
    let source = format!("const {name} = (value) => value + 1;\n");
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some(name)
                && handle.text == format!("const {name} = (value) => value + 1;\n")
        }),
        "js arrow binding name containing ZWJ should be extracted"
    );
}

#[test]
fn parse_extracts_js_unicode_escape_function_name() {
    let name = "\\u0066oo";
    let source = format!("function {name}() {{\n  return 1;\n}}\n");
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some(name)
                && handle.text.contains("return 1;")
        }),
        "js function name using unicode escape should be extracted"
    );
}

#[test]
fn parse_extracts_js_unicode_codepoint_escape_arrow_name() {
    let name = "\\u{66}oo";
    let source = format!("const {name} = () => 1;\n");
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some(name)
                && handle.text == format!("const {name} = () => 1;\n")
        }),
        "js arrow binding using unicode codepoint escape should be extracted"
    );
}

#[test]
fn parse_extracts_rust_unicode_function_name() {
    let source = "\
fn 함수명() {\n\
    let _ = 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("함수명")
                && handle.text.contains("let _ = 1;")
        }),
        "rust unicode function name should be extracted"
    );
}

#[test]
fn parse_extracts_rust_combining_mark_function_name() {
    let name = "cafe\u{301}";
    let source = format!("fn {name}() {{\n    let _ = 1;\n}}\n");
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some(name)
                && handle.text.contains("let _ = 1;")
        }),
        "rust combining-mark function name should be extracted without truncation"
    );
}

#[test]
fn parse_extracts_go_unicode_function_name() {
    let source = "\
func 함수명() {\n\
    return\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("함수명")
                && handle.text.contains("return")
        }),
        "go unicode function name should be extracted"
    );
}

#[test]
fn parse_returns_empty_handles_for_plain_text() {
    let provider = FallbackProvider;
    let handles = provider
        .parse(
            Path::new("fixture.unknown"),
            b"plain text body\njust words\n",
        )
        .expect("fallback parse should succeed");

    assert!(
        handles.is_empty(),
        "no structural patterns should mean no handles"
    );
}

#[test]
fn parse_rejects_non_utf8_source() {
    let provider = FallbackProvider;
    let error = provider
        .parse(Path::new("fixture.unknown"), &[0xFF, 0xFE, 0xFD])
        .expect_err("non-utf8 content should fail in fallback parser");

    match error {
        IdenteditError::ParseFailure { provider, message } => {
            assert_eq!(provider, "fallback");
            assert!(message.contains("UTF-8"));
        }
        other => panic!("unexpected error variant: {other}"),
    }
}

#[test]
fn parse_supports_cr_only_indentation_blocks() {
    let source =
        "def alpha(value):\r    return value + 1\r\rdef beta(value):\r    return value + 2\r";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("alpha")
                && handle.text.contains("return value + 1")
        }),
        "CR-only source should still include first function body"
    );
    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("beta")
                && handle.text.contains("return value + 2")
        }),
        "CR-only source should still include second function body"
    );
}

#[test]
fn parse_arrow_function_uses_header_line_boundary() {
    let source = "const build = (x) => x + 1;\nconst keep = x + 2;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let arrow = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("build"))
        .expect("arrow function handle should exist");
    assert_eq!(arrow.kind, "function_definition");
    assert_eq!(arrow.text, "const build = (x) => x + 1;\n");
    assert!(
        !arrow.text.contains("keep"),
        "header-line boundary must not consume following statements"
    );
}

#[test]
fn parse_extracts_multiline_arrow_binding_with_block_body() {
    let source = "const build = (\n  value,\n) => {\n  return value + 1;\n};\nconst keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let build = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("build"))
        .expect("multiline arrow binding should be detected");
    assert_eq!(build.kind, "function_definition");
    assert!(
        build.text.contains("return value + 1;"),
        "multiline arrow span should include block body"
    );
    assert!(
        !build.text.contains("const keep"),
        "multiline arrow span should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_exported_arrow_binding_name() {
    let source = "export const built = (x) => x + 1;\nconst keep = x + 2;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let exported_arrow = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("built"))
        .expect("exported arrow binding handle should exist");
    assert_eq!(exported_arrow.kind, "function_definition");
    assert_eq!(exported_arrow.text, "export const built = (x) => x + 1;\n");
    assert!(
        !exported_arrow.text.contains("keep"),
        "header-line boundary must not consume trailing declarations"
    );
}

#[test]
fn parse_extracts_typed_typescript_arrow_binding_name() {
    let source =
        "export const build: (x: number) => number = async (x) => x + 1;\nconst keep = x + 2;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let typed_arrow = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("build"))
        .expect("typed arrow binding handle should exist");
    assert_eq!(typed_arrow.kind, "function_definition");
    assert_eq!(
        typed_arrow.text,
        "export const build: (x: number) => number = async (x) => x + 1;\n"
    );
    assert!(
        !typed_arrow.text.contains("keep"),
        "typed arrow boundary must not consume trailing declarations"
    );
}

#[test]
fn parse_extracts_commonjs_module_exports_function_expression() {
    let source = "\
module.exports.parse = function parse(value) {\n\
  return value + 1;\n\
};\n\
const keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let exported_function = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("commonjs module.exports function handle should exist");
    assert_eq!(exported_function.kind, "function_definition");
    assert!(
        exported_function.text.contains("return value + 1;"),
        "commonjs function body should be captured"
    );
    assert!(
        !exported_function.text.contains("const keep"),
        "commonjs function boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_commonjs_exports_function_expression() {
    let source = "\
exports.build = function(value) {\n\
  return value + 2;\n\
};\n\
const keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let exported_function = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("build"))
        .expect("commonjs exports function handle should exist");
    assert_eq!(exported_function.kind, "function_definition");
    assert!(
        exported_function.text.contains("return value + 2;"),
        "commonjs exports function body should be captured"
    );
    assert!(
        !exported_function.text.contains("const keep"),
        "commonjs exports function boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_commonjs_default_named_function_expression() {
    let source = "\
module.exports = function parse(value) {\n\
  return value + 3;\n\
};\n\
const keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let exported_function = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("commonjs default function handle should exist");
    assert_eq!(exported_function.kind, "function_definition");
    assert!(
        exported_function.text.contains("return value + 3;"),
        "commonjs default function body should be captured"
    );
    assert!(
        !exported_function.text.contains("const keep"),
        "commonjs default function boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_commonjs_module_exports_object_method_shorthand() {
    let source = "module.exports = {\n  parse(value) {\n    return value + 4;\n  },\n  build(value) {\n    return value + 5;\n  },\n};\nconst keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("commonjs object method handle should exist");
    let build = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("build"))
        .expect("second commonjs object method handle should exist");
    assert_eq!(parse.kind, "function_definition");
    assert_eq!(build.kind, "function_definition");
    assert!(
        parse.text.contains("return value + 4;"),
        "object method body should be captured"
    );
    assert!(
        build.text.contains("return value + 5;"),
        "second object method body should be captured"
    );
    assert!(
        !parse.text.contains("const keep") && !build.text.contains("const keep"),
        "object method boundaries should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_commonjs_module_exports_object_function_property() {
    let source = "module.exports = {\n  parse: function parse(value) {\n    return value + 6;\n  },\n};\nconst keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("commonjs object function-property handle should exist");
    assert_eq!(parse.kind, "function_definition");
    assert!(
        parse.text.contains("return value + 6;"),
        "object function-property body should be captured"
    );
    assert!(
        !parse.text.contains("const keep"),
        "object function-property boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_commonjs_exports_object_ignores_nested_object_methods() {
    let source = "module.exports = {\n  parse(value) {\n    const nested = {\n      helper(item) {\n        if (item) {\n          return item;\n        }\n        return 0;\n      },\n    };\n    return nested.helper(value);\n  },\n};\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let names: Vec<&str> = handles
        .iter()
        .map(|handle| {
            handle
                .name
                .as_deref()
                .expect("fallback candidate should have a name")
        })
        .collect();
    assert!(
        names.contains(&"parse"),
        "top-level exported method should be captured"
    );
    assert!(
        !names.contains(&"helper"),
        "nested object methods should not be treated as exported handles"
    );
    assert!(
        !names.contains(&"if"),
        "control-flow keywords inside method bodies must not become handles"
    );
}

#[test]
fn parse_commonjs_exports_object_tolerates_mixed_property_indentation() {
    let source = "module.exports = {\n\tparse(value) {\n\t\treturn value + 1;\n\t},\n  build(value) {\n    return value + 2;\n  },\n};\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("parse")),
        "tab-indented exported method should be detected"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("build")),
        "space-indented sibling exported method should also be detected"
    );
}

#[test]
fn parse_extracts_commonjs_object_arrow_function_property() {
    let source = "module.exports = {\n  parse: (value) => {\n    return value + 7;\n  },\n};\nconst keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("commonjs object arrow-function property should be detected");
    assert_eq!(parse.kind, "function_definition");
    assert!(
        parse.text.contains("return value + 7;"),
        "arrow-function property span should include body"
    );
    assert!(
        !parse.text.contains("const keep"),
        "arrow-function property boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_typescript_generic_function_declaration() {
    let source = "\
export function fetchData<T>(value: T) {\n\
  return value;\n\
}\n\
const keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let generic_function = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("fetchData"))
        .expect("typescript generic function handle should exist");
    assert_eq!(generic_function.kind, "function_definition");
    assert!(
        generic_function.text.contains("return value;"),
        "generic function body should be captured"
    );
    assert!(
        !generic_function.text.contains("const keep"),
        "generic function boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_typescript_generic_class_declaration() {
    let source = "\
export class Container<T> {\n\
  value(): T {\n\
    return this.item;\n\
  }\n\
}\n\
const keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let generic_class = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("Container"))
        .expect("typescript generic class handle should exist");
    assert_eq!(generic_class.kind, "class_definition");
    assert!(
        generic_class.text.contains("value(): T"),
        "generic class body should be captured"
    );
    assert!(
        !generic_class.text.contains("const keep"),
        "generic class boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_extracts_typescript_generic_arrow_binding_name() {
    let source = "const build = <T>(value: T): T => value;\nconst keep = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let generic_arrow = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("build"))
        .expect("typescript generic arrow handle should exist");
    assert_eq!(generic_arrow.kind, "function_definition");
    assert_eq!(
        generic_arrow.text,
        "const build = <T>(value: T): T => value;\n"
    );
    assert!(
        !generic_arrow.text.contains("keep"),
        "generic arrow boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_brace_matching_handles_nested_scopes() {
    let source = "\
function outer() {\n\
  if (flag) {\n\
    while (ready) {\n\
      work();\n\
    }\n\
  }\n\
}\n\
const tail = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let outer = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("outer"))
        .expect("outer function handle should exist");
    assert!(
        outer.text.contains("while (ready)"),
        "nested blocks should remain inside outer function span"
    );
    assert!(
        !outer.text.contains("const tail"),
        "brace boundary should stop before trailing statements"
    );
}

#[test]
fn parse_unclosed_brace_block_falls_back_to_header_line() {
    let source = "function broken() {\n  if (flag) {\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let broken = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("broken"))
        .expect("broken function handle should exist");
    assert_eq!(broken.text, "function broken() {\n");
}

#[test]
fn parse_function_with_open_brace_on_next_line_captures_full_body() {
    let source = "\
function parse(value)\n\
{\n\
  return value + 1;\n\
}\n\
const tail = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse.text.contains("return value + 1;"),
        "function span should include body even when opening brace is on the next line"
    );
    assert!(
        !parse.text.contains("const tail"),
        "function boundary should still end before following declarations"
    );
}

#[test]
fn parse_extracts_multiline_js_function_header_when_name_line_follows_keyword() {
    let source = "\
function\n\
parse(value) {\n\
  return value + 10;\n\
}\n\
const tail = true;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("multiline JS function header should be detected");
    assert_eq!(parse.kind, "function_definition");
    assert!(
        parse.text.contains("return value + 10;"),
        "multiline JS function span should include body"
    );
    assert!(
        !parse.text.contains("const tail"),
        "multiline JS function boundary should stop before trailing declarations"
    );
}

#[test]
fn parse_preserves_handle_order_and_span_roundtrip() {
    let source = "\
def alpha():\n\
    return 1\n\
\n\
function beta() {\n\
  return 2;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let names: Vec<&str> = handles
        .iter()
        .map(|handle| handle.name.as_deref().expect("name should exist"))
        .collect();
    assert_eq!(names, vec!["alpha", "beta"]);

    for handle in &handles {
        let slice = source
            .get(handle.span.start..handle.span.end)
            .expect("span should map to UTF-8 boundaries");
        assert_eq!(slice, handle.text);
    }
}

#[test]
fn parse_ignores_python_signatures_inside_triple_quoted_strings() {
    let source = "\
text = \"\"\"\n\
def fake_inside_docstring():\n\
    return 0\n\
\"\"\"\n\
\n\
def real_function():\n\
    return 1\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_inside_docstring")),
        "triple-quoted string contents must not be treated as real function signatures"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_function")),
        "real top-level function should still be detected"
    );
}

#[test]
fn parse_brace_matching_ignores_braces_inside_double_quoted_strings() {
    let source = "\
function parse() {\n\
  const marker = \"}\";\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "brace matcher should not terminate early on string literal braces"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "brace matcher should still stop before trailing declarations"
    );
}

#[test]
fn parse_brace_matching_ignores_braces_inside_line_comments() {
    let source = "\
function parse() {\n\
  // }\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "line-comment braces must not terminate the surrounding function span"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still end at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_ignores_braces_inside_block_comments() {
    let source = "\
function parse() {\n\
  /* } */\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "block-comment braces must not terminate the surrounding function span"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still end at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_ignores_braces_inside_template_literals() {
    let source = "\
function parse() {\n\
  const marker = `}`;\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "template-literal braces must not terminate the surrounding function span"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still end at the real closing brace"
    );
}

#[test]
fn parse_ignores_commented_out_signatures() {
    let source = "\
# def commented_python():\n\
// function commented_js() {\n\
/* class Commented {} */\n\
\n\
def real_python():\n\
    return 1\n\
\n\
function real_js() {\n\
  return 2;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("commented_python")),
        "commented Python signature should not become a handle"
    );
    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("commented_js")),
        "commented JS signature should not become a handle"
    );
    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition" && handle.name.as_deref() == Some("real_python")
        }),
        "real Python signature should still be detected"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "real JS signature should still be detected"
    );
}

#[test]
fn parse_ignores_signatures_inside_multiline_block_comments() {
    let source = "\
/*\n\
def fake_python():\n\
    return 0\n\
function fake_js() {\n\
  return 0;\n\
}\n\
*/\n\
\n\
def real_python():\n\
    return 1\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_python")),
        "multiline block-commented Python signature should be ignored"
    );
    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_js")),
        "multiline block-commented JS signature should be ignored"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_python")),
        "real function outside comments should still be detected"
    );
}

#[test]
fn parse_ignores_signature_tokens_inside_plain_quoted_strings() {
    let source = "\
\"function fake_from_double_quote() {\";\n\
'class FakeFromSingleQuote {';\n\
\"async def fake_from_python_quote(value):\";\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_from_double_quote")),
        "double-quoted JS signature tokens must not become handles"
    );
    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("FakeFromSingleQuote")),
        "single-quoted class tokens must not become handles"
    );
    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_from_python_quote")),
        "quoted Python def tokens must not become handles"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "real signatures outside strings must still be detected"
    );
}

#[test]
fn parse_ignores_signature_tokens_inside_single_line_template_literals() {
    let source = "\
`function fake_from_template() { return 0; }`;\n\
`class FakeFromTemplate {}`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_from_template")),
        "single-line template-literal function tokens must not become handles"
    );
    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("FakeFromTemplate")),
        "single-line template-literal class tokens must not become handles"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "real signatures outside template literals must still be detected"
    );
}

#[test]
fn parse_brace_matching_ignores_braces_inside_regex_literals() {
    let source = "\
function parse() {\n\
  const regex = /}/;\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex literal braces must not terminate the surrounding function span"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still end at the real closing brace"
    );
}

#[test]
fn parse_does_not_treat_comment_tokens_inside_strings_as_real_block_comments() {
    let source = "\
const marker = \"/* not a real comment\";\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "string-contained comment token must not suppress following real signatures"
    );
}

#[test]
fn parse_does_not_treat_comment_tokens_inside_regex_literals_as_real_block_comments() {
    let source = "\
const marker = /[/*]/;\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "regex-contained comment token must not suppress following real signatures"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literals_after_return_keyword() {
    let source = "\
function parse(value) {\n\
  return /}/.test(value);\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains(".test(value)"),
        "regex literal after return should stay inside function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literals_after_return_without_whitespace() {
    let source = "\
function parse(value) {\n\
  return/}/.test(value);\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains(".test(value)"),
        "regex literal after return keyword should stay inside function span even without whitespace"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literals_after_return_with_block_comment_gap() {
    let source = "\
function parse(value) {\n\
  return/* gap *//}/.test(value);\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains(".test(value)"),
        "regex literal should stay inside function span after block comment gap"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_ignores_signatures_inside_multiline_template_literals() {
    let source = "\
const template = `\n\
function fake_js() {\n\
  return 0;\n\
}\n\
`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_js")),
        "template-literal contents should not be treated as real signatures"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "real signature outside template literal should still be detected"
    );
}

#[test]
fn parse_python_triple_quote_state_ignores_hash_comments() {
    let source = "\
# \"\"\" this is only a comment token\n\
def real_function():\n\
    return 1\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_function")),
        "hash comments containing triple-quote tokens must not suppress real function signatures"
    );
}

#[test]
fn parse_python_triple_quote_state_ignores_triple_double_inside_single_quoted_string() {
    let source = "\
text = '\"\"\" not a docstring token'\n\
def real_function():\n\
    return 1\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_function")),
        "triple-double quote tokens inside single-quoted strings must not suppress real signatures"
    );
}

#[test]
fn parse_python_triple_quote_state_ignores_triple_single_inside_double_quoted_string() {
    let source = "\
text = \"''' not a docstring token\"\n\
def real_function():\n\
    return 1\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_function")),
        "triple-single quote tokens inside double-quoted strings must not suppress real signatures"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literals_after_if_condition() {
    let source = "\
function parse(value) {\n\
  if (flag) /}/.test(value);\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex literal after if condition should not terminate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literals_after_line_comment_boundary() {
    let source = "\
function parse(value) {\n\
  // comment line\n\
  /}/.test(value);\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex literal on line after a line comment should not terminate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literals_after_return_line_break() {
    let source = "\
function parse(value) {\n\
  return\n\
  /}/.test(value);\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("/}/.test(value);"),
        "regex literal after return line break should not terminate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literals_after_while_condition() {
    let source = "\
function parse(value) {\n\
  while (keepGoing) /}/.test(value);\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex literal after while condition should not terminate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_after_if_condition_with_comment_paren_noise() {
    let source = "\
function parse(value) {\n\
  if (value /* ) noise */) /}/.test(value);\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex literal should not terminate function span when if-condition comment contains paren noise"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_after_if_condition_with_string_paren_noise() {
    let source = "\
function parse(value) {\n\
  if (value === \")\") /}/.test(value);\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex literal should not terminate function span when if-condition string contains paren noise"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_ignores_signatures_inside_nested_template_literal_expressions() {
    let source = "\
const tpl = `start ${\n\
  `inner`\n\
  function fake_js() {\n\
    return 0;\n\
  }\n\
}`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_js")),
        "nested template-literal expression contents should not leak signatures"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "real signature outside template literal should still be detected"
    );
}

#[test]
fn parse_ignores_signatures_inside_nested_multiline_template_literal_expressions() {
    let source = "\
const tpl = `start ${\n\
  `\n\
function fake_js() {\n\
  return 0;\n\
}\n\
  `\n\
}`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_js")),
        "multiline nested template-literal expression contents should not leak signatures"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "real signature outside template literal should still be detected"
    );
}

#[test]
fn parse_ignores_brace_noise_inside_template_expression_string_literals() {
    let source = "\
const tpl = `start ${\n\
  \"{\"\n\
}`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "brace tokens inside template-expression string literals must not suppress real signatures"
    );
}

#[test]
fn parse_ignores_brace_noise_inside_template_expression_block_comments() {
    let source = "\
const tpl = `start ${\n\
  /* { noise */\n\
  value\n\
}`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "brace tokens inside template-expression block comments must not suppress real signatures"
    );
}

#[test]
fn parse_ignores_brace_noise_inside_nested_template_raw_text() {
    let source = "\
const tpl = `start ${\n\
  `raw { brace`\n\
}`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "brace tokens inside nested template raw text must not suppress real signatures"
    );
}

#[test]
fn parse_brace_matching_handles_division_after_postfix_increment() {
    let source = "\
function parse(value) {\n\
  const ratio = value++/2;\n\
  return ratio;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return ratio;"),
        "division after postfix increment must not trigger regex mode and truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_division_after_postfix_decrement() {
    let source = "\
function parse(value) {\n\
  const ratio = value--/2;\n\
  return ratio;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return ratio;"),
        "division after postfix decrement must not trigger regex mode and truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_after_if_condition_with_postfix_increment_division() {
    let source = "\
function parse(value) {\n\
  if (value++/2 > 0) /}/.test(value);\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex after if-condition with postfix increment division should not truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_ignores_brace_noise_inside_nested_template_with_interpolation() {
    let source = "\
const tpl = `start ${\n\
  `inner ${value} { brace`\n\
}`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "nested template interpolation with raw braces must not suppress real signatures"
    );
}

#[test]
fn parse_brace_matching_handles_division_at_line_start_after_parenthesized_expression() {
    let source = "\
function parse(value) {\n\
  const ratio = (\n\
    value + 1\n\
  )\n\
  / 2;\n\
  return ratio;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return ratio;"),
        "line-start division after multiline parenthesized expression must not trigger regex mode"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_ignores_signatures_inside_deeply_nested_template_interpolation_backticks() {
    let source = "\
const tpl = `start ${\n\
  `inner ${`deep\n\
function fake_js() {\n\
  return 0;\n\
}\n\
`} brace`\n\
}`;\n\
\n\
function real_js() {\n\
  return 1;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles
            .iter()
            .all(|handle| handle.name.as_deref() != Some("fake_js")),
        "deeply nested template interpolation/backtick contents should not leak signatures"
    );
    assert!(
        handles
            .iter()
            .any(|handle| handle.name.as_deref() == Some("real_js")),
        "real signature outside nested templates should still be detected"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literal_after_do_keyword() {
    let source = "\
function parse(value) {\n\
  do /}/.test(value); while (false);\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex literal after do keyword must not truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literal_after_else_keyword() {
    let source = "\
function parse(value) {\n\
  if (flag) value();\n\
  else /}/.test(value);\n\
  return 1;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return 1;"),
        "regex literal after else keyword must not truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literal_after_extends_keyword() {
    let source = "\
class Parser extends /}/.constructor {\n\
  value() {\n\
    return 1;\n\
  }\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parser_class = handles
        .iter()
        .find(|handle| {
            handle.kind == "class_definition" && handle.name.as_deref() == Some("Parser")
        })
        .expect("class handle should exist");
    assert!(
        parser_class.text.contains("return 1;"),
        "regex literal after extends keyword must not truncate class span"
    );
    assert!(
        parser_class.text.ends_with("}\n"),
        "class span should include the real closing brace line"
    );
    assert!(
        !parser_class.text.contains("const tail"),
        "class span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literal_after_extends_in_class_expression() {
    let source = "\
function parse(value) {\n\
  const Derived = class extends /}/.constructor {};\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| {
            handle.kind == "function_definition" && handle.name.as_deref() == Some("parse")
        })
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "regex literal after extends in class expression must not truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literal_after_default_keyword() {
    let source = "\
function parse(value) {\n\
  default /}/.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| {
            handle.kind == "function_definition" && handle.name.as_deref() == Some("parse")
        })
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "regex literal after default keyword must not truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_line_start_division_after_code_with_trailing_comment() {
    let source = "\
function parse(value) {\n\
  const ratio = value + 1; // trailing context\n\
  / 2;\n\
  return ratio;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return ratio;"),
        "line-start division should respect code predecessor even when prior line has trailing comment"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_line_start_division_after_plain_semicolon() {
    let source = "\
function parse(value) {\n\
  const ratio = value + 1;\n\
  / 2;\n\
  return ratio;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return ratio;"),
        "line-start division should remain non-regex after a plain semicolon predecessor"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_ignores_line_comment_after_newline_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  // } comment noise\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "line comment after newline semicolon must not terminate function span via brace noise"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_treats_u2028_as_line_terminator_for_line_comments() {
    let source = "const x = 1; // comment\u{2028}function parsed() {\n  return 1;\n}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("parsed")
                && handle.text.contains("return 1;")
        }),
        "U+2028 should terminate line comments so following function is parsed"
    );
}

#[test]
fn parse_brace_matching_treats_u2029_as_line_terminator_for_line_comments() {
    let source = "const x = 1; // comment\u{2029}function parsed() {\n  return 1;\n}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    assert!(
        handles.iter().any(|handle| {
            handle.kind == "function_definition"
                && handle.name.as_deref() == Some("parsed")
                && handle.text.contains("return 1;")
        }),
        "U+2029 should terminate line comments so following function is parsed"
    );
}

#[test]
fn parse_brace_matching_keeps_function_boundary_with_u2028_line_comment_break() {
    let source = "\
function parse() {\n\
  const marker = 1; // keep scanning\u{2028}  return marker;\n\
}\n\
const tail = 0;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function should be parsed");
    assert!(
        parse_fn.text.contains("return marker;"),
        "function body should continue after U+2028 comment boundary"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_after_u2028_line_boundary() {
    let source = "\
function parse() {\n\
  const before = 1;\u{2028}  const regex = /abc/.test(\"abc\");\n\
  return regex;\n\
}\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function should be parsed");
    assert!(
        parse_fn.text.contains("const regex = /abc/.test(\"abc\");"),
        "regex literal after U+2028 boundary should not break brace matching"
    );
}

#[test]
fn parse_brace_matching_handles_regex_line_start_after_u2028_separator() {
    let source = "\
function parse() {\n\
  const x = 1;\u{2028}/{/.test(\"{\");\n\
  return 1;\n\
}\n\
const tail = 0;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function should be parsed");
    assert!(
        parse_fn.text.contains("return 1;"),
        "function body should remain intact after regex literal starts on U+2028-separated line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_line_start_after_u2029_separator() {
    let source = "\
function parse() {\n\
  const x = 1;\u{2029}/{/.test(\"{\");\n\
  return 1;\n\
}\n\
const tail = 0;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function should be parsed");
    assert!(
        parse_fn.text.contains("return 1;"),
        "function body should remain intact after regex literal starts on U+2029-separated line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the closing brace"
    );
}

#[test]
fn parse_brace_matching_ignores_block_comment_after_newline_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  /* } block noise */\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "block comment after newline semicolon must not terminate function span via brace noise"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_line_start_division_with_quoted_slash_after_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  / 2 + \"path/segment\".length;\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "quoted slash token after newline semicolon should not trigger regex-mode truncation"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_line_start_division_with_trailing_string_slash_after_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  / 2 + \"a/\".length;\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "trailing slash inside quoted token after newline semicolon should not trigger regex-mode truncation"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_line_start_division_with_url_like_string_after_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  / 2 + \"https://example.com\".length;\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "URL-like slash sequence after newline semicolon should not trigger regex-mode truncation"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_line_comment_after_division_under_semicolon_context() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  / 2 // } trailing comment noise\n\
  + 1;\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "line-comment start slash after division should not terminate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_escaped_regex_with_flags_after_newline_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  /a\\\\/b/gi.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "escaped regex with flags after newline semicolon should remain a valid regex literal context"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_with_comment_like_char_class_after_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  /[/*}]*/.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "regex with [/*] char-class sequence should not be rejected as block-comment noise"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_escaped_double_slash_regex_after_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  /\\/\\/.*/.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "escaped double-slash regex after newline semicolon should remain recognized as regex"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_protocol_like_regex_after_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  /https?:\\/\\/[^\\s]+/i.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "protocol-like regex with :// sequence after newline semicolon should remain recognized"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_after_block_comment_only_predecessor_line() {
    let source = "\
function parse(value) {\n\
  /* standalone predecessor */\n\
  /}/.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "regex literal should remain recognized when predecessor line is only a block comment"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_after_line_comment_only_predecessor_line() {
    let source = "\
function parse(value) {\n\
  // standalone predecessor\n\
  /}/.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "regex literal should remain recognized when predecessor line is only a line comment"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_after_multiline_block_comment_only_predecessor() {
    let source = "\
function parse(value) {\n\
  /* standalone predecessor\n\
   * spanning multiple lines */\n\
  /}/.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "regex literal should remain recognized after multiline block-comment-only predecessor"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_division_with_url_line_comment_tail_after_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  / 2 + value; // https://example.com/path } tail\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "URL-like line comment tail after division should not terminate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_line_start_division_with_block_comment_close_slash_after_semicolon()
{
    let source = "\
function parse(value) {\n\
  doThing();\n\
  / 2 /* } marker */ + 1;\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "block-comment close slash after newline semicolon should not trigger regex-mode truncation"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_line_start_division_after_code_with_block_comment_tail() {
    let source = "\
function parse(value) {\n\
  const ratio = value + 1; /* trailing context */\n\
  / 2;\n\
  return ratio;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return ratio;"),
        "line-start division should respect predecessor even when prior line ends with a block-comment tail"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literal_with_char_class_after_newline_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  /[\\\\/]}/.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "regex literal with char class after newline semicolon must not truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_brace_matching_handles_regex_literal_after_newline_semicolon() {
    let source = "\
function parse(value) {\n\
  doThing();\n\
  /}/.test(value);\n\
  return value;\n\
}\n\
const tail = 99;\n";
    let provider = FallbackProvider;
    let handles = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    let parse_fn = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("parse"))
        .expect("function handle should exist");
    assert!(
        parse_fn.text.contains("return value;"),
        "regex literal after newline semicolon must not truncate function span"
    );
    assert!(
        parse_fn.text.ends_with("}\n"),
        "function span should include the real closing brace line"
    );
    assert!(
        !parse_fn.text.contains("const tail"),
        "function span should still stop at the real closing brace"
    );
}

#[test]
fn parse_is_deterministic_under_mixed_noise_input() {
    let source = "\
text = \"\"\"\n\
def fake_inside_docstring():\n\
    return 0\n\
\"\"\"\n\
\n\
def real_python():\n\
    return 1\n\
\n\
function real_js() {\n\
  const marker = \"}\";\n\
  // }\n\
  /* } */\n\
  return 2;\n\
}\n";
    let provider = FallbackProvider;
    let first = provider
        .parse(Path::new("fixture.unknown"), source.as_bytes())
        .expect("fallback parse should succeed");

    for _ in 0..32 {
        let next = provider
            .parse(Path::new("fixture.unknown"), source.as_bytes())
            .expect("fallback parse should be deterministic");
        assert_eq!(next, first, "fallback parse output must be deterministic");
    }
}
