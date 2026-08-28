use crate::cli::ApplyArgs;
use crate::commands::common;
use colored::Colorize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(args: ApplyArgs) -> anyhow::Result<()> {
    let workspace = common::parse_workspace(&args.config)?;
    let base_dir = args
        .config
        .parent()
        .filter(|p| p.as_os_str() != std::ffi::OsStr::new(""))
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let renders = common::render_all(&workspace, &base_dir)?;

    for hr in &renders {
        common::print_rendered(hr);
    }

    if !args.yes && !confirm()? {
        println!("{}", "aborted, no changes applied".yellow());
        return Ok(());
    }

    let changed = renders.iter().any(has_changes);
    if changed {
        for hr in &renders {
            backup_dir(&hr.dir)?;
        }
    }

    for hr in &renders {
        apply_harness(hr)?;
    }

    println!("{} applied {} harness(es)", "OK".green().bold(), renders.len());
    Ok(())
}

fn has_changes(hr: &common::HarnessRender) -> bool {
    hr.rendered.files.iter().any(|f| {
        let target = hr.dir.join(&f.relative_path);
        std::fs::read_to_string(&target).ok() != Some(f.content.clone())
    }) || hr.rendered.copied_trees.iter().any(|t| {
        let target_dir = hr.dir.join(&t.target_relative_dir);
        crate::output::tree_differs(&t.source_dir, &target_dir)
            != crate::output::tree_differs::TreeStatus::Same
    })
}

fn confirm() -> anyhow::Result<bool> {
    print!("Apply these changes? [y/N] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let answer = line.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

fn backup_dir(dir: &Path) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!("clock error: {e}"))?
        .as_nanos();
    let backup = PathBuf::from(format!("{}.{}.bak", dir.display(), nanos));
    copy_tree(dir, &backup)?;
    println!(
        "{} backup created: {}",
        "backup".blue().bold(),
        backup.display()
    );
    Ok(())
}

fn apply_harness(hr: &common::HarnessRender) -> anyhow::Result<()> {
    std::fs::create_dir_all(&hr.dir)?;
    for file in &hr.rendered.files {
        let target = hr.dir.join(&file.relative_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(&target, &file.content)?;
    }
    for tree in &hr.rendered.copied_trees {
        let target_dir = hr.dir.join(&tree.target_relative_dir);
        copy_tree(&tree.source_dir, &target_dir)?;
    }
    Ok(())
}

fn atomic_write(target: &Path, content: &str) -> anyhow::Result<()> {
    let tmp = target.with_extension(format!(
        "{}.tmp",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("clock error: {e}"))?
            .as_nanos()
    ));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, target)?;
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if !src.exists() {
        anyhow::bail!("source does not exist: {}", src.display());
    }
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
