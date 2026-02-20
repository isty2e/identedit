use std::path::PathBuf;

use clap::Args;

use crate::changeset::MultiFileChangeset;
use crate::error::IdenteditError;

#[derive(Debug, Args)]
pub struct MergeArgs {
    #[arg(
        value_name = "PLAN",
        required = true,
        num_args = 1..,
        help = "Input edit-plan JSON files (at least one)"
    )]
    pub inputs: Vec<PathBuf>,
}

pub fn run_merge(args: MergeArgs) -> Result<MultiFileChangeset, IdenteditError> {
    crate::cli::merge_plan::run_merge_inputs(args.inputs)
}
