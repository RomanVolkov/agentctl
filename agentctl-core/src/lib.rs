pub mod error;
pub mod expand;
pub mod model;
pub mod parse;
pub mod render;

pub use error::Error;
pub use expand::{expand_env, expand_workspace};
pub use model::{Guardrails, Harness, Provider, ResolvedHarness, Skill, Workspace};
pub use parse::parse_workspace_file;
pub use parse::parse_workspace_from_str;
pub use render::{render, RenderedFile, RenderedOutput, Renderer};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver() {
        assert!(!version().is_empty());
        assert!(version().split('.').count() >= 2);
    }
}
