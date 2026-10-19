# Phase 4 Detailed Execution Plan: Web Companion

**Status:** In Progress (Branch: `feature/phase-4-web-companion`)  
**Context:** [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) §8 (CLI / Daemon), §9 (Web Companion), §15 (Phase 4)  
**Target Branch:** `feature/phase-4-web-companion`  

---

## Executive Summary

Per [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) §9, the **Web Companion** participates in a *live session only*, anchored by an active, always-on peer (such as `grocery serve` running on a home server or local machine):
- **Live session only**: It connects over relay/HTTP/WebSocket while at least one full peer for that list is online.
- **No WASM/IndexedDB fragility**: It avoids the risks of incomplete browser WASM Iroh storage, holding an active in-session view and falling back to a cached read-only snapshot in `localStorage` when offline.
- **Full grocery functionality**: Viewing lists, adding items with shorthand grammar parsing, checking off items, reordering, viewing category groupings, and retrieving photo thumbnail blobs.

---

## Milestones Breakdown

### Milestone 4.1: Web Companion Server & API Engine (`apps/web` or `crates/cli`)
**Goal:** Provide lightweight, high-performance HTTP endpoints serving list data and mutations.
- [ ] **4.1.1 Web Server Engine**:
  - Lightweight `axum` + `tower-http` HTTP server embedded in the Groshe ecosystem.
  - REST endpoints:
    - `GET /api/lists`: Discover available local lists and metadata.
    - `GET /api/lists/:id`: Complete projected list state (items, checks, positions, categories).
    - `POST /api/lists/:id/items`: Add items supporting inline shorthand grammar (`2 gal milk @dairy`).
    - `POST /api/lists/:id/check/:item_id`: Toggle check status with LWW timestamp.
    - `DELETE /api/lists/:id/items/:item_id`: Delete item.
    - `GET /api/photos/:hash`: Retrieve 256KB thumbnail blob from `GrosheSyncNode` blobs store.
    - `GET /api/status`: Active node ID, author ID, relay status, connected peers.
- [ ] **4.1.2 Automated Route & API Unit Tests**:
  - Tests verifying list querying, mutation application, and photo retrieval via Axum `TestClient` or mock requests.

### Milestone 4.2: Real-Time WebSocket Synchronization Protocol
**Goal:** Deliver sub-100ms live synchronization between the browser and Groshe peers.
- [ ] **4.2.1 Live Event Streaming (`/api/ws/:id`)**:
  - Stream live events from `node.subscribe_doc(&doc)` over WebSocket to active browser sessions.
  - Broadcast inbound web client mutations directly to the Iroh doc replica.
- [ ] **4.2.2 Live Convergence Integration Test**:
  - Test verifying that a mutation sent over WebSocket updates the CRDT replica and is received by peer projections.

### Milestone 4.3: Responsive Web Companion Frontend
**Goal:** A zero-dependency, self-contained single-page application matching Slint Material 3 tokens.
- [ ] **4.3.1 Clean, Modern Web UI**:
  - Theme matching Slint Android app (`#141218` background, `#211f26` surface, `#d0bcff` primary).
  - Responsive layout for desktop and mobile browsers.
  - Category grouping, in-store checklist mode, and quick-add input bar.
  - Visual photo thumbnail rendering for items with `photo_hash`.
- [ ] **4.3.2 Offline / Stale Session Resilience**:
  - `localStorage` snapshot caching with a prominent "Cached Snapshot — Connecting..." indicator when the server or peer is unreachable.

### Milestone 4.4: CLI Integration (`grocery serve --web`) & Documentation
**Goal:** First-class developer and user ergonomics for hosting the web companion.
- [ ] **4.4.1 CLI Serve Extension**:
  - Add `--web [BIND_ADDR]` flag to `grocery serve` (e.g. `grocery serve --web 127.0.0.1:8080`).
  - Embed web assets directly into the binary so no runtime asset paths or external tools are required.
- [ ] **4.4.2 Documentation**:
  - Update `docs/` and `README.md` with usage instructions for running `grocery serve --web` and accessing from mobile/desktop browsers.

---

## Commit & Quality Guidelines
- Pure Rust domain separation preserved in `groshe-core`.
- Zero compiler warnings (`-D warnings`).
- 100% test pass rate across all workspace crates.
