use clap::{Args, Subcommand};

use crate::error::IdenteditError;
use crate::grammar::{GrammarInstallResponse, InstallGrammarRequest, install_grammar};

#[derive(Debug, Args)]
pub struct GrammarArgs {
    #[command(subcommand)]
    pub command: GrammarCommands,
}

#[derive(Debug, Subcommand)]
pub enum GrammarCommands {
    #[command(about = "Install a tree-sitter grammar and register it for runtime loading")]
    Install(GrammarInstallArgs),
}

#[derive(Debug, Args)]
pub struct GrammarInstallArgs {
    #[arg(value_name = "LANG", help = "Language name (e.g. toml, yaml, ruby)")]
    pub language: String,
    #[arg(long, value_name = "URL", help = "Override grammar repository URL")]
    pub repo: Option<String>,
    #[arg(long, value_name = "SYMBOL", help = "Override language symbol name")]
    pub symbol: Option<String>,
    #[arg(
        long = "ext",
        value_name = "EXT",
        help = "Extension to route to this grammar (repeatable)"
    )]
    pub extensions: Vec<String>,
}

pub fn run_grammar(args: GrammarArgs) -> Result<GrammarInstallResponse, IdenteditError> {
    match args.command {
        GrammarCommands::Install(install_args) => {
            let installed = install_grammar(InstallGrammarRequest {
                lang: install_args.language,
                repo: install_args.repo,
                symbol: install_args.symbol,
                extensions: install_args.extensions,
            })?;
            Ok(GrammarInstallResponse { installed })
        }
    }
}
