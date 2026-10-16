# Phase 1 Detailed Execution Plan: Core, Store, Sync, and CLI Full Peer

**Status:** Ready to Execute  
**Context:** [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) § 15 (Phase 1)  
**Target Platform Priority:** Host Workstations (CLI Full Peer), Android, Cross-Platform Engine  

---

## Executive Summary

Phase 0 validated the core technological pillars:
- Slint mobile UI ergonomics on Android (Spike 0.1: GO).
- Iroh QUIC transport, mDNS discovery, and redb durability on Android (Spike 0.2: GO).
- CLI `serve` as an always-on peer for asynchronous convergence (Spike 0.3: GO).
- Build system evaluated, resulting in a clean purge of Buck2 with Cargo retained as sole canonical build system (Spike 0.4: COMPLETE - DELETE).

**Phase 1** transitions Groshe from prototyping spikes to **production core domain, persistence store, synchronization, and full-featured CLI peer**. The CLI built in this phase serves both as a complete product surface for power users and server self-hosters (`grocery serve`), and as the deterministic integration driver for multi-peer convergence verification.

```
+-----------------------------------------------------------------------------------+
|                           Phase 1 Architecture Overview                           |
+-----------------------------------------------------------------------------------+
|                                                                                   |
|  [crates/cli] (Full Peer Binary: grocery)                                         |
|  - grocery init | list create/ls/join/share/rotate/rm | add | check | pos | ls     |
|  - grocery peers | export | import | serve (always-on daemon)                     |
|                                     |                                             |
|                                     v                                             |
|  [crates/store] (Reactive Materialized Projection)                                |
|  - Materialized ListProjection & ItemView                                         |
|  - Multi-key aggregator: meta, item/<id>/data, check, pos                          |
|  - 30-day recently deleted / undo model                                           |
|  - Broadcast event channel for reactive UI renderers                              |
|                                     |                                             |
|                                     v                                             |
|  [crates/sync] (Iroh Engine, Identity, Discovery, & Rotation)                     |
|  - Endpoint + Router + Docs + Gossip + Blobs                                      |
|  - Author keypair management & DocTicket serialization                            |
|  - List rotation flow (create new namespace, copy items, re-issue tickets)        |
|  - mDNS local discovery + WAN relay fallback                                      |
|                                     |                                             |
|                                     v                                             |
|  [crates/core] (Pure Rust Domain Engine)                                          |
|  - CBOR serialization layout matching §6.2                                        |
|  - Fractional-index position math for arbitrary reordering                        |
|  - Zero I/O, deterministic validations, minimal dependencies                      |
|                                                                                   |
+-----------------------------------------------------------------------------------+
                                      |
                            Phase 1 Exit Gate
                                      |
                                      v
                           Phase 2 (Android MVP)
```

---

## Phase 1 Work Breakdown Structure

### Milestone 1.1: Production Core Domain Engine (`crates/core`: COMPLETE)

