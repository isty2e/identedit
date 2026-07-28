use std::path::PathBuf;

use tempfile::NamedTempFile;

use super::args::EditIntentArgs;
use super::model::{PreparedEditIntent, PreparedNodeEditOperation};
use super::parse::parse_flag_edit_intent;
use super::target::NodeTargetSelector;
use crate::changeset::{OpKind, TransformTarget};
use crate::hash::hash_bytes;
use crate::hashline::HashlineEdit;

fn base_args(file: PathBuf) -> EditIntentArgs {
    EditIntentArgs {
        file: Some(file),
        ..EditIntentArgs::default()
    }
}

#[test]
fn parse_flag_edit_intent_builds_node_intent_for_direct_symbol_targeting() {
    let mut args = base_args(PathBuf::from("fixture.py"));
    args.kind = Some("function_definition".to_string());
    args.name = Some("process_*".to_string());
    args.replace = Some(Some("def process_data():\n    return 1\n".to_string()));

    let intent = parse_flag_edit_intent(&args).expect("node intent should parse");
    let PreparedEditIntent::Node(intent) = intent else {
        panic!("expected node intent");
    };

    assert_eq!(
        intent.selector,
        NodeTargetSelector::Selector {
            kind: "function_definition".to_string(),
            name_pattern: "process_*".to_string(),
        }
    );
    assert_eq!(
        intent.operation,
        PreparedNodeEditOperation::Standard(OpKind::Replace {
            new_text: "def process_data():\n    return 1\n".to_string(),
        })
    );
}

#[test]
fn parse_flag_edit_intent_builds_node_intent_for_symbol_selector() {
    let mut args = base_args(PathBuf::from("fixture.py"));
    args.symbol = Some("Processor.process_data".to_string());
    args.replace = Some(Some("def process_data():\n    return 1\n".to_string()));

    let intent = parse_flag_edit_intent(&args).expect("symbol intent should parse");
    let PreparedEditIntent::Node(intent) = intent else {
        panic!("expected node intent");
    };

    assert_eq!(
        intent.selector,
        NodeTargetSelector::Symbol("Processor.process_data".to_string())
    );
    assert_eq!(
        intent.operation,
        PreparedNodeEditOperation::Standard(OpKind::Replace {
            new_text: "def process_data():\n    return 1\n".to_string(),
        })
    );
}

#[test]
fn parse_flag_edit_intent_builds_line_intent() {
    let mut args = base_args(PathBuf::from("fixture.py"));
    args.at = Some("12:0123456789ab".to_string());
    args.set_line = Some(Some("replacement".to_string()));

    let intent = parse_flag_edit_intent(&args).expect("line intent should parse");
    let PreparedEditIntent::Line(intent) = intent else {
        panic!("expected line intent");
    };

    match intent.edit {
        HashlineEdit::SetLine { set_line } => {
            assert_eq!(set_line.anchor.to_string(), "12:0123456789ab");
            assert_eq!(set_line.new_text, "replacement");
        }
        other => panic!("expected set-line edit, got {other:?}"),
    }
}

#[test]
fn parse_flag_edit_intent_builds_canonical_intent_for_file_insert() {
    let temp = NamedTempFile::new().expect("temp file");
    std::fs::write(temp.path(), "body\n").expect("write source");

    let mut args = base_args(temp.path().to_path_buf());
    args.at = Some("file-end".to_string());
    args.insert = Some(Some("\n# tail\n".to_string()));

    let intent = parse_flag_edit_intent(&args).expect("file intent should parse");
    let PreparedEditIntent::Canonical(intent) = intent else {
        panic!("expected canonical intent");
    };

    assert_eq!(intent.file, temp.path());
    assert_eq!(
        intent.operation.op(),
        &OpKind::Insert {
            new_text: "\n# tail\n".to_string(),
        }
    );
    match intent.operation.target() {
        TransformTarget::FileEnd { expected_file_hash } => {
            assert_eq!(expected_file_hash, &hash_bytes(b"body\n"));
        }
        other => panic!("expected file_end target, got {other:?}"),
    }
}
