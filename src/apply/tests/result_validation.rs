use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use tempfile::tempdir;

use crate::changeset::{EditOperation, OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::handle::SelectionHandle;
use crate::provider::{ParsedSource, ProviderRegistry, StructureProvider, TreeSitterProvider};
use crate::transform::build::build_changeset;

use super::super::preflight_changesets_in_order;

struct ObservedProvider {
    inner: TreeSitterProvider,
    calls: Rc<Cell<usize>>,
    handle_projections: Rc<Cell<usize>>,
    validates_syntax: bool,
    fail_result_setup: bool,
}

impl StructureProvider for ObservedProvider {
    fn parse(&self, path: &Path, source: &[u8]) -> Result<Vec<SelectionHandle>, IdenteditError> {
        self.handle_projections
            .set(self.handle_projections.get() + 1);
        self.parse_source(path, source)
            .map(|parsed| parsed.into_handles(path))
    }

    fn parse_source<'source>(
        &self,
        path: &Path,
        source: &'source [u8],
    ) -> Result<ParsedSource<'source>, IdenteditError> {
        self.calls.set(self.calls.get() + 1);
        if self.fail_result_setup && self.calls.get() > 1 {
            return Err(IdenteditError::LanguageSetup {
                message: "result parser setup failed".to_string(),
            });
        }
        self.inner.parse_source(path, source)
    }

    fn supports_syntax_validation(&self) -> bool {
        self.validates_syntax
    }

    fn can_handle(&self, path: &Path) -> bool {
        self.inner.can_handle(path)
    }

    fn name(&self) -> &'static str {
        "fallback"
    }

    fn supported_extensions(&self) -> &'static [&'static str] {
        self.inner.supported_extensions()
    }
}

fn registry(
    calls: &Rc<Cell<usize>>,
    validates_syntax: bool,
    fail_result_setup: bool,
) -> ProviderRegistry {
    ProviderRegistry::from_providers(vec![Box::new(ObservedProvider {
        inner: TreeSitterProvider::python(),
        calls: Rc::clone(calls),
        handle_projections: Rc::new(Cell::new(0)),
        validates_syntax,
        fail_result_setup,
    })])
}

#[test]
fn final_validation_does_not_project_selection_handles() {
    let root = tempdir().unwrap();
    let file = super::create_python_target(root.path());
    let before = std::fs::read(&file).unwrap();
    let handles = TreeSitterProvider::python().parse(&file, &before).unwrap();
    let target = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .unwrap();
    let changeset = crate::transform::build::build_replace_changeset(
        &file,
        &target.identity,
        target.text.clone(),
    )
    .unwrap();
    let calls = Rc::new(Cell::new(0));
    let handle_projections = Rc::new(Cell::new(0));
    let registry = ProviderRegistry::from_providers(vec![Box::new(ObservedProvider {
        inner: TreeSitterProvider::python(),
        calls: Rc::clone(&calls),
        handle_projections: Rc::clone(&handle_projections),
        validates_syntax: true,
        fail_result_setup: false,
    })]);

    let plans = preflight_changesets_in_order(&[changeset], &registry).unwrap();

    assert_eq!(calls.get(), 2, "source and result still need parsing");
    assert_eq!(
        handle_projections.get(),
        1,
        "only source handles are needed"
    );
    assert_eq!(std::fs::read(&file).unwrap(), before);
    drop(plans);
}

#[test]
fn node_file_is_parsed_once_before_and_once_after_all_edits_independent_of_provider_name() {
    let root = tempdir().unwrap();
    let file = super::create_python_target(root.path());
    let handles = TreeSitterProvider::python()
        .parse(&file, &std::fs::read(&file).unwrap())
        .unwrap();
    let operations = handles
        .iter()
        .filter(|handle| handle.kind == "function_definition")
        .map(|handle| {
            EditOperation::try_new(
                TransformTarget::node(
                    handle.identity.clone(),
                    handle.kind.clone(),
                    Some(handle.span),
                    handle.expected_old_hash.clone(),
                ),
                OpKind::Replace {
                    new_text: handle.text.clone(),
                },
            )
            .unwrap()
        })
        .collect();
    let changeset = build_changeset(&file, operations).unwrap();
    let calls = Rc::new(Cell::new(0));
    let plans =
        preflight_changesets_in_order(&[changeset], &registry(&calls, true, false)).unwrap();
    assert_eq!(calls.get(), 2);
    drop(plans);

    let calls = Rc::new(Cell::new(0));
    let empty = crate::changeset::FileChange {
        file,
        operations: vec![],
    };
    let plans = preflight_changesets_in_order(&[empty], &registry(&calls, true, false)).unwrap();
    assert_eq!(
        calls.get(),
        1,
        "empty operation lists do not enable result validation"
    );
    drop(plans);
}

