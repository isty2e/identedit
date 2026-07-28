use clap::{Args, ValueEnum};
use serde_json::Value;

use crate::cli::edit_intent::{EditIntentArgs, parse_flag_edit_intent};
use crate::error::IdenteditError;

mod diff;
mod execute;
mod json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Args)]
pub struct PatchArgs {
    #[arg(long, help = "Read patch request JSON from stdin")]
    pub json: bool,
    #[command(flatten)]
    pub(crate) intent: EditIntentArgs,
    #[arg(
        long,
        help = "If line-mode strict check fails with deterministic remap candidates, run one repair retry"
    )]
    pub auto_repair: bool,
    #[arg(long, help = "Validate and preview without writing files")]
    pub dry_run: bool,
    #[arg(long, help = "Emit dry-run preview as unified diff instead of JSON")]
    pub diff: bool,
    #[arg(
        long,
        value_enum,
        value_name = "WHEN",
        help = "Colorize --diff output (auto|always|never)"
    )]
    pub color: Option<ColorMode>,
    #[arg(long, help = "Include per-file apply results in output (flag mode)")]
    pub verbose: bool,
}

pub(super) enum PatchCommandOutput {
    Json(Value),
    Text(String),
}

pub(super) fn run_patch(args: PatchArgs) -> Result<PatchCommandOutput, IdenteditError> {
    validate_output_options(&args)?;

    if args.json {
        if !args.intent.is_empty() {
            return Err(IdenteditError::InvalidRequest {
                message:
                    "patch --json cannot be combined with flag-mode target, operation, text-source, or FILE arguments."
                        .to_string(),
            });
        }
        return json::run_patch_json_mode(args.dry_run).map(PatchCommandOutput::Json);
    }

    let intent = parse_flag_edit_intent(&args.intent)?;
    let execution = execute::PatchExecution::from_args(&args);
    execute::execute_flag_patch_request(intent, execution)
}

fn validate_output_options(args: &PatchArgs) -> Result<(), IdenteditError> {
    if args.diff && args.json {
        return Err(IdenteditError::InvalidRequest {
            message:
                "--diff is only supported in patch flag mode; JSON patch mode always returns JSON."
                    .to_string(),
        });
    }

    if args.diff && !args.dry_run {
        return Err(IdenteditError::InvalidRequest {
            message: "--diff requires --dry-run so preview output cannot be confused with an applied edit."
                .to_string(),
        });
    }

    if args.color.is_some() && !args.diff {
        return Err(IdenteditError::InvalidRequest {
            message: "--color is only meaningful with --diff.".to_string(),
        });
    }

    Ok(())
}
