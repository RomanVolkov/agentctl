use std::path::PathBuf;

use serde_json::json;

use crate::model::{ResolvedHarness, Workspace};
use crate::render::{
    collect_mcp_servers, collect_skill_trees, merge_extra_json, toml_to_json, CopiedTree,
    RenderedFile, RenderedOutput, Renderer,
};

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

        if let Some(extra) = &harness.extra {
            merge_extra_json(&mut config, extra, &["mcpServers"]);
        }

        let content = serde_json::to_string_pretty(&serde_json::Value::Object(config))
            .map_err(|e| crate::Error::Render {
                harness: harness.provider.as_str().to_string(),
                message: e.to_string(),
            })?;
        let content = format!("{content}\n");

        let mut files = vec![RenderedFile {
            relative_path: PathBuf::from("settings.json"),
            content,
        }];

        if let Some(sp) = &harness.system_prompt {
            files.push(RenderedFile {
                relative_path: PathBuf::from("CLAUDE.md"),
                content: sp.clone(),
            });
        }

        let mut mcp_servers = serde_json::Map::new();
        for mcp in collect_mcp_servers(workspace, harness) {
            let mut entry = serde_json::Map::new();
            if let Some(url) = mcp.url {
                entry.insert("type".to_string(), json!("http"));
                entry.insert("url".to_string(), json!(url));
            } else if let Some(cmd) = mcp.command {
                entry.insert("type".to_string(), json!("stdio"));
                entry.insert("command".to_string(), json!(cmd));
                if !mcp.args.is_empty() {
                    entry.insert("args".to_string(), json!(mcp.args));
                }
            }
            if !mcp.env.is_empty() {
                entry.insert("env".to_string(), json!(mcp.env));
            }
            mcp_servers.insert(mcp.name, serde_json::Value::Object(entry));
        }
        if let Some(extra_mcp) = harness
            .extra
            .as_ref()
            .and_then(|e| e.get("mcpServers"))
            .and_then(|v| v.as_table())
        {
            for (k, v) in extra_mcp {
                mcp_servers
                    .entry(k.clone())
                    .or_insert_with(|| toml_to_json(v));
            }
        }
        if !mcp_servers.is_empty() {
            let mcp_doc = json!({ "mcpServers": mcp_servers });
            let mcp_json =
                serde_json::to_string_pretty(&mcp_doc).map_err(|e| crate::Error::Render {
                    harness: harness.provider.as_str().to_string(),
                    message: e.to_string(),
                })?;
            files.push(RenderedFile {
                relative_path: PathBuf::from(".mcp.json"),
                content: format!("{mcp_json}\n"),
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

fn bash_rule(cmd: &str) -> String {
    if cmd.is_empty() {
        "Bash".to_string()
    } else if cmd.starts_with("Bash(") || cmd.starts_with("Read(") || cmd.starts_with("Write(") {
        cmd.to_string()
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

[harnesses.extra]
theme = "auto"

[harnesses.extra.hooks]
SessionStart = [{ matcher = "startup|resume|clear|compact", hooks = [{ type = "command", command = "cat ~/.benjamin-plus/injected-instruction.md" }] }]
"#
    }

    #[test]
    fn renders_claude_settings() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = ClaudeRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();

        assert_eq!(out.files.len(), 1);
        assert_eq!(out.files[0].relative_path, PathBuf::from("settings.json"));

        let parsed: serde_json::Value = serde_json::from_str(&out.files[0].content).unwrap();
        assert_eq!(parsed["model"], json!("claude-sonnet-4"));
        assert_eq!(parsed["theme"], json!("auto"));
        assert_eq!(
            parsed["permissions"]["allow"],
            json!(["Bash(git status)", "Bash(ls)"])
        );
        assert_eq!(parsed["permissions"]["ask"], json!(["Bash(git push)"]));
        assert_eq!(
            parsed["hooks"]["SessionStart"][0]["matcher"],
            json!("startup|resume|clear|compact")
        );
    }

    #[test]
    fn renders_claude_md_when_system_prompt_set() {
        let toml = r#"
[global]
system_prompt = "Follow Benjamin-Plus."

[[harnesses]]
provider = "claude"
config_path = "~/.claude"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = ClaudeRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();
        let claude_md = out
            .files
            .iter()
            .find(|f| f.relative_path == std::path::Path::new("CLAUDE.md"))
            .unwrap();
        assert_eq!(claude_md.content, "Follow Benjamin-Plus.");
    }

    #[test]
    fn omits_permissions_when_unset() {
        let toml = r#"
[[harnesses]]
provider = "claude"
config_path = "~/.claude"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = ClaudeRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out.files[0].content).unwrap();
        assert!(parsed.get("permissions").is_none());
    }

    #[test]
    fn copies_skill_trees() {
        let ws = parse_workspace_from_str(workspace_toml()).unwrap();
        let out = ClaudeRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();
        assert_eq!(
            out.copied_trees[0].target_relative_dir,
            PathBuf::from("skills/plan-make")
        );
    }

    #[test]
    fn reports_unrendered() {
        let toml = r#"
[global]
allowed_dirs = ["/src"]
max_spend_limit = 5.0

[[harnesses]]
provider = "claude"
config_path = "~/.claude"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let out = ClaudeRenderer
            .render(&ws, &ws.resolve_harness(&ws.harnesses[0]))
            .unwrap();
        assert!(out.unrendered.contains(&"allowed_dirs".to_string()));
        assert!(out.unrendered.contains(&"max_spend_limit".to_string()));
    }
}
