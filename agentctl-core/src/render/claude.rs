use std::path::PathBuf;

use serde_json::json;

use crate::model::{ResolvedHarness, Workspace};
use crate::render::{collect_skill_trees, CopiedTree, RenderedFile, RenderedOutput, Renderer};

pub struct ClaudeRenderer;

impl Renderer for ClaudeRenderer {
    fn render(
        &self,
        workspace: &Workspace,
        harness: &ResolvedHarness,
    ) -> Result<RenderedOutput, crate::Error> {
        let mut unrendered = Vec::new();
        let mut config = serde_json::Map::new();

        if let Some(model) = &harness.model {
            config.insert("model".to_string(), json!(model));
        }

        let allow = harness
            .guardrails
            .auto_approve
            .as_ref()
            .map(|cmds| cmds.iter().map(|c| bash_rule(c)).collect::<Vec<_>>());
        let ask = harness
            .guardrails
            .require_approval
            .as_ref()
            .map(|cmds| cmds.iter().map(|c| bash_rule(c)).collect::<Vec<_>>());

        if allow.is_some() || ask.is_some() {
            let mut permissions = serde_json::Map::new();
            if let Some(a) = allow {
                permissions.insert("allow".to_string(), json!(a));
            }
            if let Some(a) = ask {
                permissions.insert("ask".to_string(), json!(a));
            }
            config.insert("permissions".to_string(), json!(permissions));
        }

        let content = serde_json::to_string_pretty(&serde_json::Value::Object(config))
            .map_err(|e| crate::Error::Render {
                harness: harness.provider.as_str().to_string(),
                message: e.to_string(),
            })?;
        let content = format!("{content}\n");

        let mut trees: Vec<CopiedTree> = collect_skill_trees(workspace, harness);
        let mut output = RenderedOutput {
            files: vec![RenderedFile {
                relative_path: PathBuf::from("settings.json"),
                content,
            }],
            copied_trees: Vec::new(),
            unrendered: Vec::new(),
        };
        output.copied_trees.append(&mut trees);

        warn_if(&mut unrendered, harness.system_prompt.is_some(), "system_prompt");
        warn_if(&mut unrendered, harness.guardrails.allowed_dirs.is_some(), "allowed_dirs");
        warn_if(&mut unrendered, harness.max_spend_limit.is_some(), "max_spend_limit");
        output.unrendered = unrendered;

        Ok(output)
    }
}

fn bash_rule(cmd: &str) -> String {
    if cmd.is_empty() {
        "Bash".to_string()
    } else {
        format!("Bash({cmd})")
    }
}

fn warn_if(unrendered: &mut Vec<String>, cond: bool, name: &str) {
    if cond {
        unrendered.push(name.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_workspace_from_str;

    fn workspace_toml() -> &'static str {
        r#"
[global]
model = "claude-sonnet-4"
skills = ["plan_make"]

[skills.plan_make]
type = "skill"
path = "skills/plan-make"

[[harnesses]]
provider = "claude"
config_path = "~/.claude"
auto_approve = ["git status", "ls"]
require_approval = ["git push"]
"#
    }

    #[test]
    fn renders_claude_settings() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = ClaudeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();

        assert_eq!(out.files.len(), 1);
        assert_eq!(out.files[0].relative_path, PathBuf::from("settings.json"));

        let parsed: serde_json::Value = serde_json::from_str(&out.files[0].content).unwrap();
        assert_eq!(parsed["model"], json!("claude-sonnet-4"));
        assert_eq!(parsed["permissions"]["allow"], json!(["Bash(git status)", "Bash(ls)"]));
        assert_eq!(parsed["permissions"]["ask"], json!(["Bash(git push)"]));
    }

    #[test]
    fn omits_permissions_when_unset() {
        let toml = r#"
[[harnesses]]
provider = "claude"
config_path = "~/.claude"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = ClaudeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.files[0].content).unwrap();
        assert!(parsed.get("permissions").is_none());
    }

    #[test]
    fn copies_skill_trees() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = ClaudeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        assert_eq!(out.copied_trees[0].target_relative_dir, PathBuf::from("skills/plan-make"));
    }

    #[test]
    fn reports_unrendered() {
        let toml = r#"
[global]
system_prompt = "x"
allowed_dirs = ["/src"]
max_spend_limit = 5.0

[[harnesses]]
provider = "claude"
config_path = "~/.claude"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = ClaudeRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        assert!(out.unrendered.contains(&"system_prompt".to_string()));
        assert!(out.unrendered.contains(&"allowed_dirs".to_string()));
        assert!(out.unrendered.contains(&"max_spend_limit".to_string()));
    }
}