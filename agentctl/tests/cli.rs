use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_agentctl"))
}

fn write(dir: &std::path::Path, rel: &str, content: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

fn workspace_toml(harness_dir: &str, skill_src: &str) -> String {
    format!(
        r#"
[global]
model = "claude-sonnet-4"
skills = ["plan_make"]
auto_approve = ["git status", "ls"]
require_approval = ["git push"]

[skills.plan_make]
type = "skill"
path = "{skill_src}"

[[harnesses]]
provider = "opencode"
config_path = "{harness_dir}"

[[harnesses]]
provider = "agy"
model = "Gemini 3.7 Flash (Medium)"
config_path = "{harness_dir}"
extra = {{ allowNonWorkspaceAccess = true }}
"#
    )
}

fn setup() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let skill_src = dir.path().join("skills/plan-make");
    write(dir.path(), "workspace.toml", &workspace_toml(&skill_src.display().to_string(), &skill_src.display().to_string()));
    write(dir.path(), "skills/plan-make/SKILL.md", "# plan-make skill\n");
    write(dir.path(), "skills/plan-make/scripts/run.sh", "#!/bin/sh\n");
    (dir, skill_src)
}

#[test]
fn validate_ok() {
    let (dir, _) = setup();
    let out = Command::new(bin())
        .arg("validate")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "validate failed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("OK"));
}

#[test]
fn validate_reports_error() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "workspace.toml",
        "[[harnesses]]\nprovider = \"nope\"\nconfig_path = \"x\"\n",
    );
    let out = Command::new(bin())
        .arg("validate")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("error"));
}

#[test]
fn apply_writes_configs_and_skills() {
    let (dir, skill_src) = setup();
    let harness_dir = dir.path().join("harness");
    write(dir.path(), "workspace.toml", &workspace_toml(&harness_dir.display().to_string(), &skill_src.display().to_string()));

    let out = Command::new(bin())
        .arg("apply")
        .arg("--yes")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "apply failed: {}", String::from_utf8_lossy(&out.stderr));

    assert!(harness_dir.join("opencode.jsonc").exists());
    assert!(harness_dir.join("settings.json").exists());
    assert!(harness_dir.join("skills/plan-make/SKILL.md").exists());
    assert!(harness_dir.join("skills/plan-make/scripts/run.sh").exists());

    let settings = std::fs::read_to_string(harness_dir.join("settings.json")).unwrap();
    assert!(settings.contains("command(git status)"));
    assert!(settings.contains("\"allowNonWorkspaceAccess\": true"));
}

#[test]
fn apply_writes_all_four_harnesses() {
    let dir = tempfile::tempdir().unwrap();
    let skill_src = dir.path().join("skills/plan-make");
    write(dir.path(), "skills/plan-make/SKILL.md", "# skill\n");
    write(
        dir.path(),
        "workspace.toml",
        &format!(
            r#"
[global]
model = "default-model"
skills = ["plan_make"]
auto_approve = ["git status"]
require_approval = ["git push"]

[skills.plan_make]
type = "skill"
path = "{}"

[[harnesses]]
provider = "opencode"
config_path = "{}"

[[harnesses]]
provider = "agy"
config_path = "{}"

[[harnesses]]
provider = "claude"
config_path = "{}"

[[harnesses]]
provider = "codex"
config_path = "{}"
"#,
            skill_src.display(),
            dir.path().join("opc").display(),
            dir.path().join("agy").display(),
            dir.path().join("claude").display(),
            dir.path().join("codex").display(),
        ),
    );

    let out = Command::new(bin())
        .arg("apply")
        .arg("--yes")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "apply failed: {}", String::from_utf8_lossy(&out.stderr));

    assert!(dir.path().join("opc/opencode.jsonc").exists());
    assert!(dir.path().join("agy/settings.json").exists());
    assert!(dir.path().join("claude/settings.json").exists());
    assert!(dir.path().join("codex/config.toml").exists());

    let agy = std::fs::read_to_string(dir.path().join("agy/settings.json")).unwrap();
    assert!(agy.contains("\"model\": \"default-model\""));
    let claude = std::fs::read_to_string(dir.path().join("claude/settings.json")).unwrap();
    assert!(claude.contains("Bash(git status)"));
    assert!(claude.contains("Bash(git push)"));
    let codex = std::fs::read_to_string(dir.path().join("codex/config.toml")).unwrap();
    assert!(codex.contains("model = \"default-model\""));
    assert!(codex.contains("approval_policy = \"on-request\""));
    // skills copied into every harness
    for p in ["opc", "agy", "claude", "codex"] {
        assert!(dir.path().join(p).join("skills/plan-make/SKILL.md").exists());
    }
}

