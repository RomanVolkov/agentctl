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

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedMcp {
    pub name: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    pub url: Option<String>,
}

pub fn collect_mcp_servers(workspace: &Workspace, resolved: &ResolvedHarness) -> Vec<ResolvedMcp> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for name in resolved.skills.iter().chain(resolved.mcp.iter()) {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(Skill::Mcp {
            command,
            args,
            env,
            url,
        }) = workspace.skills.get(name)
        {
            out.push(ResolvedMcp {
                name: name.clone(),
                command: command.clone(),
                args: args.clone(),
                env: env.clone(),
                url: url.clone(),
            });
        }
    }
    out
}

/// Preserves unmanaged MCP servers, MCP permissions, and manual provider blocks from existing
/// harness configuration files on disk so `agentctl apply` never clobbers work/machine-local MCPs.
pub fn merge_existing_harness_files(
    provider: crate::Provider,
    dir: &std::path::Path,
    rendered: &mut RenderedOutput,
) {
    match provider {
        crate::Provider::OpenCode => merge_existing_opencode(dir, rendered),
        crate::Provider::Agy => merge_existing_agy(dir, rendered),
        crate::Provider::Claude => merge_existing_claude(dir, rendered),
        crate::Provider::Codex => merge_existing_codex(dir, rendered),
    }
}

