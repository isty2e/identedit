use std::process::ExitCode;

use clap::Parser;

mod apply;
mod changeset;
mod cli;
mod error;
mod execution_context;
mod grammar;
mod handle;
mod hash;
mod hashline;
mod patch;
mod provider;
mod selector;
mod transform;

use crate::cli::render_error_response;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    match cli::run_cli(cli) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", render_error_response(&error));
            ExitCode::FAILURE
        }
    }
}
