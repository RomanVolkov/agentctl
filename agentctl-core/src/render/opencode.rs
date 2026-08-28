use std::path::PathBuf;

use serde_json::json;

use crate::model::{ResolvedHarness, Workspace};
use crate::render::{collect_skill_trees, CopiedTree, RenderedFile, RenderedOutput, Renderer};

pub struct OpenCodeRenderer;

impl Renderer for OpenCodeRenderer {
    fn render(
        &self,
        workspace: &Workspace,
        harness: &ResolvedHarness,
    ) -> Result<RenderedOutput, crate::Error> {
        let mut config = serde_json::Map::new();
        config.insert(
            "$schema".to_string(),
            json!("https://opencode.ai/config.json"),
        );

        let mut files = Vec::new();

        if let Some(model) = &harness.model {
            config.insert("model".to_string(), json!(model));
        }

        let mut instructions = Vec::new();
        if let Some(sp) = &harness.system_prompt {
            files.push(RenderedFile {
                relative_path: PathBuf::from("system_prompt.md"),
                content: sp.clone(),
            });
            instructions.push("system_prompt.md".to_string());
        }
        if let Some(extra) = &harness.extra {
            append_strings(extra, "instructions", &mut instructions);
            let mut disabled = Vec::new();
            append_strings(extra, "disabled_providers", &mut disabled);
            if !disabled.is_empty() {
                config.insert("disabled_providers".to_string(), json!(disabled));
            }
        }
        if !instructions.is_empty() {
            config.insert("instructions".to_string(), json!(instructions));
        }

        let content = serde_json::to_string_pretty(&serde_json::Value::Object(config))
            .map_err(|e| crate::Error::Render {
                harness: harness.provider.as_str().to_string(),
                message: e.to_string(),
            })?;
        let content = format!("{content}\n");

        files.push(RenderedFile {
            relative_path: PathBuf::from("opencode.jsonc"),
            content,
        });

        let mut trees: Vec<CopiedTree> = collect_skill_trees(workspace, harness);
        let mut output = RenderedOutput {
            files,
            copied_trees: Vec::new(),
            unrendered: Vec::new(),
        };
        output.copied_trees.append(&mut trees);

        warn_if(&mut output.unrendered, harness.guardrails.allowed_dirs.is_some(), "allowed_dirs");
        warn_if(&mut output.unrendered, harness.guardrails.require_approval.is_some(), "require_approval");
        warn_if(&mut output.unrendered, harness.max_spend_limit.is_some(), "max_spend_limit");

        Ok(output)
    }
}

fn warn_if(unrendered: &mut Vec<String>, cond: bool, name: &str) {
    if cond {
        unrendered.push(name.to_string());
    }
}

fn append_strings(extra: &toml::Value, key: &str, target: &mut Vec<String>) {
    if let Some(arr) = extra.get(key).and_then(|v| v.as_array()) {
        for v in arr.iter().filter_map(|v| v.as_str()) {
            if !target.contains(&v.to_string()) {
                target.push(v.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_workspace_from_str;
    use std::path::Path;

    fn workspace_toml() -> &'static str {
        r#"
[global]
model = "claude-sonnet-4"
system_prompt = "You are a senior engineer."
skills = ["plan_make"]

[skills.plan_make]
type = "skill"
path = "skills/plan-make"

[[harnesses]]
provider = "opencode"
config_path = "~/.dotfiles/opencode"
extra = { instructions = ["{env:HOME}/.benjamin-plus/injected-instruction.md"], disabled_providers = ["runpod"] }
"#
    }

    #[test]
    fn renders_opencode_jsonc() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = OpenCodeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();

        let json_file = out.files.iter().find(|f| f.relative_path.ends_with("opencode.jsonc")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_file.content).unwrap();
        assert_eq!(parsed["$schema"], json!("https://opencode.ai/config.json"));
        assert_eq!(parsed["model"], json!("claude-sonnet-4"));
        assert_eq!(
            parsed["instructions"],
            json!(["system_prompt.md", "{env:HOME}/.benjamin-plus/injected-instruction.md"])
        );
        assert_eq!(parsed["disabled_providers"], json!(["runpod"]));
    }

    #[test]
    fn writes_system_prompt_file() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = OpenCodeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        let sp = out.files.iter().find(|f| f.relative_path == Path::new("system_prompt.md")).unwrap();
        assert_eq!(sp.content, "You are a senior engineer.");
    }

    #[test]
    fn copies_skill_trees() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = OpenCodeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        assert_eq!(out.copied_trees.len(), 1);
        assert_eq!(out.copied_trees[0].target_relative_dir, PathBuf::from("skills/plan-make"));
    }

    #[test]
    fn reports_unrendered() {
        let toml = r#"
[global]
allowed_dirs = ["./src"]
require_approval = ["git push"]
max_spend_limit = 5.0

[[harnesses]]
provider = "opencode"
config_path = "~/.dotfiles/opencode"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = OpenCodeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        assert!(out.unrendered.contains(&"allowed_dirs".to_string()));
        assert!(out.unrendered.contains(&"require_approval".to_string()));
        assert!(out.unrendered.contains(&"max_spend_limit".to_string()));
    }

    #[test]
    fn harness_override_wins() {
        let toml = r#"
[global]
model = "global-model"

[[harnesses]]
provider = "opencode"
config_path = "~/.dotfiles/opencode"
model = "override-model"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = OpenCodeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        let file = out.files.iter().find(|f| f.relative_path.ends_with("opencode.jsonc")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&file.content).unwrap();
        assert_eq!(parsed["model"], json!("override-model"));
    }
}