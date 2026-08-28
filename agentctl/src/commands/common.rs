use crate::output;
use colored::Colorize;
use std::path::PathBuf;

pub struct HarnessRender {
    pub harness: agentctl_core::Harness,
    pub rendered: agentctl_core::RenderedOutput,
    pub dir: PathBuf,
}

pub fn parse_workspace(path: &std::path::Path) -> anyhow::Result<agentctl_core::Workspace> {
    let ws = agentctl_core::parse_workspace_file(path)?;
    Ok(agentctl_core::expand_workspace(&ws))
}

pub fn render_all(
    workspace: &agentctl_core::Workspace,
    base_dir: &std::path::Path,
) -> anyhow::Result<Vec<HarnessRender>> {
    let mut out = Vec::new();
    for harness in &workspace.harnesses {
        let rendered = agentctl_core::render(workspace, harness)?;
        let dir = expand_dir(&harness.config_path)?;
        let rendered = resolve_tree_sources(rendered, base_dir);
        out.push(HarnessRender {
            harness: harness.clone(),
            rendered,
            dir,
        });
    }
    Ok(out)
}

fn resolve_tree_sources(
    mut r: agentctl_core::RenderedOutput,
    base_dir: &std::path::Path,
) -> agentctl_core::RenderedOutput {
    for tree in &mut r.copied_trees {
        if tree.source_dir.is_relative() {
            tree.source_dir = base_dir.join(&tree.source_dir);
        }
        let expanded = expand_dir(&tree.source_dir).unwrap_or_else(|e| {
            eprintln!("warning: {e}");
            tree.source_dir.clone()
        });
        tree.source_dir = expanded;
    }
    r
}

pub fn print_rendered(hr: &HarnessRender) {
    println!(
        "\n{} [{}] -> {}",
        "Harness".cyan().bold(),
        hr.harness.provider.as_str(),
        &hr.harness.config_path.display()
    );
    for file in &hr.rendered.files {
        let target = hr.dir.join(&file.relative_path);
        let existing = std::fs::read_to_string(&target).unwrap_or_default();
        output::print_file_diff(&target, &existing, &file.content);
    }
    for tree in &hr.rendered.copied_trees {
        let target_dir = hr.dir.join(&tree.target_relative_dir);
        output::print_tree_changes(&tree.source_dir, &target_dir);
    }
    for msg in &hr.rendered.unrendered {
        println!("  {} {}", "// not rendered:".yellow(), msg.yellow());
    }
}

pub fn expand_dir(path: &std::path::Path) -> anyhow::Result<PathBuf> {
    let s = path.to_string_lossy().into_owned();
    let expanded = shellexpand::full(&s)
        .map_err(|e| anyhow::anyhow!("failed to expand {}: {e}", path.display()))?;
    Ok(PathBuf::from(expanded.into_owned()))
}
