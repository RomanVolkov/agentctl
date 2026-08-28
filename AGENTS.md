# AGENTS.md

## Project

`agentctl` — a Rust CLI that compiles a declarative `workspace.toml` into native configuration directories for AI agent harnesses (OpenCode, Agy, Claude Code, Codex).

## Layout

- `agentctl-core/src/model.rs` — normalized `Workspace` model, `[global]` defaults, per-harness overrides, and `resolve_harness` (global → harness resolution).
- `agentctl-core/src/parse.rs` — TOML parsing entry points and validation.
- `agentctl-core/src/expand.rs` — env var expansion at render time.
- `agentctl-core/src/render/` — `Renderer` trait + one module per harness (`opencode.rs`, `agy.rs`, `claude.rs`, `codex.rs`).
- `agentctl/src/` — CLI: `commands/{validate,preview,apply,common}.rs`, `output.rs`.

## Conventions

- Run `cargo test` and `cargo clippy --all-targets` before finishing; keep both clean.
- Testing approach: code first, then tests. Every new/modified function gets tests covering success and error paths.
- Data model types live in `agentctl-core`; the CLI crate never defines model structs.
- Shared settings are declared in `[global]`; a harness overrides per-field. Resolve once via `Workspace::resolve_harness` — renderers consume `ResolvedHarness`, never raw overrides.
- A new harness = new `Renderer` impl in `agentctl-core/src/render/` + registration in `render::render()` + the `Provider` enum in `model.rs`.
- Renderers add any primitive they cannot express to `RenderedOutput.unrendered` (shown as `// not rendered:` in preview).
- Config paths are directories; renderers return relative file paths plus copied skill trees.
- `apply` backs up the whole harness directory before changes and writes files atomically.