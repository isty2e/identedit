use clap::{Parser, Subcommand};

use crate::cli::patch::PatchCommandOutput;
use crate::cli::read::ReadCommandOutput;
use crate::error::IdenteditError;

mod apply;
mod edit;
mod edit_build;
mod error_response;
mod grammar;
mod line_patch;
mod merge;
mod merge_plan;
mod patch;
mod read;
mod read_select;

pub use error_response::render_error_response;

#[derive(Debug, Parser)]
#[command(name = "identedit", version)]
#[command(about = "Agent-oriented editing engine")]
#[command(
    long_about = "Agent-oriented structural and line-based editing engine. Canonical flow: read -> edit -> apply, with patch for one-shot edits."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Read file structure/content with node or line identities")]
    Read(read::ReadArgs),
    #[command(about = "Build an edit plan from canonical targets")]
    Edit(edit::EditArgs),
    #[command(about = "Commit a prepared edit plan to one or more files")]
    Apply(apply::ApplyArgs),
    #[command(about = "Merge multiple edit plans with strict conflict checks")]
    Merge(merge::MergeArgs),
    #[command(about = "Install dynamic tree-sitter grammars")]
    Grammar(grammar::GrammarArgs),
    #[command(about = "One-shot single-target patch (build + apply)")]
    Patch(Box<patch::PatchArgs>),
}

pub fn run_cli(cli: Cli) -> Result<String, IdenteditError> {
    match cli.command {
        Commands::Read(args) => match read::run_read(args)? {
            ReadCommandOutput::Text(output) => Ok(output),
            ReadCommandOutput::Json(response) => serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source }),
        },
        Commands::Edit(args) => {
            let response = edit::run_edit(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::Apply(args) => {
            let response = apply::run_apply(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::Merge(args) => {
            let response = merge::run_merge(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::Grammar(args) => {
            let response = grammar::run_grammar(args)?;
            serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source })
        }
        Commands::Patch(args) => match patch::run_patch(*args)? {
            PatchCommandOutput::Text(output) => Ok(output),
            PatchCommandOutput::Json(response) => serde_json::to_string_pretty(&response)
                .map_err(|source| IdenteditError::ResponseSerialization { source }),
        },
    }
}
