use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid TOML: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("missing required field: {0}")]
    MissingField(String),

    #[error("unknown skill type: {0}")]
    UnknownSkillType(String),

    #[error("unknown provider: {0}")]
    UnknownProvider(String),

    #[error("unknown skill reference: {0}")]
    UnknownSkill(String),

    #[error("MCP skill requires a non-empty command")]
    InvalidMcpCommand,

    #[error("could not read {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("render failed for {harness}: {message}")]
    Render {
        harness: String,
        message: String,
    },
}