#[test]
fn apply_without_yes_and_no_tty_is_aborted() {
    let (dir, skill_src) = setup();
    let harness_dir = dir.path().join("harness");
    write(dir.path(), "workspace.toml", &workspace_toml(&harness_dir.display().to_string(), &skill_src.display().to_string()));

    let out = Command::new(bin())
        .arg("apply")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!harness_dir.join("opencode.jsonc").exists());
}

#[test]
fn apply_creates_backup_of_existing_dir() {
    let (dir, skill_src) = setup();
    let harness_dir = dir.path().join("harness");
    std::fs::create_dir_all(&harness_dir).unwrap();
    std::fs::write(harness_dir.join("settings.json"), "{\"old\": true}\n").unwrap();
    write(dir.path(), "workspace.toml", &workspace_toml(&harness_dir.display().to_string(), &skill_src.display().to_string()));

    let out = Command::new(bin())
        .arg("apply")
        .arg("--yes")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "apply failed: {}", String::from_utf8_lossy(&out.stderr));

    let backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("harness.") && n.ends_with(".bak"))
        .collect();
    assert!(!backups.is_empty(), "expected at least one backup");

    // Every backup must contain the pre-apply file content.
    for backup in backups {
        assert!(dir.path().join(&backup).join("settings.json").exists());
        let old = std::fs::read_to_string(dir.path().join(&backup).join("settings.json")).unwrap();
        assert_eq!(old, "{\"old\": true}\n");
    }
}

#[test]
fn env_vars_are_expanded_in_mcp_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let harness_dir = dir.path().join("harness");
    std::env::set_var("AGENTCTL_TEST_API_KEY", "secret-123");
    write(
        dir.path(),
        "workspace.toml",
        &format!(
            r#"
[skills.git]
type = "mcp"
command = "env"
args = ["GITHUB_TOKEN=${{AGENTCTL_TEST_API_KEY}}", "npx", "-y", "server"]
env = {{ API_TOKEN = "${{AGENTCTL_TEST_API_KEY}}" }}

[[harnesses]]
provider = "agy"
skills = ["git"]
config_path = "{}"
"#,
            harness_dir.display()
        ),
    );

    let out = Command::new(bin())
        .arg("apply")
        .arg("--yes")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "apply failed: {}", String::from_utf8_lossy(&out.stderr));

    let mcp_cfg = std::fs::read_to_string(harness_dir.join("mcp_config.json")).unwrap();
    assert!(mcp_cfg.contains("GITHUB_TOKEN=secret-123"));
    assert!(mcp_cfg.contains("\"API_TOKEN\": \"secret-123\""));
}

