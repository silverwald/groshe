# Implementation Plan: Documentation & Release Readiness

## 1. Objective & Scope
Ensure all project documentation, architecture records, and supply-chain/license checks accurately reflect the completed Phases 0–5 and post-v1 iterations (In-Place Item Editing, Drag-to-Reorder, Household Grouping, Live Peer Presence, and Household-Aware Rotation).

## 2. Tasks
1. **`DESIGN.md` Architecture & Changelog Update**:
   - Update §11 (Sharing Model) and §16 (Risks and Open Items) to document the implemented Household index document and Household-aware rotation flows.
   - Add comprehensive Changelog entries for all post-v1 iterations.
2. **`README.md` Comprehensive Guide**:
   - Document workspace crate structure (`crates/core`, `crates/store`, `crates/sync`, `crates/cli`, `apps/web`, `apps/android`, `apps/ios`).
   - Document CLI commands (`init`, `list`, `household`, `add`, `edit`, `pos`, `check`/`uncheck`, `rm`/`undo`, `ls`, `suggest`, `photo`, `export`/`import`, `serve --web`).
   - Document Web Companion REST & WebSocket endpoints.
   - Document configuration (`groshe.toml` custom/disabled relay) and test execution commands.
3. **Workspace Quality & Supply-Chain Gate**:
   - Run `cargo clippy --workspace --all-targets -- -D warnings`.
   - Run `cargo deny` (if installed) or validate `deny.toml` and `Cargo.lock` consistency.
   - Run full workspace tests (`cargo test --workspace -j 2 -- --test-threads=1`).