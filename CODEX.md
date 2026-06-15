# CODEX.md

Operating notes for AI/code agents working in this repository.

## Scope

- Applies to the full repository.
- Prefer minimal, focused changes per PR.

## Branch and PR

- Base branch: `develop`.
- Create feature branches from `develop`.
- Keep PRs scoped to one change theme.

## Before Editing

- Read: `README.md`, `CONTRIBUTING.md`.
- For behavior changes, identify the owning module first:
  - Graph/runtime: `src/graph.rs`, `src/runner.rs`
  - Nodes: `src/nodes/*.rs`
  - Checkpointing: `src/checkpoint/*.rs`
  - Events/streaming: `src/events/*.rs`
  - Orchestration: `src/orchestration/*.rs`
  - Server API: `src/bin/server.rs`

## Coding Standards

- Rust 2021 idioms, clear error paths, no silent fallbacks for core flows.
- Preserve existing API behavior unless intentionally changing it.
- Avoid unrelated refactors.

## Testing and Validation

- Always run:

```bash
cargo test
```

- For broader verification when feasible:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features
```

## PR Checklist

- Describe the problem and approach.
- Include test evidence.
- Call out behavioral or compatibility changes explicitly.
- Reference impacted files/modules.

## Notes

- If local environment auth/tooling differs from CI, prefer deterministic checks (`cargo test`) and document any gaps.
