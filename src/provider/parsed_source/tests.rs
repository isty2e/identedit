use std::path::Path;

use crate::error::IdenteditError;
use crate::handle::compute_identity;
use crate::hash::hash_text;
use crate::provider::{JsonProvider, ProviderRegistry, StructureProvider};

#[test]
fn deferred_handles_preserve_original_bytes_and_metadata() {
    let cases: &[(&str, &str)] = &[
        ("fixture.py", "def first(value):\n    return value + 1\n"),
        ("fixture.py", "def first(value):\r    return value + 1\r"),
        ("fixture.py", "def 이름(value):\r\n    return value + 1\r\n"),
        ("fixture.rs", "fn first() -> i32 { 1 }\n"),
        (
            "fixture.json",
            "{\"na\\u006de\":\"va\\u004Cue\",\"other\":true}\n",
        ),
        (
            "fixture.h",
            "int first(value)\nint value;\n{ return value; }\n",
        ),
        (
            "fixture.h",
            "template <typename T> struct Box { T value; };\r",
        ),
        (
            "fixture.unknown",
            "function first(value) { return value + 1; }\n",
        ),
    ];
    let registry = ProviderRegistry::default();

    for (file, source) in cases {
        let path = Path::new(file);
        let provider = registry.provider_for(path).unwrap();
        let parsed = provider.parse_source(path, source.as_bytes()).unwrap();
        let handles = parsed.into_handles(path);

        assert!(!handles.is_empty(), "{file}");
        for handle in handles {
            assert_eq!(handle.file, path);
            assert_eq!(handle.text, source[handle.span.start..handle.span.end]);
            assert_eq!(handle.expected_old_hash, hash_text(&handle.text));
            assert_eq!(
                handle.identity,
                compute_identity(&handle.kind, handle.name.as_deref(), &handle.text)
            );
        }
    }
}

#[test]
fn deferred_json_projection_preserves_decoding_and_duplicate_occurrences() {
    let path = Path::new("fixture.json");
    let source = br#"{"outer.part":{"inner::key":true,"inner::key":false},"a\u0062":1}"#;
    let parsed = JsonProvider.parse_source(path, source).unwrap();
    let handles = parsed.into_handles(path);
    let booleans = handles
        .iter()
        .filter(|handle| handle.kind == "boolean")
        .map(|handle| (handle.name.as_deref(), handle.text.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(
        booleans,
        vec![(Some("inner::key"), "true"), (Some("inner::key"), "false")]
    );
    let decoded_key = handles
        .iter()
        .find(|handle| handle.kind == "key" && handle.name.as_deref() == Some("ab"))
        .unwrap();
    assert_eq!(decoded_key.text, r#""a\u0062""#);
    let number = handles
        .iter()
        .find(|handle| handle.kind == "number")
        .unwrap();
    assert_eq!(number.name.as_deref(), Some("ab"));
    assert_eq!(number.text, "1");
}

#[test]
fn source_observations_preserve_rejection_and_fallback_noncoverage() {
    let registry = ProviderRegistry::default();
    for (file, source) in [
        ("fixture.py", "def broken(:"),
        ("fixture.json", "{\"missing\":}"),
        ("fixture.h", "int broken( {"),
    ] {
        let provider = registry.provider_for(Path::new(file)).unwrap();
        assert!(matches!(
            provider.parse_source(Path::new(file), source.as_bytes()),
            Err(IdenteditError::ParseFailure { .. })
        ));
    }

    let path = Path::new("fixture.unknown");
    let fallback = registry.provider_for(path).unwrap();
    assert!(!fallback.supports_syntax_validation());
    assert!(
        fallback
            .parse_source(path, b"plain text\n")
            .unwrap()
            .into_handles(path)
            .is_empty()
    );
    assert!(matches!(
        fallback.parse_source(path, &[0xff]),
        Err(IdenteditError::ParseFailure { .. })
    ));
}
