use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use crate::error::IdenteditError;
use crate::provider::{ParsedSource, ProviderRegistry, StructureProvider, TreeSitterProvider};

use super::ExecutionContext;

struct CountedPython {
    calls: Rc<Cell<usize>>,
    inner: TreeSitterProvider,
}

impl StructureProvider for CountedPython {
    fn parse_source<'source>(
        &self,
        path: &'source Path,
        source: &'source [u8],
    ) -> Result<ParsedSource<'source>, IdenteditError> {
        self.calls.set(self.calls.get() + 1);
        self.inner.parse_source(path, source)
    }

    fn supports_syntax_validation(&self) -> bool {
        true
    }

    fn can_handle(&self, path: &Path) -> bool {
        self.inner.can_handle(path)
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        self.inner.supported_extensions()
    }
}

fn counted_context() -> (ExecutionContext, Rc<Cell<usize>>) {
    let calls = Rc::new(Cell::new(0));
    let registry = ProviderRegistry::from_providers(vec![Box::new(CountedPython {
        calls: Rc::clone(&calls),
        inner: TreeSitterProvider::python(),
    })]);
    (ExecutionContext::with_registry(registry), calls)
}

#[test]
fn identical_observations_parse_once_without_hiding_owned_metadata() {
    let (context, calls) = counted_context();
    let file = Path::new("fixture.py");
    let source = b"def target():\n    return 1\n";
    let second_source = source.to_vec();

    let first = context.observe_source(file, source).unwrap();
    let second = context.observe_source(file, &second_source).unwrap();

    assert_eq!(calls.get(), 1);
    assert_eq!(first.handles(), second.handles());
    assert!(second.handles().iter().all(|handle| handle.file == file));
}

#[test]
fn changed_bytes_replace_the_observation_without_mutating_the_old_one() {
    let (context, calls) = counted_context();
    let file = Path::new("fixture.py");
    let first = context
        .observe_source(file, b"def target():\n    return 1\n")
        .unwrap();
    let second = context
        .observe_source(file, b"def target():\n    return 2\n")
        .unwrap();

    assert_eq!(calls.get(), 2);
    let function = |observation: &super::SourceObservation| {
        observation
            .handles()
            .iter()
            .find(|handle| handle.kind == "function_definition")
            .unwrap()
            .clone()
    };
    assert!(function(&first).text.ends_with("return 1"));
    assert!(function(&second).text.ends_with("return 2"));
    assert_ne!(
        function(&first).expected_old_hash,
        function(&second).expected_old_hash
    );
}

#[test]
fn raw_path_spelling_and_raw_line_endings_are_part_of_the_key() {
    let (context, calls) = counted_context();
    let first_file = Path::new("fixture.py");
    let alias = Path::new("./fixture.py");
    let source = b"def target():\n    return 1\n";
    context.observe_source(first_file, source).unwrap();

    let second = context.observe_source(alias, source).unwrap();
    assert_eq!(calls.get(), 2);
    assert!(
        second
            .handles()
            .iter()
            .all(|handle| handle.file.as_os_str() == alias.as_os_str())
    );

    let cr_source = b"def target():\r    return 1\r";
    let third = context.observe_source(alias, cr_source).unwrap();
    assert_eq!(calls.get(), 3);
    assert!(
        third
            .handles()
            .iter()
            .find(|handle| handle.kind == "function_definition")
            .unwrap()
            .text
            .contains('\r')
    );
}

#[test]
fn observations_do_not_cross_provider_contexts_or_survive_explicit_release() {
    let (first, first_calls) = counted_context();
    let (second, second_calls) = counted_context();
    let file = Path::new("fixture.py");
    let source = b"def target():\n    return 1\n";
    first.observe_source(file, source).unwrap();
    second.observe_source(file, source).unwrap();
    first.observe_source(file, source).unwrap();

    assert_eq!(first_calls.get(), 1);
    assert_eq!(second_calls.get(), 1);
    first.clear_source_observation();
    first.observe_source(file, source).unwrap();
    assert_eq!(first_calls.get(), 2);
}

#[test]
fn failed_observations_are_not_cached_or_replaced_with_empty_success() {
    let (context, calls) = counted_context();
    let file = Path::new("fixture.py");
    for _ in 0..2 {
        assert!(matches!(
            context.observe_source(file, b"def broken(:"),
            Err(IdenteditError::ParseFailure { .. })
        ));
    }
    assert_eq!(calls.get(), 2);

    let accepted = context
        .observe_source(file, b"def target():\n    return 1\n")
        .unwrap();
    assert!(!accepted.handles().is_empty());
    context
        .observe_source(file, b"def target():\n    return 1\n")
        .unwrap();
    assert_eq!(calls.get(), 3);
}