fn merge_existing_opencode(dir: &std::path::Path, rendered: &mut RenderedOutput) {
    let existing_path = dir.join("opencode.jsonc");
    let Ok(existing_raw) = std::fs::read_to_string(&existing_path) else {
        return;
    };
    let Ok(serde_json::Value::Object(existing_obj)) = serde_json::from_str(&existing_raw) else {
        return;
    };
    let Some(file) = rendered
        .files
        .iter_mut()
        .find(|f| f.relative_path == std::path::Path::new("opencode.jsonc"))
    else {
        return;
    };
    let Ok(serde_json::Value::Object(mut rendered_obj)) = serde_json::from_str(&file.content)
    else {
        return;
    };

    let mut changed = false;
    if let Some(serde_json::Value::Object(existing_mcp)) = existing_obj.get("mcp") {
        if !existing_mcp.is_empty() {
            let target_mcp = rendered_obj
                .entry("mcp".to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let serde_json::Value::Object(target_map) = target_mcp {
                for (k, v) in existing_mcp {
                    if !target_map.contains_key(k) {
                        target_map.insert(k.clone(), v.clone());
                        changed = true;
                    }
                }
            }
        }
    }
    if let Some(existing_provider) = existing_obj.get("provider") {
        if !rendered_obj.contains_key("provider") {
            rendered_obj.insert("provider".to_string(), existing_provider.clone());
            changed = true;
        }
    }

    if changed {
        if let Ok(s) = serde_json::to_string_pretty(&serde_json::Value::Object(rendered_obj)) {
            file.content = format!("{s}\n");
        }
    }
}

fn merge_existing_agy(dir: &std::path::Path, rendered: &mut RenderedOutput) {
    if let Some(settings_file) = rendered
        .files
        .iter_mut()
        .find(|f| f.relative_path == std::path::Path::new("settings.json"))
    {
        if let Ok(existing_raw) = std::fs::read_to_string(dir.join("settings.json")) {
            if let (
                Ok(serde_json::Value::Object(existing_obj)),
                Ok(serde_json::Value::Object(mut rendered_obj)),
            ) = (
                serde_json::from_str(&existing_raw),
                serde_json::from_str(&settings_file.content),
            ) {
                let mut changed =
                    merge_prefixed_permission_rules(&existing_obj, &mut rendered_obj, "mcp(");

                for key in ["trustedWorkspaces"] {
                    if let Some(val) = existing_obj.get(key) {
                        if !rendered_obj.contains_key(key) {
                            rendered_obj.insert(key.to_string(), val.clone());
                            changed = true;
                        }
                    }
                }

                if changed {
                    if let Ok(s) =
                        serde_json::to_string_pretty(&serde_json::Value::Object(rendered_obj))
                    {
                        settings_file.content = format!("{s}\n");
                    }
                }
            }
        }
    }

    if let Some(mcp_file) = rendered
        .files
        .iter_mut()
        .find(|f| f.relative_path == std::path::Path::new("mcp_config.json"))
    {
        merge_json_mcp_servers(&dir.join("mcp_config.json"), mcp_file);
    }
}

fn merge_existing_claude(dir: &std::path::Path, rendered: &mut RenderedOutput) {
    if let Some(settings_file) = rendered
        .files
        .iter_mut()
        .find(|f| f.relative_path == std::path::Path::new("settings.json"))
    {
        if let Ok(existing_raw) = std::fs::read_to_string(dir.join("settings.json")) {
            if let (
                Ok(serde_json::Value::Object(existing_obj)),
                Ok(serde_json::Value::Object(mut rendered_obj)),
            ) = (
                serde_json::from_str(&existing_raw),
                serde_json::from_str(&settings_file.content),
            ) {
                let mut changed =
                    merge_prefixed_permission_rules(&existing_obj, &mut rendered_obj, "mcp__");

                for key in [
                    "enabledMcpjsonServers",
                    "disabledMcpjsonServers",
                    "enableAllProjectMcpServers",
                    "allowedMcpServers",
                    "deniedMcpServers",
                ] {
                    if let Some(val) = existing_obj.get(key) {
                        if !rendered_obj.contains_key(key) {
                            rendered_obj.insert(key.to_string(), val.clone());
                            changed = true;
                        }
                    }
                }

                if let Some(serde_json::Value::Object(existing_mcp)) =
                    existing_obj.get("mcpServers")
                {
                    if !existing_mcp.is_empty() {
                        let target_mcp = rendered_obj
                            .entry("mcpServers".to_string())
                            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                        if let serde_json::Value::Object(target_map) = target_mcp {
                            for (k, v) in existing_mcp {
                                if !target_map.contains_key(k) {
                                    target_map.insert(k.clone(), v.clone());
                                    changed = true;
                                }
                            }
                        }
                    }
                }

                if changed {
                    if let Ok(s) =
                        serde_json::to_string_pretty(&serde_json::Value::Object(rendered_obj))
                    {
                        settings_file.content = format!("{s}\n");
                    }
                }
            }
        }
    }

    if let Some(mcp_file) = rendered
        .files
        .iter_mut()
        .find(|f| f.relative_path == std::path::Path::new(".mcp.json"))
    {
        merge_json_mcp_servers(&dir.join(".mcp.json"), mcp_file);

        // When targeting user-level ~/.claude, Claude Code reads user-scoped MCPs from
        // the sibling ~/.claude.json file under "mcpServers". Merge declared MCPs into
        // ~/.claude.json while preserving all existing state and unmanaged MCP servers.
        if dir.file_name().and_then(|n| n.to_str()) == Some(".claude") {
            let claude_json_path = dir.with_extension("json");
            if let Ok(existing_raw) = std::fs::read_to_string(&claude_json_path) {
                if let (
                    Ok(serde_json::Value::Object(mut claude_json_obj)),
                    Ok(serde_json::Value::Object(rendered_mcp_doc)),
                ) = (
                    serde_json::from_str(&existing_raw),
                    serde_json::from_str(&mcp_file.content),
                ) {
                    if let Some(serde_json::Value::Object(declared_servers)) =
                        rendered_mcp_doc.get("mcpServers")
                    {
                        let target_mcp = claude_json_obj
                            .entry("mcpServers".to_string())
                            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                        if let serde_json::Value::Object(target_map) = target_mcp {
                            for (k, v) in declared_servers {
                                target_map.insert(k.clone(), v.clone());
                            }
                        }
                        if let Ok(s) = serde_json::to_string_pretty(&serde_json::Value::Object(
                            claude_json_obj,
                        )) {
                            rendered.files.push(RenderedFile {
                                relative_path: PathBuf::from("../.claude.json"),
                                content: format!("{s}\n"),
                            });
                        }
                    }
                }
            }
        }
    }
}

fn merge_existing_codex(dir: &std::path::Path, rendered: &mut RenderedOutput) {
    let existing_path = dir.join("config.toml");
    let Ok(existing_raw) = std::fs::read_to_string(&existing_path) else {
        return;
    };
    let Ok(toml::Value::Table(existing_tbl)) = toml::from_str(&existing_raw) else {
        return;
    };
    let Some(toml::Value::Table(existing_mcp)) = existing_tbl.get("mcp_servers") else {
        return;
    };
    if existing_mcp.is_empty() {
        return;
    }
    let Some(file) = rendered
        .files
        .iter_mut()
        .find(|f| f.relative_path == std::path::Path::new("config.toml"))
    else {
        return;
    };
    let Ok(toml::Value::Table(mut rendered_tbl)) = toml::from_str(&file.content) else {
        return;
    };

    let mut changed = false;
    let target_mcp = rendered_tbl
        .entry("mcp_servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if let toml::Value::Table(target_map) = target_mcp {
        for (k, v) in existing_mcp {
            if !target_map.contains_key(k) {
                target_map.insert(k.clone(), v.clone());
                changed = true;
            }
        }
    }

    if changed {
        if let Ok(s) = toml::to_string_pretty(&toml::Value::Table(rendered_tbl)) {
            file.content = s;
        }
    }
}

fn merge_json_mcp_servers(existing_path: &std::path::Path, mcp_file: &mut RenderedFile) {
    let Ok(existing_raw) = std::fs::read_to_string(existing_path) else {
        return;
    };
    let Ok(serde_json::Value::Object(existing_obj)) = serde_json::from_str(&existing_raw) else {
        return;
    };
    let Some(serde_json::Value::Object(existing_mcp)) = existing_obj.get("mcpServers") else {
        return;
    };
    let Ok(serde_json::Value::Object(mut rendered_obj)) = serde_json::from_str(&mcp_file.content)
    else {
        return;
    };
    let mut changed = false;
    let target_mcp = rendered_obj
        .entry("mcpServers".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let serde_json::Value::Object(target_map) = target_mcp {
        for (k, v) in existing_mcp {
            if !target_map.contains_key(k) {
                target_map.insert(k.clone(), v.clone());
                changed = true;
            }
        }
    }
    if changed {
        if let Ok(s) = serde_json::to_string_pretty(&serde_json::Value::Object(rendered_obj)) {
            mcp_file.content = format!("{s}\n");
        }
    }
}

fn merge_prefixed_permission_rules(
    existing_obj: &serde_json::Map<String, serde_json::Value>,
    rendered_obj: &mut serde_json::Map<String, serde_json::Value>,
    prefix: &str,
) -> bool {
    let Some(serde_json::Value::Object(existing_perms)) = existing_obj.get("permissions") else {
        return false;
    };
    let mut changed = false;
    for bucket in ["allow", "ask", "deny"] {
        let Some(serde_json::Value::Array(existing_rules)) = existing_perms.get(bucket) else {
            continue;
        };
        let matching: Vec<serde_json::Value> = existing_rules
            .iter()
            .filter(|v| v.as_str().is_some_and(|s| s.starts_with(prefix)))
            .cloned()
            .collect();
        if matching.is_empty() {
            continue;
        }
        let perms = rendered_obj
            .entry("permissions".to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let serde_json::Value::Object(perms_map) = perms {
            let arr = perms_map
                .entry(bucket.to_string())
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            if let serde_json::Value::Array(target_arr) = arr {
                for rule in matching {
                    if !target_arr.contains(&rule) {
                        target_arr.push(rule);
                        changed = true;
                    }
                }
            }
        }
    }
    changed
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