#[test]
fn unsupported_result_validation_does_not_perform_a_second_parse() {
    let root = tempdir().unwrap();
    let file = super::create_python_target(root.path());
    let handles = TreeSitterProvider::python()
        .parse(&file, &std::fs::read(&file).unwrap())
        .unwrap();
    let target = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .unwrap();
    let operation = EditOperation::try_new(
        TransformTarget::node(
            target.identity.clone(),
            target.kind.clone(),
            Some(target.span),
            target.expected_old_hash.clone(),
        ),
        OpKind::Replace {
            new_text: "def invalid(".to_string(),
        },
    )
    .unwrap();
    let changeset = build_changeset(&file, vec![operation]).unwrap();
    let calls = Rc::new(Cell::new(0));
    let plans =
        preflight_changesets_in_order(&[changeset], &registry(&calls, false, true)).unwrap();
    assert_eq!(calls.get(), 1);
    assert!(plans[0].updated_text.contains("def invalid("));
}

#[test]
fn result_setup_failure_is_not_reclassified_as_syntax_or_fallback_success() {
    let root = tempdir().unwrap();
    let file = super::create_python_target(root.path());
    let before = std::fs::read(&file).unwrap();
    let handles = TreeSitterProvider::python().parse(&file, &before).unwrap();
    let target = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("process_data"))
        .unwrap();
    let operation = EditOperation::try_new(
        TransformTarget::node(
            target.identity.clone(),
            target.kind.clone(),
            Some(target.span),
            target.expected_old_hash.clone(),
        ),
        OpKind::Replace {
            new_text: target.text.clone(),
        },
    )
    .unwrap();
    let changeset = build_changeset(&file, vec![operation]).unwrap();
    let calls = Rc::new(Cell::new(0));
    let error =
        preflight_changesets_in_order(&[changeset], &registry(&calls, true, true)).unwrap_err();
    assert!(matches!(error, IdenteditError::LanguageSetup { .. }));
    assert_eq!(calls.get(), 2);
    assert_eq!(std::fs::read(&file).unwrap(), before);
}

#[test]
fn rejected_result_prevents_even_the_first_commit_hook_in_a_multi_file_transaction() {
    let root = tempdir().unwrap();
    let before = "function target() { return 1; }\n";
    let mut changesets = Vec::new();
    for (name, replacement) in [
        ("a.js", "function target() { return 2; }"),
        ("z.js", "function target( {"),
    ] {
        let file = root.path().join(name);
        std::fs::write(&file, before).unwrap();
        let handles = crate::transform::parse::parse_handles_for_file(&file).unwrap();
        let target = handles
            .iter()
            .find(|handle| handle.name.as_deref() == Some("target"))
            .unwrap();
        changesets.push(
            crate::transform::build::build_replace_changeset(
                &file,
                &target.identity,
                replacement.to_string(),
            )
            .unwrap(),
        );
    }

    let commit_hooks = Cell::new(0);
    let error = super::super::apply_changesets_with_hooks(
        &changesets,
        || {
            commit_hooks.set(commit_hooks.get() + 1);
            Ok(())
        },
        || {
            commit_hooks.set(commit_hooks.get() + 1);
            Ok(())
        },
    )
    .unwrap_err();
    assert!(matches!(error, IdenteditError::ResultParseFailure { .. }));
    assert_eq!(
        commit_hooks.get(),
        0,
        "validation must precede commit, not rely on rollback"
    );
    for changeset in changesets {
        assert_eq!(std::fs::read_to_string(changeset.file).unwrap(), before);
    }
}
