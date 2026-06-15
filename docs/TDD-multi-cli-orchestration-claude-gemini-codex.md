# Technical Design Document: Efficient Multi-CLI Orchestration (Claude Code, Gemini CLI, Codex)

| Field | Value |
|-------|-------|
| **Status** | Draft — for research group review |
| **Authors** | Stevedores engineering (synthesized from platform review, June 2026) |
| **Audience** | Research group, platform engineers, agent operators |
| **Repositories** | [oxidizedgraph](https://github.com/stevedores-org/oxidizedgraph), [oxidizedRAG](https://github.com/stevedores-org/oxidizedRAG), [lornu-ai/bullpen](https://github.com/lornu-ai/bullpen) (agent roster) |
| **Related** | [TDD: AI inference KV & RAG vectors](https://github.com/stevedores-org/oxidizedRAG/blob/develop/docs/TDD-ai-inference-storage-and-rag-vectors.md), oxidizedgraph [#18](https://github.com/stevedores-org/oxidizedgraph/issues/18), `lornu-mcp-settings` |

---

## 1. Executive summary

Teams often run **Claude Code**, **Gemini CLI**, and **OpenAI Codex** interchangeably on the same repository, which wastes tokens, creates config drift, and causes merge collisions. This document defines an **efficiency-first operating model**: assign each CLI a **primary lane**, isolate work in **numbered workspaces**, unify **MCP and agent context files**, and hand off via **thin artifacts** (issue, branch, `HANDOFF.md`).

**Recommended lane assignment:**

| Lane | Primary CLI | Secondary | Avoid |
|------|-------------|-----------|-------|
| **Architect / refactor** | Claude Code | — | Codex for 50+ file refactors |
| **Docs / ADRs / six-file audit** | Gemini CLI (`scribe` patterns) | Claude Code | Codex |
| **Surgical fix / CI / fmt** | Codex | Claude Code | Gemini for one-line Rust clippy |
| **Parallel feature streams** | One CLI **per worktree slot** | — | Two CLIs on same branch |
| **Rust graph / orchestration** | Claude Code or Cursor | Codex for tests only | Unfamiliar tool for `oxidizedgraph` core |

**Platform glue:** sync MCP via **`lornu-mcp-settings`**; keep **`CLAUDE.md` / `GEMINI.md` / `CODEX.md` / `AGENTS.md`** aligned (oxidizedgraph **7-file rule**; bullpen **6-file rule**).

---

## 2. Problem statement

### 2.1 Symptoms of inefficient multi-CLI use

| Symptom | Cost |
|---------|------|
| Same task run in Claude, then Gemini, then Codex | 3× tokens, divergent diffs |
| Divergent MCP configs per tool | Broken tools, stale paths |
| Multiple agents on one git branch | Conflict storms, lost context |
| No explicit handoff | Re-discovery of repo layout every session |
| Wrong tool for job (Codex on architecture) | Rework, shallow plans |

### 2.2 Goals

1. **Minimize total tokens and wall-clock** per merged PR
2. **Maximize parallel throughput** without branch collisions
3. **Deterministic config** across CLIs (MCP, rules, test commands)
4. **Traceable handoffs** compatible with AIVCS / graph orchestration (oxidizedgraph)
5. **Research-group visibility** into when to add a fourth surface (Cursor IDE, Copilot)

### 2.3 Non-goals

- Replacing human review or security sign-off
- Mandating a single vendor model for all tasks
- Automating CLI selection without human override (Phase 1)

---

## 3. Tool profiles (when to use which)

### 3.1 Claude Code

| Dimension | Assessment |
|-----------|------------|
| **Strengths** | Multi-file reasoning, sustained refactors, hook/skill ecosystem, project `.mcp.json`, strong instruction-following for `CLAUDE.md` |
| **Weaknesses** | Higher cost per long session; temptation to over-edit |
| **Best tasks** | Feature design implementation, `oxidizedgraph` graph/nodes/checkpoint, cross-crate Rust changes, PR split planning |
| **Config** | Project `.mcp.json`; `.claude/settings.local.json` per workspace slot |
| **Default validation** | `cargo test`, `cargo clippy` (per `CLAUDE.md`) |

### 3.2 Gemini CLI

| Dimension | Assessment |
|-----------|------------|
| **Strengths** | Large context for doc ingestion, fast iteration on TS/Bun agents (e.g. bullpen `scribe`), `settings.json` MCP at user or project scope |
| **Weaknesses** | Less predictable for deep Rust trait-heavy work unless explicitly scoped |
| **Best tasks** | README/ADR/TDD drafts, six-file audits, research summaries, TS tooling, multimodal doc review |
| **Config** | `~/.gemini/settings.json` or `.gemini/settings.json` (project) |
| **Default validation** | `bun test`, markdown lint, link checks |

### 3.3 OpenAI Codex (CLI)

| Dimension | Assessment |
|-----------|------------|
| **Strengths** | Fast targeted patches, CI/log-driven fixes, strong with OpenAI docs MCP, TOML config `~/.codex/config.toml` |
| **Weaknesses** | Narrower project rules unless `CODEX.md` is kept strict; not ideal for week-long refactors |
| **Best tasks** | `ffc` (fix failing checks), fmt/clippy deltas, small API additions with tests, config sync scripts |
| **Config** | `~/.codex/config.toml` `[mcp_servers.*]` |
| **Default validation** | Commands listed in `CODEX.md` (minimal scope) |

### 3.4 Cursor IDE (fourth surface)

Cursor subsumes **Composer / Agent** with unified rules (`.cursorrules`). Treat Cursor as **Claude lane superset** when the user is in-IDE; do not run Claude Code CLI on the same branch simultaneously.

---

## 4. Efficiency model: lanes, slots, and handoffs

### 4.1 Lane = type of work (§3)

Pick **one primary CLI per task** at issue creation. Record in issue template:

```markdown
## CLI lane
- [ ] architect (Claude Code)
- [ ] docs/research (Gemini CLI)
- [ ] surgical/CI (Codex)
```

### 4.2 Slot = isolated filesystem (numbered workspaces)

Observed layout on operator machines:

```text
engineering/code/
├── claude-code-{0..N}/   # one git clone or worktree per slot
├── gemini-cli-{0..M}/
├── codex-{0..K}/
└── cursor-ide-0/         # IDE-anchored repo (e.g. oxidizedgraph)
```

**Rules:**

| Rule | Rationale |
|------|-----------|
| **One active issue per slot** | Prevents cross-task context bleed |
| **Slot owns branch** `feat/<issue>-<slot>` | Clean PR from known cwd |
| **No two CLIs commit to same branch same day** | Avoid merge races |
| Prefer **git worktree** over duplicate full clones when disk-constrained | Same object DB |

### 4.3 Handoff artifact (required on lane switch)

When work passes Claude → Codex (e.g. after feature, fix CI), create **`HANDOFF.md`** at repo root (gitignored or committed briefly):

```markdown
# Handoff: ISSUE-123

## Done
- Implemented QualityGate routing in src/guardrails/gate.rs

## Not done
- nix flake check clippy on deprecated modules

## Verify
cargo test -p graphrag-core
nix flake check -L

## Constraints
- Do not refactor runner loop (out of scope)
```

Codex sessions should **start** by reading `HANDOFF.md` + issue URL — not re-scanning the repo.

### 4.4 Branch and PR conventions

| Element | Pattern |
|---------|---------|
| Branch | `feat/<issue#>-<short>-<cli>` e.g. `feat/185-fmt-gemini` |
| PR title | `[codex] fix: nix fmt multimodal` / `[claude] feat: traced runner` |
| Commit scope | One theme; `CODEX.md` minimal-diff discipline for Codex |

---

## 5. Unified configuration (MCP & agent files)

### 5.1 MCP sync — `lornu-mcp-settings`

Canonical MCP definitions should live in **one JSON source** and sync to all CLIs:

| Target | Path | Format |
|--------|------|--------|
| Codex | `~/.codex/config.toml` | `[mcp_servers.<name>]` |
| Gemini | `~/.gemini/settings.json` | `mcpServers` |
| Claude Code | `./.mcp.json` | project `mcpServers` |

**Operator command:**

```bash
lornu-mcp-settings sync
lornu-mcp-settings sync --dry-run
lornu-mcp-settings sync --target codex
```

Run sync **after** adding/removing MCP servers; include in onboarding checklist.

### 5.2 Agent context files (7-file / 6-file rule)

Keep these **semantically aligned** (not necessarily byte-identical):

| File | Claude | Gemini | Codex | Cursor |
|------|--------|--------|-------|--------|
| Build/test commands | `CLAUDE.md` | `GEMINI.md` | `CODEX.md` | `.cursorrules` |
| Capabilities | `AGENTS.md` | `GEMINI.md` § | — | `AGENTS.md` |
| PR discipline | `CODEX.md` | — | `CODEX.md` | rules |

**Efficiency tip:** `CODEX.md` should be the **shortest** (commands + scope only). `CLAUDE.md` carries architecture. `GEMINI.md` carries safety/mandates for doc-heavy work.

### 5.3 Validation matrix (run once per PR, not per CLI)

| Stack | Canonical check | Who runs |
|-------|-----------------|----------|
| oxidizedgraph | `cargo test` | Lane owner |
| oxidizedgraph | `cargo clippy --all-targets` | Codex / CI |
| oxidizedRAG | `nix flake check -L` | Codex / CI |
| bullpen TS | `bun test` | Gemini / scribe |

**Do not** run full `nix flake check` in Claude *and* Gemini *and* Codex — assign to **Codex lane** after feature complete.

---

## 6. Task routing matrix (decision table)

| Task type | Primary CLI | Notes |
|-----------|-------------|-------|
| New Rust module / trait design | Claude Code | TDD-first per `GEMINI.md` mandates |
| Fix PR CI (`ffc`) | Codex | Read logs; minimal diff |
| TDD / research doc | Gemini CLI | Then PR via human or Claude for code |
| README + six-file audit | Gemini CLI | scribe role in bullpen |
| MCP config change | Codex + `lornu-mcp-settings` | Sync all targets |
| Exploratory spike (throwaway) | Gemini CLI (large paste) | Do not commit unless promoted |
| Merge / admin `gh pr merge --admin` | Human or Codex | Document in runbook |
| Parallel unrelated issues | Different **slots** | claude-code-0 vs claude-code-1 |

---

## 7. Parallelism and throughput

### 7.1 Safe parallelism

```text
Issue A → claude-code-0 → branch feat/A-claude
Issue B → codex-1       → branch fix/B-codex
Issue C → gemini-cli-0  → branch docs/C-gemini
```

All merge to `develop` via separate PRs; **no shared branch**.

### 7.2 Unsafe parallelism (anti-patterns)

- Claude and Codex both pushing to `feat/multimodal-foundation`
- Running `cargo update` in one CLI while another runs `nix flake check` on same tree
- Duplicate issue creation for same `ffc` without checking open PRs

### 7.3 Throughput KPIs (research tracking)

| KPI | Target |
|-----|--------|
| CLI switches per merged PR | ≤ 2 (implement + fix) |
| Repeated full-repo explanations | 0 (use HANDOFF) |
| MCP sync drift incidents | 0 per sprint |
| Median time `ffc` → green CI | < 30 min (Codex lane) |

---

## 8. Integration with oxidizedgraph (future)

oxidizedgraph Phase 1+ can model CLI lanes as **graph nodes**:

```text
WORKFLOW_START → plan (Claude) → implement (Claude) → quality_gate → ffc (Codex) → human_review? → END
```

| Node | CLI lane | `NodeOutput` route |
|------|----------|-------------------|
| `plan` | Claude | `continue` |
| `implement` | Claude | `continue` |
| `quality_gate` | local / CI | `passed` / `gate_failed` |
| `ffc` | Codex | `passed` / `gate_failed` |
| `docs` | Gemini | `review` |

Store `cli_lane` and `workspace_slot` on `AgentState.context` for AIVCS traceability.

---

## 9. Decision log (ADRs)

| ID | Decision | Rationale | Status |
|----|----------|-----------|--------|
| ADR-CLI-01 | **One primary CLI per issue** | Cuts token waste | Proposed |
| ADR-CLI-02 | **Numbered workspace slots** | Parallelism without collision | Proposed |
| ADR-CLI-03 | **Codex owns `ffc` and fmt** | Cheapest path to green CI | Proposed |
| ADR-CLI-04 | **Gemini owns docs/TDD** | Large context + scribe patterns | Proposed |
| ADR-CLI-05 | **Claude owns multi-file Rust features** | Best refactor fidelity | Proposed |
| ADR-CLI-06 | **MCP via `lornu-mcp-settings` only** | Single source of truth | Proposed |
| ADR-CLI-07 | **HANDOFF.md on lane switch** | Mandatory context bundle | Proposed |

---

## 10. Implementation roadmap

### Phase 1 — Operator hygiene (1 week)

- [ ] Add issue template field `CLI lane` to stevedores repos
- [ ] Document slot layout in operator runbook (this TDD)
- [ ] Run `lornu-mcp-settings sync` on all dev machines
- [ ] Add `HANDOFF.md` to `.gitignore` template with optional commit for handoffs

### Phase 2 — Repo alignment (2–3 weeks)

- [ ] Align `CLAUDE.md` / `GEMINI.md` / `CODEX.md` in oxidizedgraph, oxidizedRAG, bullpen
- [ ] Add `docs/CLI-RUNBOOK.md` quick reference (1 page)
- [ ] Codex lane playbook for `ffc` + `nix flake check`

### Phase 3 — Orchestration (oxidizedgraph)

- [ ] `CliLane` enum on `AgentState`
- [ ] Example graph: `autonomous_dev_workflow` extended with `ffc` node
- [ ] Metrics: lane transitions in `TransitionLog`

---

## 11. Open questions for research group

1. **Cursor vs Claude Code CLI:** When should operators prefer IDE agent over headless Claude?
2. **Gemini for Rust:** Is Gemini CLI acceptable for `graphrag-core` if `GEMINI.md` mandates tests, or doc-only?
3. **Slot count:** Optimal N/M/K for claude-code / gemini / codex pools on a 64GB machine?
4. **Central handoff store:** `HANDOFF.md` vs Valkey session blob (see [RAG/KV TDD](https://github.com/stevedores-org/oxidizedRAG/blob/develop/docs/TDD-ai-inference-storage-and-rag-vectors.md))?
5. **Cost caps:** Per-lane token budgets and escalation to human?

Comment on the tracking issue (§12).

---

## 12. Distribution

| Channel | Action |
|---------|--------|
| Repository | `docs/TDD-multi-cli-orchestration-claude-gemini-codex.md` (oxidizedgraph) |
| GitHub | Research tracking issue (stevedores-org/oxidizedgraph) |
| Cross-link | oxidizedRAG #188 research thread (platform storage) |

---

## 13. References

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) — Anthropic CLI
- [Gemini CLI](https://github.com/google-gemini/gemini-cli) — Google
- [OpenAI Codex CLI](https://developers.openai.com/codex) — OpenAI
- [lornu-ai-mcp-settings](https://github.com/lornu-ai/lornu-ai-mcp-settings) — MCP sync across tools
- [oxidizedgraph Issue #18](https://github.com/stevedores-org/oxidizedgraph/issues/18) — agent orchestration roadmap
- [oxidizedRAG TDD — KV & vectors](https://github.com/stevedores-org/oxidizedRAG/blob/develop/docs/TDD-ai-inference-storage-and-rag-vectors.md)

---

*End of document.*
