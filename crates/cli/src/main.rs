use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::build::build;

mod build;
#[allow(dead_code, unused_imports)]
mod js;
mod manifest;
mod report;

/// CLI tools for the Kobold framework
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build a kobold crate
    #[command(visible_alias = "b")]
    Build,

    /// Create a new kobold crate in an existing directory
    Init,

    /// Start a local development server
    #[command(visible_alias = "s")]
    Serve,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let res = match cli.command {
        Command::Build => build(),
        Command::Init => todo!(),
        Command::Serve => todo!(),
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
