pub mod agy;
pub mod claude;
pub mod codex;
pub mod opencode;

use std::path::PathBuf;

use crate::model::{Harness, ResolvedHarness, Skill, Workspace};

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedFile {
    pub relative_path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CopiedTree {
    pub source_dir: PathBuf,
    pub target_relative_dir: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderedOutput {
    pub files: Vec<RenderedFile>,
    pub copied_trees: Vec<CopiedTree>,
    /// Primitives that were declared but this harness cannot express.
    pub unrendered: Vec<String>,
}

pub trait Renderer {
    fn render(
        &self,
        workspace: &Workspace,
        harness: &ResolvedHarness,
    ) -> Result<RenderedOutput, crate::Error>;
}

pub fn render(workspace: &Workspace, harness: &Harness) -> Result<RenderedOutput, crate::Error> {
    let resolved = workspace.resolve_harness(harness);
    render_resolved(workspace, &resolved)
}

pub fn render_resolved(
    workspace: &Workspace,
    harness: &ResolvedHarness,
) -> Result<RenderedOutput, crate::Error> {
    match harness.provider {
        crate::Provider::OpenCode => opencode::OpenCodeRenderer.render(workspace, harness),
        crate::Provider::Agy => agy::AgyRenderer.render(workspace, harness),
        crate::Provider::Claude => claude::ClaudeRenderer.render(workspace, harness),
        crate::Provider::Codex => codex::CodexRenderer.render(workspace, harness),
    }
}

pub fn collect_skill_trees(workspace: &Workspace, resolved: &ResolvedHarness) -> Vec<CopiedTree> {
    resolved
        .skills
        .iter()
        .filter_map(|name| workspace.skills.get(name))
        .filter_map(|skill| match skill {
            Skill::SkillDir { path } => {
                let target = PathBuf::from("skills").join(skill_name(path));
                Some(CopiedTree {
                    source_dir: path.clone(),
                    target_relative_dir: target,
                })
            }
            _ => None,
        })
        .collect()
}

fn skill_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "skill".to_string())
}