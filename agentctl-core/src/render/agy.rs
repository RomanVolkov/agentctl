use std::path::PathBuf;

use serde_json::json;

use crate::model::{ResolvedHarness, Workspace};
use crate::render::{collect_skill_trees, CopiedTree, RenderedFile, RenderedOutput, Renderer};

pub struct AgyRenderer;

struct AgySettings {
    model: Option<String>,
    allow_non_workspace_access: Option<bool>,
    enable_telemetry: Option<bool>,
    permissions: Option<AgyPermissions>,
}

struct AgyPermissions {
    allow: Vec<String>,
}

impl AgySettings {
    fn camel_case(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(m) = &self.model {
            map.insert("model".into(), json!(m));
        }
        if let Some(v) = self.allow_non_workspace_access {
            map.insert("allowNonWorkspaceAccess".into(), json!(v));
        }
        if let Some(v) = self.enable_telemetry {
            map.insert("enableTelemetry".into(), json!(v));
        }
        if let Some(p) = &self.permissions {
            map.insert("permissions".into(), json!({ "allow": p.allow }));
        }
        serde_json::Value::Object(map)
    }
}

impl Renderer for AgyRenderer {
    fn render(
        &self,
        workspace: &Workspace,
        harness: &ResolvedHarness,
    ) -> Result<RenderedOutput, crate::Error> {
        let mut unrendered = Vec::new();

        let permissions = harness
            .guardrails
            .auto_approve
            .as_ref()
            .map(|allow| AgyPermissions {
                allow: allow.iter().map(|c| format!("command({c})")).collect(),
            });

        let mut extra_map = serde_json::Map::new();
        if let Some(extra) = &harness.extra {
            copy_bool(extra, "allowNonWorkspaceAccess", &mut extra_map);
            copy_bool(extra, "enableTelemetry", &mut extra_map);
        }

        let settings = AgySettings {
            model: harness.model.clone(),
            allow_non_workspace_access: extra_map
                .get("allowNonWorkspaceAccess")
                .and_then(|v| v.as_bool()),
            enable_telemetry: extra_map
                .get("enableTelemetry")
                .and_then(|v| v.as_bool()),
            permissions,
        };

        let content =
            serde_json::to_string_pretty(&settings.camel_case()).map_err(|e| {
                crate::Error::Render {
                    harness: harness.provider.as_str().to_string(),
                    message: e.to_string(),
                }
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
        warn_if(&mut unrendered, harness.guardrails.require_approval.is_some(), "require_approval");
        warn_if(&mut unrendered, harness.max_spend_limit.is_some(), "max_spend_limit");
        output.unrendered = unrendered;

        Ok(output)
    }
}

fn warn_if(unrendered: &mut Vec<String>, cond: bool, name: &str) {
    if cond {
        unrendered.push(name.to_string());
    }
}

fn copy_bool(
    extra: &toml::Value,
    key: &str,
    target: &mut serde_json::Map<String, serde_json::Value>,
) {
    if let Some(v) = extra.get(key).and_then(|v| v.as_bool()) {
        target.insert(key.to_string(), json!(v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_workspace_from_str;

    fn workspace_toml() -> &'static str {
        r#"
[global]
model = "Gemini 3.7 Flash (Medium)"
skills = ["plan_make"]

[skills.plan_make]
type = "skill"
path = "skills/plan-make"

[[harnesses]]
provider = "agy"
config_path = "~/.gemini/antigravity-cli"
auto_approve = ["git status", "ls"]
extra = { allowNonWorkspaceAccess = true, enableTelemetry = false }
"#
    }

    #[test]
    fn renders_agy_settings_json() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = AgyRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();

        assert_eq!(out.files.len(), 1);
        let file = &out.files[0];
        assert_eq!(file.relative_path, PathBuf::from("settings.json"));

        let parsed: serde_json::Value = serde_json::from_str(&file.content).unwrap();
        assert_eq!(parsed["model"], json!("Gemini 3.7 Flash (Medium)"));
        assert_eq!(parsed["allowNonWorkspaceAccess"], json!(true));
        assert_eq!(parsed["enableTelemetry"], json!(false));
        assert_eq!(
            parsed["permissions"]["allow"],
            json!(["command(git status)", "command(ls)"])
        );
    }

    #[test]
    fn global_skills_flow_to_copied_trees() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = AgyRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        assert_eq!(out.copied_trees.len(), 1);
        assert_eq!(out.copied_trees[0].target_relative_dir, PathBuf::from("skills/plan-make"));
    }

    #[test]
    fn omits_model_and_permissions_when_unset() {
        let toml = r#"
[[harnesses]]
provider = "agy"
config_path = "~/.gemini/antigravity-cli"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = AgyRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.files[0].content).unwrap();
        assert!(parsed.get("model").is_none());
        assert!(parsed.get("permissions").is_none());
    }

    #[test]
    fn reports_unrendered() {
        let toml = r#"
[global]
system_prompt = "x"
require_approval = ["git push"]
allowed_dirs = ["/data"]

[[harnesses]]
provider = "agy"
config_path = "~/.gemini/antigravity-cli"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = AgyRenderer.render(&ws, &ws.resolve_harness(&ws.harnesses[0])).unwrap();
        assert!(out.unrendered.contains(&"system_prompt".to_string()));
        assert!(out.unrendered.contains(&"require_approval".to_string()));
        assert!(out.unrendered.contains(&"allowed_dirs".to_string()));
    }
}