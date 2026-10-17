# Phase 5 Detailed Execution Plan: Hardening & Extras

**Status:** In Progress (Milestones 5.1, 5.2 & 5.3 Complete)  
**Context:** [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) §10 (Mobile UI), §15 (Phase 5)  
**Target Branch:** `feature/phase-5-hardening-extras`  

---

## Executive Summary

Phase 1 delivered the pure Rust core, reactive projection store, P2P Iroh sync layer, and CLI peer.
Phase 2 delivered the complete Android MVP (Slint multi-screen UI, reactive sync bridge, quick-add parser, Android JNI runtime, haptic feedback, release APK packaging, and emulator automation).

**Phase 5** enhances the product with essential durability, ergonomics, and production hardening features per DESIGN.md §15:
1. **Frequent-item suggestions from history**: Fast 1-tap entry in Slint and CLI based on addition frequency & recency decay.
2. **Backup, export, and import**: Full JSON snapshot backups and human-readable Markdown/plain-text checklist exports.
3. **Item photos via iroh-blobs**: High-performance, bandwidth-capped thumbnail/photo attachments synced over Iroh blobs.
4. **Self-hosted relay documentation & config**: Configurable relay endpoints in `groshe.toml` for sovereign, zero-third-party deployments.

---

## Milestones Breakdown

### Milestone 5.1: Frequent-Item Suggestions from History (COMPLETE)
**Goal:** Deliver 1-tap item entry suggestions based on personal grocery habits.
- [x] **5.1.1 History Tracker Model (`crates/store/src/history.rs`)**:
  - Track item name, normalized canonical key, category, default quantity/unit, usage count, and last added timestamp.
  - Score items using a frequency + recency decay formula: $\text{score} = \text{count} / (1.0 + \text{age\_in\_days} \times 0.05)$.
  - Expose `suggest_frequent(limit: usize)` filtering out items already on the active list.
  - 4 dedicated unit tests covering canonicalization, scoring, recency decay, and CBOR disk persistence.
- [x] **5.1.2 Sync Bridge & UI Integration (`apps/android`)**:
  - Automatically record items added or checked off into the local history store (`history.cbor`).
  - Expose `frequent_suggestions` model `[string]` to `detail_screen.slint`.
  - Render horizontal suggestion chips above the quick-add input bar for 1-tap item entry.
- [x] **5.1.3 CLI Suggestion Support (`crates/cli`)**:
  - Added `grocery suggest [--limit N] [--add-top]` command listing top items with usage count.
  - Automatically records items added or checked off in the CLI into `history.cbor`.

### Milestone 5.2: Complete Backup, Export & Import (COMPLETE)
**Goal:** Data portability and zero lock-in with human-readable exports and full backups.
- [x] **5.2.1 Format Encoders & Decoders (`crates/core/src/export.rs`)**:
  - Full-fidelity JSON format: list metadata, active and checked items, notes, positions.
  - Human-readable Markdown format: grouped by category with `- [ ]` checkboxes and `- [x]` completed items.
  - Parser for importing markdown/JSON back into a list.
  - 3 dedicated unit tests covering JSON roundtrip, GFM formatting, and Markdown parsing.
- [x] **5.2.2 CLI Import & Export Commands (`crates/cli`)**:
  - `grocery export [--list <id>] [--format json|markdown|text] [--output <path>]`
  - `grocery import <path> [--list <id>]`
- [x] **5.2.3 Slint Settings Screen Export (`apps/android`)**:
  - Added "Export Active List (Markdown)" and "Full Backup (JSON)" actions in `settings_screen.slint`.
  - Wired callbacks in `sync_bridge.rs` and `lib.rs` saving exports into `storage_dir/exports/`.

### Milestone 5.3: Item Photos via Iroh-Blobs (COMPLETE)
**Goal:** Attach reference photos (e.g. specific packaging/brand) synced over Iroh content-addressed blobs.
- [x] **5.3.1 Photo Blob Schema (`crates/core/src/model.rs`, `export.rs`)**:
  - Added `photo_hash: Option<[u8; 32]>` (Blake3 hash) to `ItemData` with builder method `with_photo_hash`.
  - Added `photo_hash: Option<String>` to `ExportItem` for JSON export/import portability with `parse_hex_32` and `hex_32` helpers.
- [x] **5.3.2 Blob Sync Engine (`crates/sync/src/lib.rs`)**:
  - Integrated `iroh_blobs::api::Store` into `GrosheSyncNode`.
  - Implemented `add_photo_blob(&self, data: &[u8]) -> Result<[u8; 32], SyncError>` enforcing the 256KB thumbnail cap.
  - Implemented `get_photo_blob(&self, hash_bytes: &[u8; 32]) -> Result<Vec<u8>, SyncError>` retrieving raw bytes by Blake3 hash.
  - Added unit test `test_photo_blob_storage_and_cap` verifying blob storage, byte-for-byte retrieval, and 256KB cap rejection.
- [x] **5.3.3 CLI Photo Commands & Options (`crates/cli`)**:
  - Added `--photo <path>` option to `grocery add` to attach photos during item creation.
  - Added `grocery photo <item> --output <path> [--list <id>]` to inspect and extract item photos.
  - Display photo indicators in `grocery ls` and JSON item DTOs.
- [x] **5.3.4 Slint UI Photo Indicators (`apps/android`)**:
  - Added `has_photo: bool` field to `GroceryItemData`.
  - Visual photo badges rendered in both `detail_screen.slint` (`📷 photo`) and `in_store_screen.slint` (`📷`).
  - Populated `has_photo: item.data.photo_hash.is_some()` in `sync_bridge.rs`.

### Milestone 5.4: Self-Hosted Relay & Hardening
**Goal:** Sovereign infrastructure deployment and connection resilience.
- [ ] **5.4.1 Relay Configuration in `groshe.toml`**:
  - Allow specifying custom Iroh relay URLs in configuration.
- [ ] **5.4.2 Documentation & Verification**:
  - Add `docs/RELAY_SETUP.md` with instructions for running an independent `iroh-relay`.

---

## Commit & Quality Guidelines
- Pure Rust domain separation preserved.
- Zero compiler warnings (`-D warnings`).
- 100% test pass rate across all workspace crates.
