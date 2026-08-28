use crate::cli::PreviewArgs;
use crate::commands::common;
use colored::Colorize;

pub fn run(args: PreviewArgs) -> anyhow::Result<()> {
    let workspace = common::parse_workspace(&args.config)?;
    let base_dir = args
        .config
        .parent()
        .filter(|p| p.as_os_str() != std::ffi::OsStr::new(""))
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    println!(
        "{} rendering {} harness(es)",
        "=".green().bold(),
        workspace.harnesses.len()
    );

    for hr in common::render_all(&workspace, &base_dir)? {
        common::print_rendered(&hr);
    }
    Ok(())
}
