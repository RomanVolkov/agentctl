use std::path::PathBuf;

use serde_json::json;

use crate::model::{ResolvedHarness, Workspace};
use crate::render::{
    collect_skill_trees, merge_extra_json, toml_to_json, CopiedTree, RenderedFile, RenderedOutput,
    Renderer,
};

pub struct AgyRenderer;

impl Renderer for AgyRenderer {
    fn render(
        &self,
        workspace: &Workspace,
        harness: &ResolvedHarness,
    ) -> Result<RenderedOutput, crate::Error> {
        let mut unrendered = Vec::new();
        let mut map = serde_json::Map::new();

        if let Some(m) = &harness.model {
            map.insert("model".into(), json!(m));
        }

        if let Some(extra) = &harness.extra {
            copy_bool(extra, "allowNonWorkspaceAccess", &mut map);
            copy_bool(extra, "enableTelemetry", &mut map);
            merge_extra_json(
                &mut map,
                extra,
                &["allowNonWorkspaceAccess", "enableTelemetry", "hooks"],
            );
        }

        let allow = harness
            .guardrails
            .auto_approve
            .as_ref()
            .map(|cmds| cmds.iter().map(|c| agy_rule(c)).collect::<Vec<_>>());
        let ask = harness
            .guardrails
            .require_approval
            .as_ref()
            .map(|cmds| cmds.iter().map(|c| agy_rule(c)).collect::<Vec<_>>());

        if allow.is_some() || ask.is_some() {
            let mut perms = serde_json::Map::new();
            if let Some(a) = allow {
                perms.insert("allow".into(), json!(a));
            }
            if let Some(a) = ask {
                perms.insert("ask".into(), json!(a));
            }
            map.insert("permissions".into(), serde_json::Value::Object(perms));
        }

        let content = serde_json::to_string_pretty(&serde_json::Value::Object(map)).map_err(|e| {
            crate::Error::Render {
                harness: harness.provider.as_str().to_string(),
                message: e.to_string(),
            }
        })?;
        let content = format!("{content}\n");

        let mut files = vec![RenderedFile {
            relative_path: PathBuf::from("settings.json"),
            content,
        }];

        if let Some(sp) = &harness.system_prompt {
            files.push(RenderedFile {
                relative_path: PathBuf::from("AGENTS.md"),
                content: sp.clone(),
            });
        }

        if let Some(hooks_val) = harness.extra.as_ref().and_then(|e| e.get("hooks")) {
            let hooks_json =
                serde_json::to_string_pretty(&toml_to_json(hooks_val)).map_err(|e| {
                    crate::Error::Render {
                        harness: harness.provider.as_str().to_string(),
                        message: e.to_string(),
                    }
                })?;
            files.push(RenderedFile {
                relative_path: PathBuf::from("hooks.json"),
                content: format!("{hooks_json}\n"),
            });
        }

        let mut trees: Vec<CopiedTree> = collect_skill_trees(workspace, harness);
        let mut output = RenderedOutput {
            files,
            copied_trees: Vec::new(),
            unrendered: Vec::new(),
        };
        output.copied_trees.append(&mut trees);

        warn_if(
            &mut unrendered,
            harness.guardrails.allowed_dirs.is_some(),
            "allowed_dirs",
        );
        warn_if(
            &mut unrendered,
            harness.max_spend_limit.is_some(),
            "max_spend_limit",
        );
        output.unrendered = unrendered;

        Ok(output)
    }
}

fn agy_rule(cmd: &str) -> String {
    if cmd.starts_with("command(")
        || cmd.starts_with("read_file(")
        || cmd.starts_with("write_file(")
        || cmd.starts_with("read_url(")
        || cmd.starts_with("execute_url(")
        || cmd.starts_with("unsandboxed(")
        || cmd.starts_with("mcp(")
    {
        cmd.to_string()
    } else {
        format!("command({cmd})")
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
require_approval = ["git push"]
extra = { allowNonWorkspaceAccess = true, enableTelemetry = false, toolPermission = "request-review" }
"#
    }

    #[test]
    fn renders_agy_settings_json() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = AgyRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();

        assert_eq!(out.files.len(), 1);
        let file = &out.files[0];
        assert_eq!(file.relative_path, PathBuf::from("settings.json"));

        let parsed: serde_json::Value = serde_json::from_str(&file.content).unwrap();
        assert_eq!(parsed["model"], json!("Gemini 3.7 Flash (Medium)"));
        assert_eq!(parsed["allowNonWorkspaceAccess"], json!(true));
        assert_eq!(parsed["enableTelemetry"], json!(false));
        assert_eq!(parsed["toolPermission"], json!("request-review"));
        assert_eq!(
            parsed["permissions"]["allow"],
            json!(["command(git status)", "command(ls)"])
        );
        assert_eq!(
            parsed["permissions"]["ask"],
            json!(["command(git push)"])
        );
    }

    #[test]
    fn renders_agy_agents_md_and_hooks_json() {
        let toml = r#"
[global]
system_prompt = "Follow Benjamin-Plus rules."

[[harnesses]]
provider = "agy"
config_path = "~/.gemini/config"

[harnesses.extra.hooks.benjamin-plus]
enabled = true
SessionStart = [{ type = "command", command = "bash ~/.benjamin-plus/sessionstart-agy.sh" }]
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = AgyRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();

        let agents_md = out
            .files
            .iter()
            .find(|f| f.relative_path == std::path::Path::new("AGENTS.md"))
            .unwrap();
        assert_eq!(agents_md.content, "Follow Benjamin-Plus rules.");

        let hooks_json = out
            .files
            .iter()
            .find(|f| f.relative_path == std::path::Path::new("hooks.json"))
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&hooks_json.content).unwrap();
        assert_eq!(parsed["benjamin-plus"]["enabled"], json!(true));
        assert_eq!(
            parsed["benjamin-plus"]["SessionStart"][0]["command"],
            json!("bash ~/.benjamin-plus/sessionstart-agy.sh")
        );
    }

    #[test]
    fn global_skills_flow_to_copied_trees() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = AgyRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();
        assert_eq!(out.copied_trees.len(), 1);
        assert_eq!(
            out.copied_trees[0].target_relative_dir,
            PathBuf::from("skills/plan-make")
        );
    }

    #[test]
    fn omits_model_and_permissions_when_unset() {
        let toml = r#"
[[harnesses]]
provider = "agy"
config_path = "~/.gemini/antigravity-cli"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = AgyRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.files[0].content).unwrap();
        assert!(parsed.get("model").is_none());
        assert!(parsed.get("permissions").is_none());
    }

    #[test]
    fn reports_unrendered() {
        let toml = r#"
[global]
allowed_dirs = ["/data"]
max_spend_limit = 5.0

[[harnesses]]
provider = "agy"
config_path = "~/.gemini/antigravity-cli"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = AgyRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();
        assert!(out.unrendered.contains(&"allowed_dirs".to_string()));
        assert!(out.unrendered.contains(&"max_spend_limit".to_string()));
    }
}
