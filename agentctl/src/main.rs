mod cli;
mod commands;
mod output;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate(args) => commands::validate::run(args),
        Command::Preview(args) => commands::preview::run(args),
        Command::Apply(args) => commands::apply::run(args),
    }
}
