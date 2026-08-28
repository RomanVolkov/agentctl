use crate::cli::ValidateArgs;
use crate::commands::common;
use colored::Colorize;

pub fn run(args: ValidateArgs) -> anyhow::Result<()> {
    match common::parse_workspace(&args.config) {
        Ok(workspace) => {
            println!(
                "{} workspace {} ({} harnesses, {} skills, {} guardrails set)",
                "OK".green().bold(),
                args.config.display(),
                workspace.harnesses.len(),
                workspace.skills.len(),
                workspace.global.guardrails != Default::default()
            );
            Ok(())
        }
        Err(err) => {
            eprintln!("{} {}", "error:".red().bold(), err);
            std::process::exit(1);
        }
    }
}
