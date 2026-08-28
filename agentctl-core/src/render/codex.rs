use std::path::PathBuf;

use crate::model::{ResolvedHarness, Workspace};
use crate::render::{collect_skill_trees, CopiedTree, RenderedFile, RenderedOutput, Renderer};

pub struct CodexRenderer;

const COARSE_APPROVALS: &str = "auto_approve/require_approval approximated by approval_policy";
const COARSE_DIRS: &str = "allowed_dirs approximated by sandbox_mode";

impl Renderer for CodexRenderer {
    fn render(
        &self,
        workspace: &Workspace,
        harness: &ResolvedHarness,
    ) -> Result<RenderedOutput, crate::Error> {
        let mut table = toml::map::Map::new();

        if let Some(model) = &harness.model {
            table.insert("model".into(), toml::Value::String(model.clone()));
        }

        let mut unrendered = Vec::new();
        if harness.guardrails.auto_approve.is_some() || harness.guardrails.require_approval.is_some() {
            let policy = if harness.guardrails.require_approval.as_ref().is_some_and(|v| !v.is_empty())
            {
                "on-request"
            } else {
                "never"
            };
            table.insert(
                "approval_policy".into(),
                toml::Value::String(policy.into()),
            );
            unrendered.push(COARSE_APPROVALS.to_string());
        }

        if let Some(dirs) = &harness.guardrails.allowed_dirs {
            if !dirs.is_empty() {
                table.insert(
                    "sandbox_mode".into(),
                    toml::Value::String("workspace-write".into()),
                );
                unrendered.push(COARSE_DIRS.to_string());
            }
        }

        if let Some(extra) = &harness.extra {
            merge_extra(&mut table, extra);
        }

        let content = toml::to_string_pretty(&toml::Value::Table(table))
            .map_err(|e| crate::Error::Render {
                harness: harness.provider.as_str().to_string(),
                message: e.to_string(),
            })?;

        let mut trees: Vec<CopiedTree> = collect_skill_trees(workspace, harness);
        let mut output = RenderedOutput {
            files: vec![RenderedFile {
                relative_path: PathBuf::from("config.toml"),
                content,
            }],
            copied_trees: Vec::new(),
            unrendered: Vec::new(),
        };
        output.copied_trees.append(&mut trees);

        if harness.system_prompt.is_some() {
            unrendered.push("system_prompt".to_string());
        }
        if harness.max_spend_limit.is_some() {
            unrendered.push("max_spend_limit".to_string());
        }
        output.unrendered = unrendered;

        Ok(output)
    }
}

fn merge_extra(table: &mut toml::map::Map<String, toml::Value>, extra: &toml::Value) {
    let Some(extra_table) = extra.as_table() else {
        return;
    };
    for (k, v) in extra_table {
        table.entry(k.clone()).or_insert_with(|| v.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_workspace_from_str;

    fn workspace_toml() -> &'static str {
        r#"
[global]
model = "gpt-5.6"
skills = ["plan_make"]

[skills.plan_make]
type = "skill"
path = "skills/plan-make"

[[harnesses]]
provider = "codex"
config_path = "~/.codex"
auto_approve = ["git status"]
extra = { notify = ["/path/to/notify.sh"] }
"#
    }

    #[test]
    fn renders_codex_config() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = CodexRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();

        assert_eq!(out.files.len(), 1);
        assert_eq!(out.files[0].relative_path, PathBuf::from("config.toml"));

        // `notify` and `auto_approve` live on the harness as top-level keys after parse.
        let parsed: toml::Value = toml::from_str(&out.files[0].content).unwrap();
        assert_eq!(parsed["model"].as_str(), Some("gpt-5.6"));
        assert_eq!(parsed["approval_policy"].as_str(), Some("never"));
        assert_eq!(parsed["notify"], toml::Value::Array(vec![toml::Value::String("/path/to/notify.sh".into())]));
    }

    #[test]
    fn require_approval_maps_to_on_request() {
        let toml = r#"
[[harnesses]]
provider = "codex"
config_path = "~/.codex"
require_approval = ["git push"]
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = CodexRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        let parsed: toml::Value = toml::from_str(&out.files[0].content).unwrap();
        assert_eq!(parsed["approval_policy"].as_str(), Some("on-request"));
        assert!(out.unrendered.iter().any(|m| m.starts_with("auto_approve/require_approval")));
    }

    #[test]
    fn allowed_dirs_map_to_workspace_write() {
        let toml = r#"
[[harnesses]]
provider = "codex"
config_path = "~/.codex"
allowed_dirs = ["/src"]
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = CodexRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        let parsed: toml::Value = toml::from_str(&out.files[0].content).unwrap();
        assert_eq!(parsed["sandbox_mode"].as_str(), Some("workspace-write"));
        assert!(out.unrendered.iter().any(|m| m.starts_with("allowed_dirs")));
    }

    #[test]
    fn copies_skill_trees_and_reports_unrendered() {
        let toml = r#"
[global]
model = "gpt-5.6"
system_prompt = "x"
max_spend_limit = 1.0
skills = ["plan_make"]

[skills.plan_make]
type = "skill"
path = "skills/plan-make"

[[harnesses]]
provider = "codex"
config_path = "~/.codex"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = CodexRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        assert_eq!(out.copied_trees[0].target_relative_dir, PathBuf::from("skills/plan-make"));
        assert!(out.unrendered.contains(&"system_prompt".to_string()));
        assert!(out.unrendered.contains(&"max_spend_limit".to_string()));
    }
}