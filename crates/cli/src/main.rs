use std::io::{self, IsTerminal};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use crate::build::build;

mod build;
#[allow(dead_code, unused_imports)]
mod js;
mod log;
mod manifest;
mod report;

/// CLI tools for the Kobold framework
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Use verbose output
    #[arg(short, long)]
    verbose: bool,

    /// When to use colors in output
    #[arg(long, default_value_t, value_enum)]
    color: When,
}

#[derive(Subcommand)]
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

#[derive(Clone, Copy, Default, ValueEnum)]
enum When {
    #[default]
    Auto,
    Always,
    Never,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.verbose {
        log::enable_verbose_output();
    }

    match cli.color {
        When::Auto if io::stdout().is_terminal() => log::enable_color_output(),
        When::Always => log::enable_color_output(),
        _ => {}
    }

    let res = match cli.command {
        Command::Build => build(),
        Command::Init => todo!(),
        Command::Serve => todo!(),
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log::error!("{err}");
            ExitCode::FAILURE
        }
    }
}
