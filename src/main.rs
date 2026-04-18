use std::process::ExitCode;

use clap::Parser;
use identedit::cli::render_error_response;

fn main() -> ExitCode {
    let cli = identedit::cli::Cli::parse();

    match identedit::cli::run_cli(cli) {
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
