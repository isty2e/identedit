use std::process::ExitCode;

use clap::Parser;
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
    }
}
