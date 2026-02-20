use std::path::PathBuf;

use clap::Args;

use crate::changeset::MultiFileChangeset;
use crate::error::IdenteditError;

#[derive(Debug, Args)]
pub struct EditArgs {
    #[arg(
        long,
        value_name = "IDENTITY",
        help = "Target identity from read output (flag mode only)"
    )]
    pub identity: Option<String>,
    #[arg(
        long,
        value_name = "TEXT",
        help = "Replacement text for the target (--identity mode)"
    )]
    pub replace: Option<String>,
    #[arg(long, help = "Delete the target node (--identity mode)")]
    pub delete: bool,
    #[arg(long, help = "Read edit request JSON from stdin")]
    pub json: bool,
    #[arg(
        long,
        help = "Emit verbose preview fields (old_text) instead of compact fields"
    )]
    pub verbose: bool,
    #[arg(
        value_name = "FILE",
        help = "Input file in flag mode; omit when using --json"
    )]
    pub file: Option<PathBuf>,
}

pub fn run_edit(args: EditArgs) -> Result<MultiFileChangeset, IdenteditError> {
    crate::cli::edit_build::run_edit_build(crate::cli::edit_build::EditBuildArgs {
        identity: args.identity,
        replace: args.replace,
        delete: args.delete,
        json: args.json,
        verbose: args.verbose,
        file: args.file,
    })
}
