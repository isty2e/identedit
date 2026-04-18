use std::path::PathBuf;

use clap::Args;
use serde_json::Value;

use crate::error::IdenteditError;

mod execute;
mod flag;
mod json;
mod target;
mod text;

#[derive(Debug, Args)]
pub struct PatchArgs {
    #[arg(long, help = "Read patch request JSON from stdin")]
    pub json: bool,
    #[arg(
        long,
        value_name = "TARGET",
        help = "Unified target selector: node identity (hex16), line anchor (line:hex12), or file-start/file-end"
    )]
    pub at: Option<String>,
    #[arg(
        long,
        value_name = "IDENTITY",
        hide = true,
        help = "Legacy target identity flag (use --at)"
    )]
    pub identity: Option<String>,
    #[arg(
        long,
        value_name = "LINE:HASH",
        hide = true,
        help = "Legacy line anchor flag (use --at)"
    )]
    pub anchor: Option<String>,
    #[arg(
        long,
        value_name = "LINE:HASH",
        help = "Optional end line anchor for --replace-range (line flag mode)"
    )]
    pub end_anchor: Option<String>,
    #[arg(
        long = "config-path",
        value_name = "PATH",
        help = "Config path target for JSON/YAML/TOML files (dot/bracket syntax)"
    )]
    pub config_path: Option<String>,
    #[arg(
        long,
        value_name = "KIND",
        help = "Node kind for direct symbol-targeted patching (requires --name, node flag mode)"
    )]
    pub kind: Option<String>,
    #[arg(
        long,
        value_name = "GLOB",
        help = "Symbol name glob for direct symbol-targeted patching (requires --kind, node flag mode)"
    )]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace target node with text (node flag mode)"
    )]
    pub replace: Option<Option<String>>,
    #[arg(
        long = "text-file",
        value_name = "PATH",
        help = "Read text payload from file for the selected text-taking patch operation"
    )]
    pub text_file: Option<PathBuf>,
    #[arg(
        long = "stdin-text",
        help = "Read text payload from stdin for the selected text-taking patch operation"
    )]
    pub stdin_text: bool,
    #[arg(
        long = "set-value",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Set config path value text (config path flag mode)"
    )]
    pub set_value: Option<Option<String>>,
    #[arg(
        long = "append-value",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Append value text to target array at config path (config path flag mode)"
    )]
    pub append_value: Option<Option<String>>,
    #[arg(
        long = "create-missing",
        help = "Allow config path set to create missing map/table keys (not array indexes)"
    )]
    pub create_missing: bool,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text for file-start/file-end targets"
    )]
    pub insert: Option<Option<String>>,
    #[arg(
        long = "scoped-regex",
        value_name = "PATTERN",
        help = "Regex pattern applied only inside the resolved node target (node flag mode)"
    )]
    pub scoped_regex: Option<String>,
    #[arg(
        long = "scoped-replacement",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replacement text used with --scoped-regex (node flag mode)"
    )]
    pub scoped_replacement: Option<Option<String>>,
    #[arg(long, help = "Delete target node (node flag mode)")]
    pub delete: bool,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text immediately before target node (node flag mode)"
    )]
    pub insert_before: Option<Option<String>>,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text immediately after target node (node flag mode)"
    )]
    pub insert_after: Option<Option<String>>,
    #[arg(
        long = "set-line",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace the anchored line with text (line flag mode)"
    )]
    pub set_line: Option<Option<String>>,
    #[arg(
        long = "replace-range",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace anchored line range with text (line flag mode)"
    )]
    pub replace_range: Option<Option<String>>,
    #[arg(
        long = "insert-after-line",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text after anchored line (line flag mode)"
    )]
    pub insert_after_line: Option<Option<String>>,
    #[arg(
        long,
        help = "If line-mode strict check fails with deterministic remap candidates, run one repair retry"
    )]
    pub auto_repair: bool,
    #[arg(long, help = "Validate and preview without writing files")]
    pub dry_run: bool,
    #[arg(long, help = "Include per-file apply results in output (flag mode)")]
    pub verbose: bool,
    #[arg(value_name = "FILE", help = "Target file path in flag mode")]
    pub file: Option<PathBuf>,
}

