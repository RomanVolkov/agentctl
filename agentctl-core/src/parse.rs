use std::path::Path;

use crate::error::Error;
use crate::model::{RawWorkspace, Workspace};

pub fn parse_workspace_from_str(input: &str) -> Result<Workspace, Error> {
    let raw: RawWorkspace = toml::from_str(input)?;
    let ws = raw.into_validated()?;
    ws.validate_refs()?;
    Ok(ws)
}

pub fn parse_workspace_file(path: &Path) -> Result<Workspace, Error> {
    let content = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_workspace_from_str(&content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Skill;
    use crate::Provider;

    fn valid_toml() -> &'static str {
        r#"
[global]
model = "default-model"
skills = ["plan_make", "git_mcp"]
auto_approve = ["ls"]
require_approval = ["git push"]

[skills.plan_make]
type = "skill"
path = "skills/plan-make"

[skills.git_mcp]
type = "mcp"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[[harnesses]]
provider = "opencode"
model = "claude-sonnet-4"
skills = ["plan_make"]
config_path = "~/.dotfiles/opencode"

[[harnesses]]
provider = "agy"
config_path = "~/.gemini/antigravity-cli"
extra = { allowNonWorkspaceAccess = true }
"#
    }

    #[test]
    fn parses_valid_workspace() {
        let ws = parse_workspace_from_str(valid_toml()).unwrap();
        assert_eq!(ws.skills.len(), 2);
        assert_eq!(ws.harnesses.len(), 2);
        assert_eq!(ws.global.model.as_deref(), Some("default-model"));
        assert_eq!(ws.harnesses[0].model.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(ws.harnesses[1].model, None);
        assert_eq!(
            ws.global.guardrails.auto_approve.as_deref(),
            Some(&["ls".to_string()][..])
        );

        match &ws.skills["plan_make"] {
            Skill::SkillDir { path } => assert_eq!(path, std::path::Path::new("skills/plan-make")),
            other => panic!("expected SkillDir, got {other:?}"),
        }
        assert_eq!(ws.harnesses[0].provider, Provider::OpenCode);
        assert_eq!(ws.harnesses[1].provider, Provider::Agy);
    }

    #[test]
    fn parses_all_providers() {
        for (name, expected) in [
            ("opencode", Provider::OpenCode),
            ("agy", Provider::Agy),
            ("claude", Provider::Claude),
            ("codex", Provider::Codex),
        ] {
            let toml = format!(
                "[[harnesses]]\nprovider = \"{name}\"\nconfig_path = \"/tmp/x\"\n"
            );
            let ws = parse_workspace_from_str(&toml).unwrap();
            assert_eq!(ws.harnesses[0].provider, expected);
        }
    }

    #[test]
    fn workspace_without_global_parses() {
        let toml = r#"
[[harnesses]]
provider = "agy"
config_path = "~/.gemini"
model = "Gemini 3.7 Flash (Medium)"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        assert_eq!(ws.global.max_spend_limit, None);
        assert_eq!(ws.global.skills, None);
        assert_eq!(ws.harnesses.len(), 1);
    }

    #[test]
    fn rejects_unknown_provider() {
        let toml = r#"
[[harnesses]]
provider = "bogus"
config_path = "~/.x"
"#;
        let err = parse_workspace_from_str(toml).unwrap_err();
        assert!(matches!(err, Error::UnknownProvider(_)));
    }

    #[test]
    fn rejects_unknown_global_skill_reference() {
        let toml = r#"
[global]
skills = ["does-not-exist"]

[[harnesses]]
provider = "agy"
config_path = "~/.gemini"
"#;
        let err = parse_workspace_from_str(toml).unwrap_err();
        assert!(matches!(err, Error::UnknownSkill(_)));
    }

    #[test]
    fn rejects_unknown_harness_skill_reference() {
        let toml = r#"
[[harnesses]]
provider = "agy"
skills = ["does-not-exist"]
config_path = "~/.gemini"
"#;
        let err = parse_workspace_from_str(toml).unwrap_err();
        assert!(matches!(err, Error::UnknownSkill(_)));
    }

    #[test]
    fn rejects_unknown_skill_type() {
        let toml = r#"
[skills.bad]
type = "nonsense"

[[harnesses]]
provider = "agy"
skills = ["bad"]
config_path = "~/.gemini"
"#;
        let err = parse_workspace_from_str(toml).unwrap_err();
        assert!(matches!(err, Error::UnknownSkillType(_)));
    }

    #[test]
    fn rejects_empty_mcp_command() {
        let toml = r#"
[skills.bad]
type = "mcp"
command = "  "

[[harnesses]]
provider = "agy"
skills = ["bad"]
config_path = "~/.gemini"
"#;
        let err = parse_workspace_from_str(toml).unwrap_err();
        assert!(matches!(err, Error::InvalidMcpCommand));
    }

    #[test]
    fn rejects_invalid_toml() {
        let err = parse_workspace_from_str("not [valid toml").unwrap_err();
        assert!(matches!(err, Error::Parse(_)));
    }

    #[test]
    fn reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.toml");
        std::fs::write(&path, valid_toml()).unwrap();
        let ws = parse_workspace_file(&path).unwrap();
        assert_eq!(ws.harnesses.len(), 2);
    }

    #[test]
    fn missing_file_is_io_error() {
        let err =
            parse_workspace_file(std::path::Path::new("/nonexistent/workspace.toml")).unwrap_err();
        assert!(matches!(err, Error::Io { .. }));
    }

    #[test]
    fn global_defaults_fill_harness_gaps() {
        let toml = r#"
[global]
model = "default"
max_spend_limit = 5.0
system_prompt = "be good"
skills = ["a", "b"]
auto_approve = ["ls"]
require_approval = ["git push"]

[skills.a]
type = "skill"
path = "skills/a"

[skills.b]
type = "skill"
path = "skills/b"

[[harnesses]]
provider = "agy"
config_path = "~/.gemini"
model = "override"
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let r = ws.resolve_harness(&ws.harnesses[0]);
        assert_eq!(r.model.as_deref(), Some("override"));
        assert_eq!(r.max_spend_limit, Some(5.0));
        assert_eq!(r.system_prompt.as_deref(), Some("be good"));
        assert_eq!(r.skills, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(r.guardrails.auto_approve, Some(vec!["ls".to_string()]));
        assert_eq!(r.guardrails.require_approval, Some(vec!["git push".to_string()]));
    }

    #[test]
    fn empty_list_overrides_inherit() {
        let toml = r#"
[global]
skills = ["a", "b"]
auto_approve = ["ls"]
require_approval = ["git push"]

[skills.a]
type = "skill"
path = "skills/a"

[skills.b]
type = "skill"
path = "skills/b"

[[harnesses]]
provider = "agy"
config_path = "~/.gemini"
skills = []
auto_approve = ["git status"]
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let r = ws.resolve_harness(&ws.harnesses[0]);
        assert!(r.skills.is_empty(), "skills=[] must override global to empty");
        assert_eq!(r.guardrails.auto_approve, Some(vec!["git status".to_string()]));
        assert_eq!(r.guardrails.require_approval, Some(vec!["git push".to_string()]));
    }

    #[test]
    fn field_level_guardrail_merge() {
        let toml = r#"
[global]
auto_approve = ["ls"]

[[harnesses]]
provider = "agy"
config_path = "~/.gemini"
require_approval = ["sudo"]
"#;
        let ws = parse_workspace_from_str(toml).unwrap();
        let r = ws.resolve_harness(&ws.harnesses[0]);
        assert_eq!(r.guardrails.auto_approve, Some(vec!["ls".to_string()]));
        assert_eq!(r.guardrails.require_approval, Some(vec!["sudo".to_string()]));
    }
}
