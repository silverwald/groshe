# Phase 2 Detailed Execution Plan: Android MVP

**Status:** 100% Complete  
**Context:** [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) §13 (UX & Screen Flows), §15 (Phase 2)  
**Target Platforms:** Android (`aarch64-linux-android`, `x86_64-linux-android`), Host Desktop Simulator (`groshe-mobile-preview`)  

---

## Executive Summary

Phase 1 established the production core domain engine (`crates/core`), reactive materialized projection store (`crates/store`), peer-to-peer Iroh sync layer (`crates/sync`), and the complete `grocery` CLI peer (`crates/cli`).

**Phase 2** built the full native Android MVP user experience using **Slint UI** backed by our Rust core and reactive store.

```
+-----------------------------------------------------------------------------------+
|                           Phase 2 Architecture Overview                           |
+-----------------------------------------------------------------------------------+
|                                                                                   |
|  [apps/android/ui] (Slint Declarative UI - Material 3 / iOS HIG Fusion)           |
|  - Screen 1: Lists Screen (Cards, counters, peer status, empty state, [+] FAB)   |
|  - Screen 2: List Detail Screen (Collapsible category sections, quick-add, edit) |
|  - Screen 3: In-Store Mode (High-contrast 48dp touch targets, auto-collapse done) |
|  - Screen 4: Share / Join Modal (Ticket display, QR code, camera/paste ticket)   |
|  - Screen 5: Settings Screen (Author ID, relay status, database prune/export)    |
|                                     |                                             |
|                                     v                                             |
|  [apps/android/src/sync_bridge.rs] (Reactive Slint <-> Rust Channel Bridge)        |
|  - Slint VecModel mutations -> Iroh Doc CRDT operations                           |
|  - LiveEvent subscription -> Slint invoke_from_event_loop UI model updates       |
|  - Multi-list tracking & active projection switching                              |
|                                     |                                             |
|                                     v                                             |
|  [apps/android/src/android_runtime.rs] (Android Lifecycle & Storage)             |
|  - Android Activity lifecycle (on_resume, on_pause, on_stop)                      |
|  - App storage path (/data/data/com.groshe.app/files vs desktop preview dir)      |
|  - Haptic feedback JNI hook for in-store purchases                               |
|                                     |                                             |
|                                     v                                             |
|  [crates/core, crates/store, crates/sync] (Phase 1 Verified Foundation)          |
|                                                                                   |
+-----------------------------------------------------------------------------------+
```

---

## Milestones Breakdown

### Milestone 2.1: Slint Multi-Screen Navigation Architecture & UI Themes (COMPLETE)
**Goal:** Implement the complete declarative Slint UI with multi-screen navigation, design tokens, and Material 3 / iOS HIG typography.
- [x] **2.1.1 Slint Screen Hierarchy & Navigation**:
  - Implement bottom bar / navigation router (`Lists`, `Detail`, `InStore`, `Share`, `Settings`).
  - Implement top app bar with dynamic title, peer presence badges, and context menu.
- [x] **2.1.2 Lists Screen (Home)**:
  - List cards displaying title, active/total items, peer status dot (green=online, gray=offline), last edited timestamp.
  - Context menu actions: Share, Rotate, Rename, Delete.
  - Friendly empty state with "Create your first list" and "Join via QR" CTAs.
- [x] **2.1.3 List Detail Screen**:
  - Collapsible category sections adhering to grocery category order.
  - Item row: Checkbox, primary item name, secondary quantity+unit, tertiary note, drag handle, swipe delete with undo toast.
  - Floating/bottom quick-add bar.
- [x] **2.1.4 In-Store Mode Screen**:
  - Fullscreen minimal chrome, high-contrast palette, large touch targets (48dp minimum).
  - Uncompleted items grouped by category; checked items auto-collapse into "Done" section at bottom.
  - "Finish Shopping" confirmation prompt.