pub fn run_patch(args: PatchArgs) -> Result<Value, IdenteditError> {
    if args.json {
        if args.text_file.is_some() || args.stdin_text {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "--text-file and --stdin-text are only supported in flag mode; JSON patch mode already reads the request body from stdin."
                        .to_string(),
            });
        }
        return json::run_patch_json_mode(args.dry_run);
    }

    let request = flag::parse_flag_patch_request(&args)?;
    execute::execute_flag_patch_request(request)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::NamedTempFile;

    use super::PatchArgs;
    use super::execute::{
        ApplyBackedExecution, FlagPatchRequest, LineExecution, NodeFlagPatchRequest,
        PreparedNodePatchOperation,
    };
    use super::flag::parse_flag_patch_request;
    use super::target::NodeTargetSelector;
    use crate::changeset::{OpKind, TransformTarget};
    use crate::hash::hash_bytes;
    use crate::hashline::HashlineEdit;

    fn base_args(file: PathBuf) -> PatchArgs {
        PatchArgs {
            json: false,
            at: None,
            identity: None,
            anchor: None,
            end_anchor: None,
            config_path: None,
            kind: None,
            name: None,
            replace: None,
            text_file: None,
            stdin_text: false,
            set_value: None,
            append_value: None,
            create_missing: false,
            insert: None,
            scoped_regex: None,
            scoped_replacement: None,
            delete: false,
            insert_before: None,
            insert_after: None,
            set_line: None,
            replace_range: None,
            insert_after_line: None,
            auto_repair: false,
            dry_run: false,
            verbose: false,
            file: Some(file),
        }
    }

    #[test]
    fn parse_flag_patch_request_builds_node_request_for_direct_symbol_targeting() {
        let mut args = base_args(PathBuf::from("fixture.py"));
        args.kind = Some("function_definition".to_string());
        args.name = Some("process_*".to_string());
        args.replace = Some(Some("def process_data():\n    return 1\n".to_string()));
        args.dry_run = true;
        args.verbose = true;

        let request = parse_flag_patch_request(&args).expect("node request should parse");
        let FlagPatchRequest::Node(NodeFlagPatchRequest {
            selector,
            operation,
            execution,
            ..
        }) = request
        else {
            panic!("expected node request");
        };

        assert_eq!(
            selector,
            NodeTargetSelector::Selector {
                kind: "function_definition".to_string(),
                name_pattern: "process_*".to_string(),
            }
        );
        assert_eq!(
            operation,
            PreparedNodePatchOperation::Standard(OpKind::Replace {
                new_text: "def process_data():\n    return 1\n".to_string(),
            })
        );
        assert_eq!(
            execution,
            ApplyBackedExecution {
                dry_run: true,
                verbose: true,
            }
        );
    }

    #[test]
    fn parse_flag_patch_request_builds_line_request_with_line_execution_options() {
        let mut args = base_args(PathBuf::from("fixture.py"));
        args.at = Some("12:0123456789ab".to_string());
        args.set_line = Some(Some("replacement".to_string()));
        args.auto_repair = true;
        args.dry_run = true;

        let request = parse_flag_patch_request(&args).expect("line request should parse");
        let FlagPatchRequest::Line(line_request) = request else {
            panic!("expected line request");
        };

        match line_request.edit {
            HashlineEdit::SetLine { set_line } => {
                assert_eq!(set_line.anchor, "12:0123456789ab");
                assert_eq!(set_line.new_text, "replacement");
            }
            other => panic!("expected set-line edit, got {other:?}"),
        }
        assert_eq!(
            line_request.execution,
            LineExecution {
                dry_run: true,
                auto_repair: true,
            }
        );
    }

    #[test]
    fn parse_flag_patch_request_builds_canonical_request_for_file_insert() {
        let temp = NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), "body\n").expect("write source");

        let mut args = base_args(temp.path().to_path_buf());
        args.at = Some("file-end".to_string());
        args.insert = Some(Some("\n# tail\n".to_string()));
        args.verbose = true;

        let request = parse_flag_patch_request(&args).expect("file request should parse");
        let FlagPatchRequest::Canonical(request) = request else {
            panic!("expected canonical request");
        };

        assert_eq!(request.file, temp.path());
        assert_eq!(
            request.op,
            OpKind::Insert {
                new_text: "\n# tail\n".to_string(),
            }
        );
        assert_eq!(
            request.execution,
            ApplyBackedExecution {
                dry_run: false,
                verbose: true,
            }
        );
        match request.target {
            TransformTarget::FileEnd { expected_file_hash } => {
                assert_eq!(expected_file_hash, hash_bytes(b"body\n"));
            }
            other => panic!("expected file-end target, got {other:?}"),
        }
    }
}
