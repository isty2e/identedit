use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use tempfile::tempdir;

use crate::cli::edit_intent::{
    EditIntentArgs, NodeEditIntent, PreparedEditIntent, parse_flag_edit_intent,
};
use crate::error::IdenteditError;
use crate::execution_context::ExecutionContext;
use crate::handle::SelectionHandle;
use crate::provider::{ParsedSource, ProviderRegistry, StructureProvider, TreeSitterProvider};

use super::{
    ApplyBackedExecution, ColorMode, PatchOutputMode, execute_node_flag_patch_request,
    run_patch_edit_operation,
};

const SOURCE: &str = "def target():\n    return 1\n\ndef other():\n    return 3\n";

#[derive(Default)]
struct Counts {
    parses: Cell<usize>,
    projections: Cell<usize>,
}

struct CountedPython {
    counts: Rc<Counts>,
    inner: TreeSitterProvider,
}

impl StructureProvider for CountedPython {
    fn parse(&self, path: &Path, source: &[u8]) -> Result<Vec<SelectionHandle>, IdenteditError> {
        self.counts
            .projections
            .set(self.counts.projections.get() + 1);
        self.parse_source(path, source)
            .map(ParsedSource::into_handles)
    }

    fn parse_source<'source>(
        &self,
        path: &'source Path,
        source: &'source [u8],
    ) -> Result<ParsedSource<'source>, IdenteditError> {
        self.counts.parses.set(self.counts.parses.get() + 1);
        self.inner.parse_source(path, source)
    }

    fn supports_syntax_validation(&self) -> bool {
        true
    }
    fn can_handle(&self, path: &Path) -> bool {
        self.inner.can_handle(path) || path.extension().is_some_and(|extension| extension == "rs")
    }
    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn supported_extensions(&self) -> &'static [&'static str] {
        &["py", "rs"]
    }
}

fn context() -> (ExecutionContext, Rc<Counts>) {
    let counts = Rc::new(Counts::default());
    let registry = ProviderRegistry::from_providers(vec![Box::new(CountedPython {
        counts: Rc::clone(&counts),
        inner: TreeSitterProvider::python(),
    })]);
    (ExecutionContext::with_registry(registry), counts)
}

fn intent(file: &Path, mut args: EditIntentArgs, scoped_regex: bool) -> NodeEditIntent {
    args.file = Some(file.to_path_buf());
    if scoped_regex {
        args.scoped_regex = Some("return 1".to_string());
        args.scoped_replacement = Some(Some("return 2".to_string()));
    } else {
        args.replace = Some(Some("def target():\n    return 2".to_string()));
    }
    let PreparedEditIntent::Node(intent) = parse_flag_edit_intent(&args).unwrap() else {
        panic!("expected node intent");
    };
    intent
}

fn symbol_args() -> EditIntentArgs {
    EditIntentArgs {
        symbol: Some("target".to_string()),
        ..Default::default()
    }
}

#[test]
fn node_patch_routes_share_source_work_but_always_validate_the_result() {
    let root = tempdir().unwrap();
    // A fresh default registry would choose Rust, not the injected Python provider.
    let file = root.path().join("fixture.rs");
    std::fs::write(&file, SOURCE).unwrap();
    let handles = TreeSitterProvider::python()
        .parse(&file, SOURCE.as_bytes())
        .unwrap();
    let identity = handles
        .iter()
        .find(|handle| handle.name.as_deref() == Some("target"))
        .unwrap()
        .identity
        .clone();

    for scoped_regex in [false, true] {
        for execution in [
            ApplyBackedExecution::json(true, false),
            ApplyBackedExecution::json(false, false),
            ApplyBackedExecution {
                dry_run: true,
                verbose: false,
                output: PatchOutputMode::Diff {
                    color: ColorMode::Never,
                },
            },
        ] {
            for args in [
                symbol_args(),
                EditIntentArgs {
                    kind: Some("function_definition".to_string()),
                    name: Some("target".to_string()),
                    ..Default::default()
                },
                EditIntentArgs {
                    at: Some(identity.clone()),
                    ..Default::default()
                },
            ] {
                std::fs::write(&file, SOURCE).unwrap();
                let (context, counts) = context();
                execute_node_flag_patch_request(
                    intent(&file, args, scoped_regex),
                    execution,
                    &context,
                )
                .unwrap();

                assert_eq!(
                    counts.projections.get(),
                    1,
                    "original handles are projected once per unchanged request"
                );
                assert_eq!(
                    counts.parses.get(),
                    2,
                    "the proposed result still has its own parse"
                );
                let expected = if execution.dry_run {
                    SOURCE.to_string()
                } else {
                    SOURCE.replace("return 1", "return 2")
                };
                assert_eq!(std::fs::read_to_string(&file).unwrap(), expected);
            }
        }
    }
}

#[test]
fn target_to_build_changes_reparse_instead_of_applying_a_stale_observation() {
    let root = tempdir().unwrap();
    let file = root.path().join("fixture.py");
    for selected_text_changed in [false, true] {
        std::fs::write(&file, SOURCE).unwrap();
        let (context, counts) = context();
        let resolved = intent(&file, symbol_args(), false)
            .resolve_with_context(&context)
            .unwrap();
        let changed = if selected_text_changed {
            SOURCE.replace("return 1", "return 8")
        } else {
            SOURCE.replace("return 3", "return 4")
        };
        std::fs::write(&file, &changed).unwrap();

        let result = run_patch_edit_operation(
            resolved.file,
            resolved.operation,
            ApplyBackedExecution::json(false, false),
            None,
            &context,
        );

        assert_eq!(
            counts.projections.get(),
            2,
            "different bytes require fresh handles"
        );
        if selected_text_changed {
            assert!(matches!(
                result,
                Err(IdenteditError::PreconditionFailed { .. })
            ));
            assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);
        } else {
            result.unwrap();
            assert_eq!(
                std::fs::read_to_string(&file).unwrap(),
                changed.replace("return 1", "return 2")
            );
        }
    }
}

#[test]
fn target_to_build_invalid_source_does_not_hide_behind_a_successful_observation() {
    let root = tempdir().unwrap();
    let file = root.path().join("fixture.py");
    for changed in [b"def broken(:".as_slice(), &[0xff]] {
        std::fs::write(&file, SOURCE).unwrap();
        let (context, _) = context();
        let resolved = intent(&file, symbol_args(), false)
            .resolve_with_context(&context)
            .unwrap();
        std::fs::write(&file, changed).unwrap();

        let result = run_patch_edit_operation(
            resolved.file,
            resolved.operation,
            ApplyBackedExecution::json(false, false),
            None,
            &context,
        );

        if changed == [0xff] {
            assert!(matches!(result, Err(IdenteditError::Io { .. })));
        } else {
            assert!(matches!(result, Err(IdenteditError::ParseFailure { .. })));
        }
        assert_eq!(std::fs::read(&file).unwrap(), changed);
    }
}
