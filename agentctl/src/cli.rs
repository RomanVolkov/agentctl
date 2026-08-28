use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "agentctl", version, about = "Declarative IaC for AI agent harness configurations")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Validate workspace.toml and report reference or schema errors
    Validate(ValidateArgs),
    /// Render all harnesses and print diffs and asset changes without writing
    Preview(PreviewArgs),
    /// Render and write harness configs after confirmation
    Apply(ApplyArgs),
}

#[derive(Debug, clap::Args)]
pub struct ValidateArgs {
    /// Path to workspace.toml
    #[arg(short, long, default_value = "workspace.toml")]
    pub config: PathBuf,
}

#[derive(Debug, clap::Args)]
pub struct PreviewArgs {
    /// Path to workspace.toml
    #[arg(short, long, default_value = "workspace.toml")]
    pub config: PathBuf,
}

#[derive(Debug, clap::Args)]
pub struct ApplyArgs {
    /// Path to workspace.toml
    #[arg(short, long, default_value = "workspace.toml")]
    pub config: PathBuf,
    /// Skip the interactive confirmation prompt
    #[arg(long)]
    pub yes: bool,
}
