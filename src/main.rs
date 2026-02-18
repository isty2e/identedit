use std::process::ExitCode;

use clap::Parser;
use identedit::cli::hashline::HashlineCommandOutput;
use identedit::cli::read::ReadCommandOutput;
use identedit::cli::{Cli, Commands};
use identedit::error::IdenteditError;

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let serialized = serde_json::to_string_pretty(&error.to_error_response()).unwrap_or_else(
                |_| {
                    "{\"error\":{\"type\":\"serialization_error\",\"message\":\"Failed to serialize error response\"}}"
                        .to_string()
                },
            );
            println!("{serialized}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, IdenteditError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Read(args) => match identedit::cli::read::run_read(args)? {
            ReadCommandOutput::Text(output) => Ok(output),
            ReadCommandOutput::Json(response) => serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source }),
        },
        Commands::Edit(args) => {
            let response = identedit::cli::edit::run_edit(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::Apply(args) => {
            let response = identedit::cli::apply::run_apply(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::Merge(args) => {
            let response = identedit::cli::merge::run_merge(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::Grammar(args) => {
            let response = identedit::cli::grammar::run_grammar(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::Patch(args) => {
            let response = identedit::cli::patch::run_patch(*args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::LegacySelect(args) => {
            if !legacy_commands_allowed() {
                return Err(legacy_command_removed_error("select"));
            }
            let response = identedit::cli::select::run_select(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::LegacyTransform(args) => {
            if !legacy_commands_allowed() {
                return Err(legacy_command_removed_error("transform"));
            }
            let response = identedit::cli::transform::run_transform(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::LegacyChangeset(args) => {
            if !legacy_commands_allowed() {
                return Err(legacy_command_removed_error("changeset"));
            }
            let response = identedit::cli::changeset::run_changeset(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::LegacyHashline(args) => {
            if !legacy_commands_allowed() {
                return Err(legacy_command_removed_error("hashline"));
            }
            match identedit::cli::hashline::run_hashline(*args)? {
                HashlineCommandOutput::Text(output) => Ok(output),
                HashlineCommandOutput::Json(response) => serde_json::to_string_pretty(&response)
                    .map_err(|source| IdenteditError::ResponseSerialization { source }),
            }
        }
    }
}

fn legacy_commands_allowed() -> bool {
    std::env::var("IDENTEDIT_ALLOW_LEGACY").ok().as_deref() == Some("1")
}

fn legacy_command_removed_error(command: &str) -> IdenteditError {
    let guidance = match command {
        "select" => {
            "Use 'identedit read --mode ast ...' for structural handles, or 'identedit read --mode line ...' for line anchors."
        }
        "transform" => {
            "Use 'identedit edit ...' (flag mode) or 'identedit edit --json' (request mode)."
        }
        "changeset" => "Use 'identedit merge <plan1.json> <plan2.json> ...'.",
        "hashline" => {
            "Use 'identedit read --mode line', 'identedit patch --at <line:hash> ...', or 'identedit apply --repair'."
        }
        _ => "Use canonical commands: read, edit, apply, patch, merge, grammar.",
    };

    IdenteditError::InvalidRequest {
        message: format!(
            "Legacy command '{}' has been removed from the canonical CLI surface. {} Set IDENTEDIT_ALLOW_LEGACY=1 only for transitional test flows.",
            command, guidance
        ),
    }
}
