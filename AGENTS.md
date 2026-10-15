# Agent Guidelines & Workflow Rules

These rules apply to all AI agents and automated assistants working in the Groshe repository.

---

## 1. Commit Sizing & Granularity

- **Size Target (~300 lines added/modified):**
  - Organize work into incremental, focused commits of approximately **300 lines of code/config** added or modified per commit.
  - Avoid giant monolithic commits. Break down multi-file implementations into logical, self-contained, working steps.
- **Deletions Have No Limit:**
  - Removing code, dead code, obsolete dependencies, or refactoring reductions has no line limit.
- **Buildable & Testable States:**
  - Each commit must leave the repository in a clean, passing state: `cargo check` and relevant tests must pass. Never leave broken intermediate commits.

---

## 2. Commit Message Standards

Every commit message must be comprehensive and provide full context for reviewers and human maintainers:

1. **Title Line:**
   - Conventional Commit format (`feat: ...`, `fix: ...`, `test: ...`, `refactor: ...`, `docs: ...`, `chore: ...`).
   - Concise summary of the change (< 72 characters).

2. **What Changed & Why (Rationale):**
   - Provide a clear, detailed rationale explaining the problem being solved, architectural decisions made, and specific files/structures modified.
   - Reference relevant sections in [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) or [PHASE_0_PLAN.md](file:///C:/projects/groshe/PHASE_0_PLAN.md).

3. **How It Was Tested:**
   - Explicitly document the verification procedure.
   - State the exact command(s) executed (e.g., `cargo test -p groshe-core`, `cargo check --workspace --all-targets`).
   - Mention newly added unit or integration tests and their results.

### Commit Message Template

```gitcommit
<type>(<scope>): <concise summary>

What changed and why:
- Detailed explanation of changes made across affected crates/files.
- Architectural rationale and context tying back to DESIGN.md.

How it was tested:
- Specific commands executed (e.g. `cargo test -p <crate>`, `cargo clippy`).
- New test cases added and outcome verified.
```

---

## 3. Architecture & Separation of Concerns

- **`crates/core`**: Pure Rust domain logic, no I/O, no UI dependencies, minimal external crates (`serde`, `uuid`, `thiserror`).
- **`crates/sync`**: Iroh networking, Endpoint, Router, iroh-docs replicas, DocTickets, and gossip presence.
- **`crates/store`**: Materialized view projection and on-disk persistence (`redb`). Frontends read projections; they never query Iroh directly in render loops.
- **`crates/cli`**: Full-peer binary and headless `serve` daemon.
- **`apps/android`**: Slint UI for Android.
- Keep frontends thin and CRDT/sync logic testable headlessly.
