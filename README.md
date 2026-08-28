# agentctl

[![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![GitHub](https://img.shields.io/badge/GitHub-RomanVolkov%2Fagentctl-blue?logo=github)](https://github.com/RomanVolkov/agentctl)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Works with OpenCode](https://img.shields.io/badge/Works%20with-OpenCode-DA7857?logo=anthropic)](https://opencode.com/)
[![Works with Agy](https://img.shields.io/badge/Works%20with-Agy-4285F4?logo=google)](https://gemini.google.com/)
[![Works with Claude Code](https://img.shields.io/badge/Works%20with-Claude%20Code-DA7857?logo=anthropic)](https://claude.ai/code)
[![Works with Codex](https://img.shields.io/badge/Works%20with-Codex-412991?logo=openai)](https://openai.com/codex)

## Overview

`agentctl` is a Rust CLI that manages AI agent harness configurations from a single
declarative `workspace.toml`. It reads one source of truth and compiles it into the
native config directories each harness expects, so the same setup works across tools
and machines.

The tool follows a Terraform-like workflow: `validate` checks the workspace, `preview`
shows diffs before anything is written, and `apply` writes changes after confirmation.

It targets OpenCode, Agy (Antigravity CLI), Claude Code, and Codex. Each harness is a
single renderer module, so adding or adjusting support stays isolated.

## What it manages

- **Model** — a default model at `[global]`, overridable per harness (each harness has its own model family).
- **Context** — `system_prompt`, applied where the native config supports instructions.
- **Skills** — local skill directories (`SKILL.md` plus scripts) copied into each harness's `skills/` folder.
- **Guardrails** — `allowed_dirs`, `auto_approve`, `require_approval` declared once; mapped to each
  harness's permission schema (Agy `command(...)`, Claude `Bash(...)`, Codex `approval_policy`).
- **Harness-specific flags** — kept under each harness's `extra`, passed through to the native config.

Two rules keep the setup unified:

- Settings are declared once at `[global]` and overridden per harness where they differ.
  A harness omits what it inherits. Set `skills = []` to override a default with nothing.
- Values support `$KEY` / `${KEY}` environment expansion at render time, so secrets stay out of the file.

## Getting started

### Prerequisites

- Rust 1.85 or newer, or just the released `agentctl` binary.

### Install

Build from source:

```bash
cargo install --path agentctl
```

### Create a workspace

A `workspace.toml` lives at the root of a folder that represents the configuration (commonly the project itself):

```toml
[global]
model = "claude-sonnet-4"
max_spend_limit = 5.0
system_prompt = "You are a senior software engineer."
skills = ["plan_make"]
allowed_dirs = ["./src", "./tests"]
auto_approve = ["git status", "ls", "cargo test"]
require_approval = ["git push", "npm install"]

[skills.plan_make]
type = "skill"
path = "skills/plan-make"

[[harnesses]]
provider = "opencode"
config_path = "~/.dotfiles/opencode"
extra = { instructions = ["{env:HOME}/.benjamin-plus/injected-instruction.md"] }

[[harnesses]]
provider = "agy"
model = "Gemini 3.7 Flash (Medium)"   # overrides the global model
config_path = "~/.gemini/antigravity-cli"
extra = { allowNonWorkspaceAccess = true, enableTelemetry = false }
```

Skill `path` values may be relative to the workspace file or absolute. `config_path`
points to a harness directory, not a single file, and supports `~` and `$HOME`.

### Validate

```bash
agentctl validate
```

Parses the workspace and checks that skill references resolve and providers are known. Useful in CI.

### Preview

```bash
agentctl preview
```

Renders every harness and prints colored unified diffs for each generated file, plus a
summary of skill directory changes. Nothing is written.

### Apply

```bash
agentctl apply          # shows diffs, then prompts
agentctl apply --yes    # writes without prompting
```

Before any change, `apply` copies the harness directory into a timestamped backup
(`<dir>.<nanos>.bak`). Files are written atomically (temp file plus rename).

All commands accept `-c <path>` to point at a workspace file; the default is `./workspace.toml`.

## Repository structure

```
agentctl/
├── agentctl-core/               # library: model, parsing, rendering
│   └── src/
│       ├── model.rs             # workspace data model and validation
│       ├── parse.rs             # TOML parsing entry points
│       ├── expand.rs            # env var expansion
│       └── render/              # Renderer trait + per-harness impls
│           ├── opencode.rs      # OpenCode renderer
│           ├── agy.rs           # Agy renderer
│           ├── claude.rs        # Claude Code renderer
│           └── codex.rs         # Codex renderer
├── agentctl/                    # CLI binary
│   └── src/
│       ├── cli.rs               # clap-based CLI definition
│       ├── commands/            # validate, preview, apply, common
│       └── output.rs            # colored unified diffs
├── config/                      # example workspace for this repo
│   ├── workspace.toml
│   └── skills/                  # local skill copies
└── Cargo.toml                   # Cargo workspace
```

## Supported harnesses

| Provider  | Rendered files           | Notes                                |
|-----------|--------------------------|--------------------------------------|
| `opencode`| `opencode.jsonc`, `system_prompt.md`, `skills/` | Provider config managed manually for now |
| `agy`     | `settings.json`, `skills/` | `system_prompt`/`require_approval` not emitted |
| `claude`  | `settings.json`, `skills/` | Guardrails map to `permissions.allow`/`ask` |
| `codex`   | `config.toml`, `skills/` | Guardrails approximated by `approval_policy`/`sandbox_mode` |

Primitives a harness cannot express are reported as `// not rendered: ...` notices during `preview`.

## Reporting issues

Found a bug, typo, or have a suggestion? Open an issue at
[github.com/RomanVolkov/agentctl/issues](https://github.com/RomanVolkov/agentctl/issues).

Please include a description of the problem, steps to reproduce, and expected versus actual behavior.

## License

Licensed under the [MIT License](https://opensource.org/licenses/MIT).

## Acknowledgements

Built with the Rust ecosystem:

- [clap](https://github.com/clap-rs/clap) — CLI argument parsing
- [serde](https://serde.rs/) — serialization
- [toml](https://github.com/toml-rs/toml) — TOML parsing
- [similar](https://github.com/mitsuhiko/similar) — text diffs
- [colored](https://github.com/mackwic/colored) — terminal output

## Contact

**Roman Volkov**

- GitHub: [@RomanVolkov](https://github.com/RomanVolkov)
- Repository: [RomanVolkov/agentctl](https://github.com/RomanVolkov/agentctl)

Questions, suggestions, or collaboration? Open an issue on GitHub.