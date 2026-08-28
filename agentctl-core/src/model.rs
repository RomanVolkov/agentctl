use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub global: Global,
    pub skills: HashMap<String, Skill>,
    pub harnesses: Vec<Harness>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub model: Option<String>,
    pub max_spend_limit: Option<f64>,
    pub system_prompt: Option<String>,
    pub skills: Option<Vec<String>>,
    pub guardrails: Guardrails,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Guardrails {
    pub allowed_dirs: Option<Vec<PathBuf>>,
    pub auto_approve: Option<Vec<String>>,
    pub require_approval: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Skill {
    Mcp { command: String, args: Vec<String> },
    SkillDir { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Harness {
    pub provider: Provider,
    pub config_path: PathBuf,
    pub model: Option<String>,
    pub max_spend_limit: Option<f64>,
    pub system_prompt: Option<String>,
    pub skills: Option<Vec<String>>,
    pub guardrails: Guardrails,
    pub extra: Option<toml::Value>,
}

/// A harness with all overrides resolved against `[global]` defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedHarness {
    pub provider: Provider,
    pub config_path: PathBuf,
    pub model: Option<String>,
    pub max_spend_limit: Option<f64>,
    pub system_prompt: Option<String>,
    pub skills: Vec<String>,
    pub guardrails: Guardrails,
    pub extra: Option<toml::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenCode,
    Agy,
    Claude,
    Codex,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::OpenCode => "opencode",
            Provider::Agy => "agy",
            Provider::Claude => "claude",
            Provider::Codex => "codex",
        }
    }
}

// ---- Raw deserialization types (before validation/resolution) ----

#[derive(Debug, Deserialize)]
pub struct RawWorkspace {
    #[serde(default)]
    pub global: Option<RawGlobal>,
    #[serde(default)]
    pub skills: HashMap<String, RawSkill>,
    #[serde(default)]
    pub harnesses: Vec<RawHarness>,
}

#[derive(Debug, Deserialize)]
pub struct RawGlobal {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub max_spend_limit: Option<f64>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    #[serde(default)]
    pub allowed_dirs: Option<Vec<PathBuf>>,
    #[serde(default)]
    pub auto_approve: Option<Vec<String>>,
    #[serde(default)]
    pub require_approval: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct RawSkill {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub struct RawHarness {
    pub provider: String,
    pub config_path: PathBuf,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub max_spend_limit: Option<f64>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub skills: Option<Vec<String>>,
    #[serde(default)]
    pub allowed_dirs: Option<Vec<PathBuf>>,
    #[serde(default)]
    pub auto_approve: Option<Vec<String>>,
    #[serde(default)]
    pub require_approval: Option<Vec<String>>,
    #[serde(default)]
    pub extra: Option<toml::Value>,
}

impl RawWorkspace {
    pub fn into_validated(self) -> Result<Workspace, crate::Error> {
        let global = Global {
            model: self.global.as_ref().and_then(|g| g.model.clone()),
            max_spend_limit: self.global.as_ref().and_then(|g| g.max_spend_limit),
            system_prompt: self.global.as_ref().and_then(|g| g.system_prompt.clone()),
            skills: self.global.as_ref().and_then(|g| g.skills.clone()),
            guardrails: Guardrails {
                allowed_dirs: self.global.as_ref().and_then(|g| g.allowed_dirs.clone()),
                auto_approve: self.global.as_ref().and_then(|g| g.auto_approve.clone()),
                require_approval: self.global.as_ref().and_then(|g| g.require_approval.clone()),
            },
        };

        let mut skills = HashMap::new();
        for (name, raw) in self.skills {
            let skill = raw.into_skill()?;
            skills.insert(name, skill);
        }

        let mut harnesses = Vec::new();
        for raw in self.harnesses {
            harnesses.push(raw.into_harness(&skills)?);
        }

        Ok(Workspace {
            global,
            skills,
            harnesses,
        })
    }
}

impl RawSkill {
    fn into_skill(self) -> Result<Skill, crate::Error> {
        match self.kind.as_str() {
            "mcp" => {
                let command = self
                    .command
                    .ok_or_else(|| crate::Error::MissingField("skills.X.command".into()))?;
                if command.trim().is_empty() {
                    return Err(crate::Error::InvalidMcpCommand);
                }
                Ok(Skill::Mcp {
                    command,
                    args: self.args.unwrap_or_default(),
                })
            }
            "skill" => {
                let path = self
                    .path
                    .ok_or_else(|| crate::Error::MissingField("skills.X.path".into()))?;
                Ok(Skill::SkillDir { path })
            }
            other => Err(crate::Error::UnknownSkillType(other.to_string())),
        }
    }
}

impl RawHarness {
    fn into_harness(
        self,
        skills: &HashMap<String, Skill>,
    ) -> Result<Harness, crate::Error> {
        let provider = match self.provider.as_str() {
            "opencode" => Provider::OpenCode,
            "agy" => Provider::Agy,
            "claude" => Provider::Claude,
            "codex" => Provider::Codex,
            other => return Err(crate::Error::UnknownProvider(other.to_string())),
        };

        if let Some(skill_names) = &self.skills {
            for name in skill_names {
                if !skills.contains_key(name) {
                    return Err(crate::Error::UnknownSkill(name.clone()));
                }
            }
        }

        Ok(Harness {
            provider,
            config_path: self.config_path,
            model: self.model,
            max_spend_limit: self.max_spend_limit,
            system_prompt: self.system_prompt,
            skills: self.skills,
            guardrails: Guardrails {
                allowed_dirs: self.allowed_dirs,
                auto_approve: self.auto_approve,
                require_approval: self.require_approval,
            },
            extra: self.extra,
        })
    }
}

impl Workspace {
    pub fn resolve_harness(&self, harness: &Harness) -> ResolvedHarness {
        ResolvedHarness {
            provider: harness.provider,
            config_path: harness.config_path.clone(),
            model: harness.model.clone().or_else(|| self.global.model.clone()),
            max_spend_limit: harness
                .max_spend_limit
                .or(self.global.max_spend_limit),
            system_prompt: harness
                .system_prompt
                .clone()
                .or_else(|| self.global.system_prompt.clone()),
            skills: harness
                .skills
                .clone()
                .or_else(|| self.global.skills.clone())
                .unwrap_or_default(),
            guardrails: Guardrails {
                allowed_dirs: harness
                    .guardrails
                    .allowed_dirs
                    .clone()
                    .or_else(|| self.global.guardrails.allowed_dirs.clone()),
                auto_approve: harness
                    .guardrails
                    .auto_approve
                    .clone()
                    .or_else(|| self.global.guardrails.auto_approve.clone()),
                require_approval: harness
                    .guardrails
                    .require_approval
                    .clone()
                    .or_else(|| self.global.guardrails.require_approval.clone()),
            },
            extra: harness.extra.clone(),
        }
    }

    pub fn validate_refs(&self) -> Result<(), crate::Error> {
        for name in self.global.skills.iter().flatten() {
            if !self.skills.contains_key(name) {
                return Err(crate::Error::UnknownSkill(name.clone()));
            }
        }
        Ok(())
    }
}