**Goal:** Establish the canonical data models, fractional indexing, and CBOR byte layouts matching [DESIGN.md §6.2](file:///C:/projects/groshe/DESIGN.md).

#### Tasks
- [x] **1.1.1 Canonical Key Schema & CBOR Layouts**:
  - Implemented CBOR binary serialization for all doc values using `ciborium` (`to_cbor`, `from_cbor`).
  - Created canonical `DocKey` enum (`Meta`, `ItemData`, `ItemCheck`, `ItemPos`) matching §6.2 key schemas with roundtrip string and byte parsers.
- [x] **1.1.2 Fractional-Index Order Engine**:
  - Implemented deterministic midpoint generation (`between_strings`, `between`).
  - Handled prepending (`before`), appending (`after`), and arbitrary insertion without unbounded length growth.
  - Enforced lowercase ASCII interval validation.
- [x] **1.1.3 Validation & Error Handling**:
  - Implemented structured `CoreError` for empty titles/names, invalid quantities, malformed keys, and CBOR encode/decode failures.
- [x] **1.1.4 Property & Unit Tests**:
  - Implemented 10 unit tests covering domain validations, key roundtrips, and CBOR serialization.
  - Implemented `proptest` harness in `crates/core/tests/fractional_position_proptest.rs` verifying strict monotonicity across arbitrary random insertion sequences.

---

### Milestone 1.2: Reactive Materialized Store (`crates/store`: COMPLETE)

**Goal:** Implement the projection layer that reads raw `iroh-docs` entries and maintains a live, queryable `ListProjection` without querying Iroh in render loops.

#### Tasks
- [x] **1.2.1 Materialized Document Projection**:
  - Defined `ItemView` merging `ItemData`, `ItemCheck`, and `ItemPosition` by item `Uuid` with field-level timestamps.
  - Defined `ListProjection` holding active items, sorted item views, and category groupings (`items_by_category`).
  - Implemented fast in-memory indexing (`BTreeMap<ItemId, ItemView>`).
- [x] **1.2.2 Multi-Key Entry Aggregator & LWW Resolution**:
  - Built `apply_entry` aggregator resolving raw key-value events into `ItemView` components.
  - Handled tombstone entries (empty byte payloads) per §6.3 delete semantics.
  - Implemented field-level Last-Writer-Wins (LWW) timestamp comparisons so check/data/pos updates never clobber each other.
- [x] **1.2.3 Recently Deleted / Undo Buffer**:
  - Implemented `RecentlyDeletedItem` 30-day tombstone tracking buffer (`TOMBSTONE_RETENTION_SECS`).
  - Implemented `restore_deleted` supporting both LIFO undo and ID-specific item restoration.
- [x] **1.2.4 Reactive Event Channel**:
  - Built `ReactiveStore` wrapping multiple lists behind `RwLock` with a `tokio::sync::broadcast` channel emitting fine-grained `StoreEvent` notifications.
- [x] **1.2.5 Store Integration Tests**:
  - Implemented async multi-key aggregation test verifying non-clobbering updates, LWW rejection of older timestamps, tombstone deletions, and undo restoration.

---

### Milestone 1.3: Sync Node, Identity, Discovery & Rotation (`crates/sync`)

**Goal:** Implement full list lifecycle management, persistent identity, peer discovery, gossip presence, and list revocation/rotation per [DESIGN.md §7](file:///C:/projects/groshe/DESIGN.md).

#### Tasks
- [ ] **1.3.1 Persistent Identity & Keyring Management**:
  - Implement persistent `AuthorId` and device keypair generation and secure loading.
  - Configure file permissions (0600) for stored secret keys on Unix/Windows.
- [ ] **1.3.2 Multi-Document Namespace Lifecycle**:
  - Support managing multiple concurrent lists within a single `GrosheSyncNode`.
  - Implement `create_list(title)`, `join_list(ticket)`, and `leave_list(list_id)`.
  - Auto-open and resume syncing all subscribed lists on node startup.
- [ ] **1.3.3 List Sharing & Ticket Generation**:
  - Generate full-capability `DocTicket` objects with local and relay connection info.
  - Implement ASCII QR code generator for direct terminal display (`qr_code` / `qrcode-generator`).
- [ ] **1.3.4 List Rotation Flow (§7, §11, §16)**:
  - Implement `rotate_list(old_list_id)`:
    1. Read latest materialized snapshot from old namespace.
    2. Create brand-new `iroh-docs` namespace.
    3. Re-serialize active metadata and active items into the new doc.
    4. Issue new tickets.
    5. Terminate sync and drop replica of old namespace.
- [ ] **1.3.5 Presence & Gossip Topic**:
  - Derive gossip presence topic per list namespace.
  - Broadcast periodic heartbeats for "in-store" and online status.
  - Expose connected peer counter stream per list.

---

### Milestone 1.4: Full-Peer CLI Application (`crates/cli`)

**Goal:** Deliver the production `grocery` CLI binary implementing the complete grammar defined in [DESIGN.md §8](file:///C:/projects/groshe/DESIGN.md).

#### Tasks
- [ ] **1.4.1 CLI Grammar & Config Scaffold**:
  - Implement Clap CLI with global `--data-dir` and `--json` flags.
  - Implement `grocery init` for bootstrapping author credentials.
- [ ] **1.4.2 List Management Subcommands**:
  - `grocery list create <title>`
  - `grocery list ls` (displaying title, ID, item count, peer status).
  - `grocery list join <ticket>`
  - `grocery list share [--list <id>]` (prints ticket string + ASCII QR code).
  - `grocery list rotate [--list <id>]` (executes rotation, emits new ticket).
  - `grocery list rm [--list <id>]`
- [ ] **1.4.3 Item Manipulation Subcommands**:
  - `grocery add "<name>" [--qty N] [--unit u] [--category cat] [--note note] [--list L]`
  - `grocery ls [--list L] [--all] [--by-category]`
  - `grocery check <item-id-or-prefix>` and `grocery uncheck <item-id-or-prefix>`
  - `grocery edit <item-id-or-prefix> [--name N] [--qty Q] [--unit U] [--note note]`
  - `grocery pos <item-id-or-prefix> [--before <id> | --after <id> | --top | --bottom]`
  - `grocery rm <item-id-or-prefix>` and `grocery undo [--list L]`
- [ ] **1.4.4 Peers, Export & Import**:
  - `grocery peers [--list L]` (active connections, relay status, gossip presence).
  - `grocery export [--list L] [--format json|text]` (human-readable backup).
  - `grocery import <file> [--list L]` (bulk item import).
- [ ] **1.4.5 Headless Daemon `grocery serve`**:
  - Implement persistent headless daemon mode suitable for systemd / Windows Service / container.
  - Keep all registered list replicas open, answer sync queries, maintain relay connections.
  - Clean shutdown on `SIGINT` / `SIGTERM` / `Ctrl+C`.

---

### Milestone 1.5: Property Testing & Multi-Peer Convergence Verification

**Goal:** Prove 100% data consistency and convergence under adverse network conditions per [DESIGN.md §14](file:///C:/projects/groshe/DESIGN.md).

#### Tasks
- [ ] **1.5.1 CRDT Property Tests (`proptest`)**:
  - Generate randomized mutation streams (adds, edits, checks, deletes, reorders).
  - Replay events across 3 simulated replicas with out-of-order delivery, packet drops, and delays.
  - Assert that all replicas converge to identical byte-for-byte materialized state.
- [ ] **1.5.2 Multi-Peer CLI Convergence Integration Tests**:
  - Automated 3-node end-to-end integration test driving separate CLI processes.
  - Simulate concurrent offline edits on Peer 1 and Peer 2, reconnect through Peer 3 (`serve`), and verify clean reconciliation.
- [ ] **1.5.3 List Rotation Integration Test**:
  - Verify that a 3-peer list rotation cleanly migrates all items, invalidates old tickets for removed peers, and synchronizes subsequent writes on the new namespace.

---

## Commit & Quality Guidelines

1. **Atomic Commits:** ~300 added/modified lines per commit where practical.
2. **Three-Part Commit Messages:**
   - **What:** Concise summary of additions or refactors.
   - **Why:** Mapping directly to Phase 1 tasks or DESIGN.md requirements.
   - **How Tested:** Concrete command and test results.
3. **Quality Gates:**
   - Zero compiler warnings (`cargo check --workspace --all-targets`).
   - Clean clippy lints (`cargo clippy --workspace --all-targets -- -D warnings`).
   - 100% pass rate on workspace test suite (`cargo test --workspace`).

---

## Phase 1 Exit Deliverables

1. **Pure Rust Domain Engine**: Validated CBOR serialization and fractional position ordering in `crates/core`.
2. **Reactive Projection Store**: In-memory and persisted `ListProjection` with event streaming in `crates/store`.
3. **Sync & Lifecycle Engine**: Document creation, joining, sharing, rotation, and presence in `crates/sync`.
4. **Complete CLI Binary**: Working `grocery` CLI with all §8 subcommands and `serve` daemon in `crates/cli`.
5. **Convergence Test Suite**: Passing `proptest` and 3-peer integration tests proving eventual consistency.
