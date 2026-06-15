# Multi-CLI runbook (quick reference)

Full design: [TDD-multi-cli-orchestration-claude-gemini-codex.md](./TDD-multi-cli-orchestration-claude-gemini-codex.md)

## Pick a lane (one per issue)

| Lane | Tool | Typical work |
|------|------|----------------|
| **architect** | Claude Code | Multi-file features, Rust core |
| **docs** | Gemini CLI | TDD, README, ADRs, six-file audit |
| **ffc** | Codex | CI green, fmt, clippy, small patches |

## Workspace slots

Use isolated directories: `claude-code-N`, `gemini-cli-N`, `codex-N`. **One issue per slot per branch.**

## Handoff

When switching lanes, write `HANDOFF.md` (done / not done / verify / constraints).

## MCP

```bash
lornu-mcp-settings sync
```

## Canonical checks (once per PR)

| Repo | Check |
|------|--------|
| oxidizedgraph | `cargo test` |
| oxidizedRAG | `nix flake check -L` (Codex lane) |
