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
}