#[test]
fn preserves_existing_unmanaged_mcps_and_merges_declarative_mcps() {
    let dir = tempfile::tempdir().unwrap();
    let opc_dir = dir.path().join("opencode");
    let agy_dir = dir.path().join("agy");
    let claude_dir = dir.path().join(".claude");
    let codex_dir = dir.path().join("codex");

    // Pre-populate unmanaged work MCPs on disk across all 4 harnesses
    write(
        &opc_dir,
        "opencode.jsonc",
        r#"{
  "mcp": {
    "work-jira": {
      "type": "local",
      "command": ["npx", "jira-mcp"]
    }
  },
  "provider": {
    "custom": { "name": "corp" }
  }
}
"#,
    );
    write(
        &agy_dir,
        "mcp_config.json",
        r#"{
  "mcpServers": {
    "work-jira": {
      "command": "npx",
      "args": ["jira-mcp"]
    }
  }
}
"#,
    );
    write(
        &claude_dir,
        "settings.json",
        r#"{
  "enabledMcpjsonServers": ["work-jira"],
  "permissions": {
    "allow": ["mcp__work_jira__search"]
  }
}
"#,
    );
    write(
        dir.path(),
        ".claude.json",
        r#"{
  "userID": "user-123",
  "mcpServers": {
    "work-jira": {
      "type": "stdio",
      "command": "npx",
      "args": ["jira-mcp"]
    }
  }
}
"#,
    );
    write(
        &codex_dir,
        "config.toml",
        r#"[mcp_servers.work-jira]
command = "npx"
args = ["jira-mcp"]
"#,
    );

    write(
        dir.path(),
        "workspace.toml",
        &format!(
            r#"
[global]
model = "claude-sonnet-4"
mcp = ["github", "sentry"]
auto_approve = ["git status"]

[mcp.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp.sentry]
url = "https://mcp.sentry.dev/mcp"

[[harnesses]]
provider = "opencode"
config_path = "{}"

[[harnesses]]
provider = "agy"
config_path = "{}"

[[harnesses]]
provider = "claude"
config_path = "{}"

[[harnesses]]
provider = "codex"
config_path = "{}"
"#,
            opc_dir.display(),
            agy_dir.display(),
            claude_dir.display(),
            codex_dir.display(),
        ),
    );

    let out = Command::new(bin())
        .arg("apply")
        .arg("--yes")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "apply failed: {}", String::from_utf8_lossy(&out.stderr));

    // OpenCode: has both declarative (github, sentry) and existing (work-jira) + provider
    let opc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(opc_dir.join("opencode.jsonc")).unwrap())
            .unwrap();
    assert_eq!(opc["mcp"]["work-jira"]["command"], serde_json::json!(["npx", "jira-mcp"]));
    assert_eq!(
        opc["mcp"]["github"]["command"],
        serde_json::json!(["npx", "-y", "@modelcontextprotocol/server-github"])
    );
    assert_eq!(opc["mcp"]["sentry"]["type"], serde_json::json!("remote"));
    assert_eq!(opc["provider"]["custom"]["name"], serde_json::json!("corp"));

    // Agy: has both declarative and existing in mcp_config.json
    let agy_mcp: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(agy_dir.join("mcp_config.json")).unwrap())
            .unwrap();
    assert!(agy_mcp["mcpServers"]["work-jira"].is_object());
    assert!(agy_mcp["mcpServers"]["github"].is_object());
    assert_eq!(
        agy_mcp["mcpServers"]["sentry"]["url"],
        serde_json::json!("https://mcp.sentry.dev/mcp")
    );

    // Claude: settings.json preserves enabledMcpjsonServers and mcp__work_jira__search permission,
    // and .claude.json preserves userID + work-jira while adding github and sentry
    let claude_settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(claude_dir.join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        claude_settings["enabledMcpjsonServers"],
        serde_json::json!(["work-jira"])
    );
    let allow = claude_settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.contains(&serde_json::json!("Bash(git status)")));
    assert!(allow.contains(&serde_json::json!("mcp__work_jira__search")));

    let claude_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join(".claude.json")).unwrap())
            .unwrap();
    assert_eq!(claude_json["userID"], serde_json::json!("user-123"));
    assert!(claude_json["mcpServers"]["work-jira"].is_object());
    assert_eq!(claude_json["mcpServers"]["github"]["type"], serde_json::json!("stdio"));
    assert_eq!(claude_json["mcpServers"]["sentry"]["type"], serde_json::json!("http"));

    // Codex: config.toml has both work-jira and github/sentry
    let codex_cfg = std::fs::read_to_string(codex_dir.join("config.toml")).unwrap();
    assert!(codex_cfg.contains("[mcp_servers.work-jira]"));
    assert!(codex_cfg.contains("[mcp_servers.github]"));
    assert!(codex_cfg.contains("[mcp_servers.sentry]"));
    assert!(codex_cfg.contains("url = \"https://mcp.sentry.dev/mcp\""));
}

#[test]
fn apply_prunes_stale_skills_from_harness() {
    let (dir, skill_src) = setup();
    let harness_dir = dir.path().join("harness");
    let stale_skill = harness_dir.join("skills/obsolete-skill");
    std::fs::create_dir_all(&stale_skill).unwrap();
    std::fs::write(stale_skill.join("SKILL.md"), "# old\n").unwrap();
    write(
        dir.path(),
        "workspace.toml",
        &workspace_toml(&harness_dir.display().to_string(), &skill_src.display().to_string()),
    );

    let out = Command::new(bin())
        .arg("apply")
        .arg("--yes")
        .arg("-c")
        .arg(dir.path().join("workspace.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "apply failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!stale_skill.exists(), "stale skill directory should be pruned");
    assert!(harness_dir.join("skills/plan-make/SKILL.md").exists());
}
