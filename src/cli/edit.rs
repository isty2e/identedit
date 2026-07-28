use clap::Args;

use crate::changeset::MultiFileChangeset;
use crate::cli::edit_intent::{EditIntentArgs, PreparedEditIntent, parse_flag_edit_intent};
use crate::error::IdenteditError;

#[derive(Debug, Args)]
pub struct EditArgs {
    #[arg(long, help = "Read edit request JSON from stdin")]
    pub json: bool,
    #[command(flatten)]
    pub(crate) intent: EditIntentArgs,
    #[arg(
        long,
        help = "Emit verbose preview fields (old_text) instead of compact fields"
    )]
    pub verbose: bool,
}

pub fn run_edit(args: EditArgs) -> Result<MultiFileChangeset, IdenteditError> {
    if args.json {
        if !args.intent.is_empty() {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "edit --json cannot be combined with flag-mode target, operation, text-source, or FILE arguments."
                        .to_string(),
            });
        }
        return crate::cli::edit_build::run_edit_json_mode(args.verbose);
    }

    let intent = parse_flag_edit_intent(&args.intent)?;
    match intent {
        PreparedEditIntent::Node(intent) => {
            let resolved = intent.resolve()?;
            crate::cli::edit_build::build_single_file_edit_plan(
                resolved.file,
                resolved.operation,
                args.verbose,
            )
        }
        PreparedEditIntent::Canonical(intent) => {
            crate::cli::edit_build::build_single_file_edit_plan(
                intent.file,
                intent.operation,
                args.verbose,
            )
        }
        PreparedEditIntent::Line(intent) => {
            let intent = intent.into_canonical()?;
            crate::cli::edit_build::build_single_file_edit_plan(
                intent.file,
                intent.operation,
                args.verbose,
            )
        }
    }
}
