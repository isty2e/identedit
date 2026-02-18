use std::sync::OnceLock;

use regex::Regex;

use super::{BoundaryKind, Pattern};

pub(super) fn fallback_patterns() -> &'static [Pattern] {
    static PATTERNS: OnceLock<Vec<Pattern>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            let js_identifier_start =
                r"(?:[\p{L}_$]|\\u[0-9A-Fa-f]{4}|\\u\{[0-9A-Fa-f]+\})";
            let js_identifier_continue =
                r"(?:[\p{L}\p{M}\p{N}_$\x{200C}\x{200D}]|\\u[0-9A-Fa-f]{4}|\\u\{[0-9A-Fa-f]+\})";
            let js_identifier = format!(r"(?:{js_identifier_start}{js_identifier_continue}*)");

            vec![
                Pattern {
                    regex: Regex::new(
                        r"^\s*class\s+([\p{L}_][\p{L}\p{M}\p{N}_]*)\b[^\n]*:\s*(?:#.*)?$",
                    )
                    .expect("python class regex should compile"),
                    kind: "class_definition",
                    boundary: BoundaryKind::Indentation,
                    suppress_in_python_multiline: true,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(
                        r"^\s*(?:async\s+)?def\s+([\p{L}_][\p{L}\p{M}\p{N}_]*)\b[^\n]*:\s*(?:#.*)?$",
                    )
                    .expect("python function regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Indentation,
                    suppress_in_python_multiline: true,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(
                        r#"^\s*(?:#\s*\[[^\]\n]+\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:(?:const|async|unsafe|extern(?:\s+"[^"]+")?)\s+)*fn\s+((?:r#)?[\p{L}_][\p{L}\p{M}\p{N}_]*)\b"#,
                    )
                    .expect("rust function regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(
                        r"^\s*func\s+(?:\([^)]*\)\s*)?([\p{L}_][\p{L}\p{M}\p{N}_]*)\b",
                    )
                    .expect("go function regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*(?:export\s+(?:default\s+)?)?(?:async\s+)?function\s*\*?\s*({js_identifier})(?:\s*<[^(\n]+>)?\s*\("
                    ))
                    .expect("javascript function regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*(?:module\.)?exports\.({js_identifier})\s*=\s*(?:async\s+)?function(?:\s+{js_identifier})?\s*\("
                    ))
                    .expect("commonjs property function regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*(?:module\.)?exports\s*=\s*(?:async\s+)?function\s+({js_identifier})\s*\("
                    ))
                    .expect("commonjs default function regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*(?:async\s+)?({js_identifier})(?:\s*<[^(\n]+>)?\s*\([^)]*\)\s*\{{"
                    ))
                    .expect("commonjs object method regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: true,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*({js_identifier})\s*:\s*(?:async\s+)?function(?:\s+{js_identifier})?\s*\("
                    ))
                    .expect("commonjs object function property regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: true,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*({js_identifier})\s*:\s*(?:async\s*)?(?:<[^(\n]+>\s*)?(?:\([^)]*\)|{js_identifier})\s*(?::\s*[^=\n]+)?\s*=>"
                    ))
                    .expect("commonjs object arrow property regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: true,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*(?:export\s+)?(?:const|let|var)\s+({js_identifier})\s*:\s*[^\n]+\s*=\s*(?:async\s*)?(?:<[^(\n]+>\s*)?(?:\([^)]*\)|{js_identifier})\s*(?::\s*[^=\n]+)?\s*=>"
                    ))
                    .expect("typescript typed arrow function regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::HeaderLine,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*(?:export\s+)?(?:const|let|var)\s+({js_identifier})\s*=\s*(?:async\s*)?(?:<[^(\n]+>\s*)?(?:\([^)]*\)|{js_identifier})\s*(?::\s*[^=\n]+)?\s*=>"
                    ))
                    .expect("javascript arrow function regex should compile"),
                    kind: "function_definition",
                    boundary: BoundaryKind::HeaderLine,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: false,
                },
                Pattern {
                    regex: Regex::new(&format!(
                        r"^\s*(?:export\s+(?:default\s+)?)?(?:abstract\s+)?class\s+({js_identifier})\b"
                    ))
                    .expect("class regex should compile"),
                    kind: "class_definition",
                    boundary: BoundaryKind::Braces,
                    suppress_in_python_multiline: false,
                    requires_commonjs_exports_object_top_level: false,
                },
            ]
        })
        .as_slice()
}
