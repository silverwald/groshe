# Phase 4 Detailed Execution Plan: Web Companion

**Status:** CompletedComplete (Milestones 4.1, 4.2, 4.3 & 4.4 Complete)  
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

### Milestone 4.1: Web Companion Server & API Engine (`apps/web` or `crates/cli`) (COMPLETE)
**Goal:** Provide lightweight, high-performance HTTP endpoints serving list data and mutations.
- [xx] **4.1.1 Web Server Engine**:
  - Lightweight `axum` + `tower-http` HTTP server embedded in `apps/web`.
  - REST endpoints:
    - `GET /api/lists`: Discover available local lists and metadata.
    - `GET /api/lists/:id`: Complete projected list state (items, checks, positions, categories).
    - `POST /api/lists/:id/items`: Add items supporting inline shorthand grammar (`2 gal milk @dairy`).
    - `POST /api/lists/:id/check/:item_id`: Toggle check status with LWW timestamp.
    - `DELETE /api/lists/:id/items/:item_id`: Delete item.
    - `GET /api/photos/:hash`: Retrieve 256KB thumbnail blob from `GrosheSyncNode` blobs store.
    - `GET /api/status`: Active node ID, author ID, relay status, connected peers.
- [xx] **4.1.2 Automated Route & API Unit Tests**:
  - Integration tests verifying list querying, mutation application, and photo retrieval via Axum oneshot service calls.

### Milestone 4.2: Real-Time WebSocket Synchronization Protocol (COMPLETE)
**Goal:** Deliver sub-100ms live synchronization between the browser and Groshe peers.
- [xx] **4.2.1 Live Event Streaming (`/api/ws/:id`)**:
  - Streams live events from `node.subscribe_doc(&doc)` over WebSocket to active browser sessions.
  - Broadcasts inbound web client mutations directly to the Iroh doc replica.
  - Bidirectional ping/pong heartbeat support.
- [xx] **4.2.2 Live Convergence Integration Test**:
  - `test_web_socket_streaming_and_mutation` verifying that document entries pushed to the Iroh replica trigger instant WebSocket push updates to the connected browser client.

### Milestone 4.3: Responsive Web Companion Frontend (COMPLETE)
**Goal:** A zero-dependency, self-contained single-page application matching Slint Material 3 tokens.
- [xx] **4.3.1 Clean, Modern Web UI**:
  - Embedded HTML/CSS/JS frontend in `apps/web/src/frontend.rs`.
  - Dark mode by default matching Android tokens (`#121212` / `#1e1e1e` / `#252525`).
  - Responsive layout for desktop and mobile browsers.
  - Category grouping, in-store checklist mode, and quick-add input bar.
  - Inline photo thumbnail rendering with modal view.
- [xx] **4.3.2 Offline / Stale Session Resilience**:
  - `localStorage` snapshot caching with an offline banner ("Offline / Reconnecting...") when disconnected from the anchor peer.

### Milestone 4.4: CLI Integration (`grocery serve --web`) & Documentation (COMPLETE)
**Goal:** First-class developer and user ergonomics for hosting the web companion.
- [xx] **4.4.1 CLI Serve Extension**:
  - Added `--web [BIND_ADDR]` flag to `grocery serve` (defaulting to `127.0.0.1:8080`).
  - Spawns the web companion server alongside the Iroh sync daemon in the same Tokio runtime.
- [xx] **4.4.2 Documentation**:
  - Added usage guidelines for `grocery serve --web [ADDR]` in CLI help and documentation.

---

## Commit & Quality Guidelines
- Pure Rust domain separation preserved in `groshe-core`.
- Zero compiler warnings (`-D warnings`).
- 100% test pass rate across all workspace crates (47/47 tests passing).
