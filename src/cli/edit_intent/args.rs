use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Default, Args)]
pub(crate) struct EditIntentArgs {
    #[arg(
        long,
        value_name = "TARGET",
        help = "Unified target selector: node identity (hex16), line anchor (line:hex12), or file-start/file-end"
    )]
    pub(crate) at: Option<String>,
    #[arg(
        long,
        value_name = "LINE:HASH",
        help = "Optional end line anchor for --replace-range (line flag mode)"
    )]
    pub(crate) end_anchor: Option<String>,
    #[arg(
        long = "config-path",
        value_name = "PATH",
        help = "Config path target for JSON/YAML/TOML files (dot/bracket syntax)"
    )]
    pub(crate) config_path: Option<String>,
    #[arg(
        long = "document-index",
        value_name = "INDEX",
        help = "0-based YAML document index for config path targets in multi-document YAML streams"
    )]
    pub(crate) document_index: Option<usize>,
    #[arg(
        long,
        value_name = "KIND",
        help = "Node kind for direct symbol targeting (requires --name)"
    )]
    pub(crate) kind: Option<String>,
    #[arg(
        long,
        value_name = "GLOB",
        help = "Symbol name glob for direct symbol targeting (requires --kind)"
    )]
    pub(crate) name: Option<String>,
    #[arg(
        long,
        value_name = "SYMBOL",
        help = "Target a unique named symbol; supports local names and containing-name paths like Class.method"
    )]
    pub(crate) symbol: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace target node with text (node mode)"
    )]
    pub(crate) replace: Option<Option<String>>,
    #[arg(
        long = "text-file",
        value_name = "PATH",
        help = "Read text payload from file for the selected text-taking operation"
    )]
    pub(crate) text_file: Option<PathBuf>,
    #[arg(
        long = "stdin-text",
        help = "Read text payload from stdin for the selected text-taking operation"
    )]
    pub(crate) stdin_text: bool,
    #[arg(
        long = "set-value",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Set config path value text (config path mode)"
    )]
    pub(crate) set_value: Option<Option<String>>,
    #[arg(
        long = "append-value",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Append value text to target array at config path (config path mode)"
    )]
    pub(crate) append_value: Option<Option<String>>,
    #[arg(
        long = "create-missing",
        help = "Allow config path set to create missing map/table keys (not array indexes)"
    )]
    pub(crate) create_missing: bool,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text for file-start/file-end targets"
    )]
    pub(crate) insert: Option<Option<String>>,
    #[arg(
        long = "scoped-regex",
        value_name = "PATTERN",
        help = "Regex pattern applied only inside the resolved node target"
    )]
    pub(crate) scoped_regex: Option<String>,
    #[arg(
        long = "scoped-replacement",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replacement text used with --scoped-regex"
    )]
    pub(crate) scoped_replacement: Option<Option<String>>,
    #[arg(long, help = "Delete target node or config path")]
    pub(crate) delete: bool,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text immediately before target node"
    )]
    pub(crate) insert_before: Option<Option<String>>,
    #[arg(
        long,
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text immediately after target node"
    )]
    pub(crate) insert_after: Option<Option<String>>,
    #[arg(
        long = "set-line",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace the anchored line with text"
    )]
    pub(crate) set_line: Option<Option<String>>,
    #[arg(
        long = "replace-range",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Replace anchored line range with text"
    )]
    pub(crate) replace_range: Option<Option<String>>,
    #[arg(
        long = "insert-after-line",
        value_name = "TEXT",
        num_args = 0..=1,
        help = "Insert text after anchored line"
    )]
    pub(crate) insert_after_line: Option<Option<String>>,
    #[arg(value_name = "FILE", help = "Target file path in flag mode")]
    pub(crate) file: Option<PathBuf>,
}

impl EditIntentArgs {
    pub(crate) fn is_empty(&self) -> bool {
        self.at.is_none()
            && self.end_anchor.is_none()
            && self.config_path.is_none()
            && self.document_index.is_none()
            && self.kind.is_none()
            && self.name.is_none()
            && self.symbol.is_none()
            && self.replace.is_none()
            && self.text_file.is_none()
            && !self.stdin_text
            && self.set_value.is_none()
            && self.append_value.is_none()
            && !self.create_missing
            && self.insert.is_none()
            && self.scoped_regex.is_none()
            && self.scoped_replacement.is_none()
            && !self.delete
            && self.insert_before.is_none()
            && self.insert_after.is_none()
            && self.set_line.is_none()
            && self.replace_range.is_none()
            && self.insert_after_line.is_none()
            && self.file.is_none()
    }
}
