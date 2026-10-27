use std::path::PathBuf;

use crate::model::{Harness, ResolvedHarness, Skill, Workspace};

pub mod agy;
pub mod claude;
pub mod codex;
pub mod opencode;

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

pub fn toml_to_json(val: &toml::Value) -> serde_json::Value {
    match val {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(*i),
        toml::Value::Float(f) => serde_json::json!(*f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_to_json).collect())
        }
        toml::Value::Table(tbl) => {
            let mut map = serde_json::Map::new();
            for (k, v) in tbl {
                map.insert(k.clone(), toml_to_json(v));
            }
            serde_json::Value::Object(map)
        }
    }
}

pub fn merge_extra_json(
    target: &mut serde_json::Map<String, serde_json::Value>,
    extra: &toml::Value,
    skip_keys: &[&str],
) {
    let Some(tbl) = extra.as_table() else {
        return;
    };
    for (k, v) in tbl {
        if skip_keys.contains(&k.as_str()) {
            continue;
        }
        target.entry(k.clone()).or_insert_with(|| toml_to_json(v));
    }
}

fn skill_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "skill".to_string())
}
