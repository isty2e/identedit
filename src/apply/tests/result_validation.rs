use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use tempfile::tempdir;

use crate::changeset::{EditOperation, OpKind, TransformTarget};
use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::handle::SelectionHandle;
use crate::provider::{ParsedSource, ProviderRegistry, StructureProvider, TreeSitterProvider};
use crate::transform::build::{build_changeset, build_changeset_with_context};

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
            .map(ParsedSource::into_handles)
    }

    fn parse_source<'source>(
        &self,
        path: &'source Path,
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

fn context(
    calls: &Rc<Cell<usize>>,
    validates_syntax: bool,
    fail_result_setup: bool,
) -> ExecutionContext {
    ExecutionContext::with_registry(ProviderRegistry::from_providers(vec![Box::new(
        ObservedProvider {
            inner: TreeSitterProvider::python(),
            calls: Rc::clone(calls),
            handle_projections: Rc::new(Cell::new(0)),
            validates_syntax,
            fail_result_setup,
        },
    )]))
}

fn prepared_target_replacement(
    file: &Path,
    source: &str,
    context: &ExecutionContext,
) -> crate::changeset::FileChange {
    let handles = TreeSitterProvider::python()
        .parse(file, source.as_bytes())
        .unwrap();
    let target = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("target"))
        .unwrap();
    let operation = EditOperation::try_new(
        TransformTarget::node(
            target.identity.clone(),
            target.kind.clone(),
            Some(target.span),
            target.expected_old_hash.clone(),
        ),
        OpKind::Replace {
            new_text: "def target():\n    return 2".to_string(),
        },
    )
    .unwrap();
    build_changeset_with_context(file, vec![operation], context).unwrap()
}

#[test]
fn cached_build_source_cannot_bypass_the_apply_lock() {
    let root = tempdir().unwrap();
    let file = root.path().join("fixture.py");
    let source = "def target():\n    return 1\n";
    std::fs::write(&file, source).unwrap();
    let calls = Rc::new(Cell::new(0));
    let context = context(&calls, true, false);
    let changeset = prepared_target_replacement(&file, source, &context);
    let lock = super::super::acquire_apply_lock(&file).unwrap();

    let error = preflight_changesets_in_order(&[changeset], &context).unwrap_err();

    assert!(matches!(error, IdenteditError::ResourceBusy { .. }));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
    drop(lock);
}

#[test]
fn build_to_locked_preflight_changes_are_reparsed_without_a_whole_file_stale_rule() {
    let root = tempdir().unwrap();
    let file = root.path().join("fixture.py");
    let source = "def target():\n    return 1\n\ndef other():\n    return 3\n";
    for selected_text_changed in [false, true] {
        std::fs::write(&file, source).unwrap();
        let calls = Rc::new(Cell::new(0));
        let context = context(&calls, true, false);
        let changeset = prepared_target_replacement(&file, source, &context);
        let changed = if selected_text_changed {
            source.replace("return 1", "return 8")
        } else {
            source.replace("return 3", "return 4")
        };
        std::fs::write(&file, &changed).unwrap();

        let result = preflight_changesets_in_order(&[changeset], &context);

        if selected_text_changed {
            assert!(matches!(
                result,
                Err(IdenteditError::PreconditionFailed { .. })
            ));
            assert_eq!(calls.get(), 2);
        } else {
            let plans = result.unwrap();
            assert_eq!(
                plans[0].updated_text,
                changed.replace("return 1", "return 2")
            );
            assert_eq!(
                calls.get(),
                3,
                "changed source and proposed result are both parsed"
            );
            drop(plans);
        }
        assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);
    }
}

#[test]
fn cached_build_source_does_not_hide_current_read_or_source_parse_errors() {
    let root = tempdir().unwrap();
    let file = root.path().join("fixture.py");
    let source = "def target():\n    return 1\n";
    for changed in [b"def broken(:".as_slice(), &[0xff]] {
        std::fs::write(&file, source).unwrap();
        let calls = Rc::new(Cell::new(0));
        let context = context(&calls, true, false);
        let changeset = prepared_target_replacement(&file, source, &context);
        std::fs::write(&file, changed).unwrap();

        let error = preflight_changesets_in_order(&[changeset], &context).unwrap_err();

        if changed == [0xff] {
            assert!(matches!(error, IdenteditError::Io { .. }));
        } else {
            assert!(matches!(error, IdenteditError::ParseFailure { .. }));
        }
        assert_eq!(std::fs::read(&file).unwrap(), changed);
    }
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
    let context =
        ExecutionContext::with_registry(ProviderRegistry::from_providers(vec![Box::new(
            ObservedProvider {
                inner: TreeSitterProvider::python(),
                calls: Rc::clone(&calls),
                handle_projections: Rc::clone(&handle_projections),
                validates_syntax: true,
                fail_result_setup: false,
            },
        )]));

    let plans = preflight_changesets_in_order(&[changeset], &context).unwrap();

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
    let plans = preflight_changesets_in_order(&[changeset], &context(&calls, true, false)).unwrap();
    assert_eq!(calls.get(), 2);
    drop(plans);

    let calls = Rc::new(Cell::new(0));
    let empty = crate::changeset::FileChange {
        file,
        operations: vec![],
    };
    let plans = preflight_changesets_in_order(&[empty], &context(&calls, true, false)).unwrap();
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
    let plans = preflight_changesets_in_order(&[changeset], &context(&calls, false, true)).unwrap();
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
        preflight_changesets_in_order(&[changeset], &context(&calls, true, true)).unwrap_err();
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
