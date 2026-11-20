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
    #[command(about = "Build and load a pinned or caller-selected native tree-sitter grammar")]
    Install(GrammarInstallArgs),
}

#[derive(Debug, Args)]
pub struct GrammarInstallArgs {
    #[arg(value_name = "LANG", help = "Language name; use --repo if not pinned")]
    pub language: String,
    #[arg(
        long,
        value_name = "SOURCE",
        help = "Build and load native code from this trusted Git URL or path instead of a catalog pin"
    )]
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