- [x] **2.1.5 Share / Join & Settings Screens**:
  - Share modal: QR code display, copy ticket button.
  - Join modal: Text input for pasting ticket string, join action.
  - Settings screen: Author ID, device key, connected peers count, relay status, tombstone pruning.

### Milestone 2.2: Reactive Slint-Rust Integration Bridge (`sync_bridge`) (COMPLETE)
**Goal:** Connect the Slint declarative UI to `ReactiveStore`, `ListProjection`, and `GrosheSyncNode`.
- [x] **2.2.1 Multi-List State Management**:
  - Wire list creation, listing, selection, deletion, and rotation.
  - Expose `[GroceryListInfo]` model dynamically populated from local document replicas.
- [x] **2.2.2 Item CRUD & Positional Ordering**:
  - Wire `add_item` with automatic fractional position calculation.
  - Wire `toggle_item_check` emitting `ItemCheck` updates with LWW timestamps.
  - Wire `delete_item` with tombstone creation and undo toast restore.
  - Wire `reorder_item` updating fractional position keys.
- [x] **2.2.3 Live P2P Event Synchronization**:
  - Subscribe to `GrosheSyncNode::subscribe_doc`.
  - Dispatch incoming CRDT updates to Slint via `slint::invoke_from_event_loop`.

### Milestone 2.3: Quick-Add Grammar Shorthand & Category Auto-Suggestion (COMPLETE)
**Goal:** Deliver fast one-handed grocery entry per DESIGN.md §13.
- [x] **2.3.1 Inline Shorthand Parser**:
  - Parse `"2 lbs apples"` -> quantity `2.0`, unit `"lbs"`, name `"apples"`.
  - Parse `"milk 1 gal"` -> quantity `1.0`, unit `"gal"`, name `"milk"`.
  - Parse `"3 avocados"` -> quantity `3.0`, unit `None`, name `"avocados"`.
  - Parse `"1/2 lb butter"` -> quantity `0.5`, unit `"lb"`, name `"butter"`.
- [x] **2.3.2 Grocery Category Classification**:
  - Built-in categorization dictionary matching common supermarket taxonomy (Produce, Dairy, Meat, Bakery, Pantry, Frozen, Beverage, Household).
  - Auto-suggest category chip dynamically in Slint UI.

### Milestone 2.4: Android JNI Runtime, Lifecycle & Durability Integration (COMPLETE)
**Goal:** Native Android Activity glue and background lifecycle management.
- [x] **2.4.1 Native Storage & Activity Lifecycle**:
  - Retrieve internal storage path from Android `native_app_glue`.
  - Suspend/resume sync bridge command processing.
- [x] **2.4.2 Haptic Feedback JNI Hook**:
  - Android `Vibrator` JNI call on item check in In-Store mode.
  - `VIBRATE` permission added to `AndroidManifest.xml`.
  - Verified compilation on `aarch64-linux-android` and `x86_64-linux-android`.

### Milestone 2.5: Build, Package & End-to-End Verification (COMPLETE)
**Goal:** Successful APK packaging and end-to-end sync verification between phone and CLI.
- [x] **2.5.1 Desktop Preview Verification**:
  - Built and verified `groshe-mobile-preview` locally on host.
- [x] **2.5.2 Android NDK Build & APK Packaging**:
  - Executed `just android-build release` for `arm64-v8a` and `x86_64`.
  - Packaged signed release APKs via `scripts/package-apk.ps1`:
    - `target/groshe-release-arm64-v8a.apk` (14.48 MB < 25 MB budget).
    - `target/groshe-release-x86_64.apk` (15.41 MB < 25 MB budget).
- [x] **2.5.3 Cross-Device Sync Verification**:
  - 100% test pass rate across workspace (29 unit & integration tests).
  - Multi-peer gossip convergence verified with deterministic position ordering.

---

## Commit & Quality Guidelines
- ~300 lines per commit where practical.
- 3-part commit messages (What, Why, How Tested).
- Zero compiler warnings, 100% test pass rate across all workspace crates.
