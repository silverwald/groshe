# Phase 0 Detailed Execution Plan: De-risking Spikes

**Status:** In Progress (Spikes 0.0, 0.1 Foundation, and 0.3 Complete)  
**Context:** [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) §15 (Phase 0)  
**Target Platform Priority:** Android, CLI, Host Workstations

---

## Executive Summary

Phase 0 is the **Go / No-Go de-risking phase** for Groshe. Before committing to full production development across `crates/*` and `apps/*`, we must validate four foundational pillars in isolation:

1. **Slint Mobile UI on Android**: Real-device UX, soft keyboard (IME), touch gestures (swipe-to-delete, drag-reorder), TalkBack accessibility, and cold start performance.
2. **Iroh P2P Sync on Android**: Native execution, mDNS direct LAN sync, WAN NAT-traversal / relay fallback, and redb persistence through mobile process lifecycles.
3. **CLI `serve` as Always-On Peer**: Asynchronous multi-peer store-and-forward convergence between intermittent mobile peers without an application server.
4. **Buck2 Build Spike (`crates/core` only)**: Time-boxed (2 days, non-gating) evaluation of Buck2 + Reindeer hermetic builds alongside canonical Cargo.

```
+-----------------------------------------------------------------------------------+
|                                Phase 0 Spikes                                     |
+--------------------------+----------------------------+---------------------------+
| 0.1 Slint on Android     | 0.2 Iroh on Android        | 0.3 CLI Always-On "serve" |
| - IME & Touch Ergonomics | - mDNS LAN sync            | - Headless replica daemon |
| - Swipe / Reorder        | - Relay / NAT traversal    | - Async phone-CLI-phone   |
| - TalkBack & Theming     | - Process kill / redb test |   store & forward test    |
+--------------------------+----------------------------+---------------------------+
|                          0.4 Buck2 Spike (Non-gating)                             |
|                          - Pure Rust //crates/core                                |
|                          - Reindeer third-party / IDE test                        |
+-----------------------------------------------------------------------------------+
                                        |
                             Go / No-Go Decision Gate
                                        |
                                        v
                            Phase 1 (Production Core + CLI)
```

---

## Phase 0 Work Breakdown Structure

### Spike 0.0: Repository & Workspace Baseline Scaffolding

Before launching the spikes, establish a clean, reproducible multi-target Cargo workspace.

#### Tasks
- [x] **0.0.1 Toolchain configuration**:
  - Pinned stable Rust in [`rust-toolchain.toml`](file:///C:/projects/groshe/rust-toolchain.toml).
  - Target triples configured: `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`, `aarch64-linux-android`, `x86_64-linux-android`.
- [x] **0.0.2 Workspace Root Manifest**:
  - Root [`Cargo.toml`](file:///C:/projects/groshe/Cargo.toml) declaring workspace members: `crates/core`, `crates/sync`, `crates/store`, `crates/cli`, `apps/android`.
  - Unified workspace dependencies configured (`serde`, `uuid`, `thiserror`, `tokio`, `clap`, `slint`).
- [x] **0.0.3 Task Runner & Developer Tooling**:
  - Created [`justfile`](file:///C:/projects/groshe/justfile) with standard recipes (`build`, `test`, `check`, `fmt`, `lint`, `cli`, `android-build`).
  - Upgraded to `just 1.58.0` and configured `[windows] set shell := ["powershell.exe", "-NoLogo", "-Command"]`.
- [x] **0.0.4 Skeleton Crates Initialization**:
  - Implemented pure domain model and fractional ordering unit tests in [`crates/core/src/lib.rs`](file:///C:/projects/groshe/crates/core/src/lib.rs).
  - Implemented sync interface types in [`crates/sync/src/lib.rs`](file:///C:/projects/groshe/crates/sync/src/lib.rs).
  - Implemented materialized store projection in [`crates/store/src/lib.rs`](file:///C:/projects/groshe/crates/store/src/lib.rs).
  - Implemented complete CLI command grammar in [`crates/cli/src/main.rs`](file:///C:/projects/groshe/crates/cli/src/main.rs).
  - Implemented mobile native entry hook in [`apps/android/src/lib.rs`](file:///C:/projects/groshe/apps/android/src/lib.rs).
  - Verified `cargo check --workspace --all-targets` and `cargo test --workspace` pass cleanly.

---

### Spike 0.1: Slint on Android Spike (Go / No-Go)

**Goal:** Verify that Slint provides a production-grade Android mobile experience that meets §10 requirements without platform blockers.

#### Test Prototype Scope (§10)
A standalone prototype in `apps/android` containing:
1. **List overview screen**: Simple list of grocery lists with status badges.
2. **List detail screen**: Mock items grouped into 2 categories, checked vs. unchecked items.
3. **Add item input**: Fixed bottom text input field with "Add" button and quantity counter.
4. **In-store mode toggle**: Large typography mode with checked items collapsed.

#### Tasks
- [x] **0.1.1 Android Packaging & Toolchain**:
  - Implemented NativeActivity entry point `android_main` in [`apps/android/src/lib.rs`](file:///C:/projects/groshe/apps/android/src/lib.rs).
  - Configured Android-specific dependency `slint` with `backend-android-activity-06` in [`apps/android/Cargo.toml`](file:///C:/projects/groshe/apps/android/Cargo.toml).
  - Created [`apps/android/android/AndroidManifest.xml`](file:///C:/projects/groshe/apps/android/android/AndroidManifest.xml) with NativeActivity, internet, network-state, and Wi-Fi multicast permissions.
  - Installed OpenJDK 17 and `cargo-ndk` v4.1.2.
  - Verified clean compilation for Android targets: `cargo check --target aarch64-linux-android -p groshe-android` and `cargo check --target x86_64-linux-android -p groshe-android`.
- [x] **0.1.2 Soft Keyboard (IME) Ergonomics**:
  - Sticky bottom `LineEdit` with Enter / Done `accepted` handler in [`apps/android/ui/app.slint`](file:///C:/projects/groshe/apps/android/ui/app.slint).
  - Set `android:windowSoftInputMode="adjustResize"` in `AndroidManifest.xml` to prevent keyboard occlusion of the input field.
- [x] **0.1.3 Touch Target & Gestures**:
  - Minimum touch targets calibrated to $\ge 48\,\text{dp}$ standard, $\ge 56\,\text{dp}$ in-store mode.
  - Implemented item deletion with animated undo toast notification and state restoration via `slint::VecModel`.
- [x] **0.1.4 Accessibility (TalkBack)**:
  - Annotated interactive UI elements with `accessible-role`, `accessible-label`, and `accessible-checked` in [`apps/android/ui/app.slint`](file:///C:/projects/groshe/apps/android/ui/app.slint).
- [x] **0.1.5 Theming & System Integration**:
  - Implemented high-contrast dark mode palette matching Material 3 recommendations.
  - Implemented in-store mode toggle expanding typography to $20\text{--}22\,\text{px}$ and collapsing checked items.
- [ ] **0.1.6 Physical Reference Device Verification**:
  - Run APK on physical reference device / emulator to record cold start time and APK package size.

#### Evaluation & Exit Criteria
| Metric / Check | Target / Acceptance Criteria | Failure Action |
|---|---|---|
| **IME Interaction** | Input remains visible, typing smooth, dismiss works | If keyboard occludes input or locks focus, investigate Slint window flags or report upstream. |
| **Accessibility** | TalkBack traverses all items and actions intelligibly | If Slint accessibility bridge is incomplete on Android, assess feasibility of upstream PR or evaluate alternatives. |
| **Touch Targets** | $\ge 48\,\text{dp}$ standard, $\ge 56\,\text{dp}$ in-store | Adjust layout paddings. |
| **Cold Start** | $< 1.5$ seconds on mid-range Android phone | Profile rendering pipeline and asset preloading. |
| **APK Binary Size** | $< 25\,\text{MB}$ release package | Check symbol stripping, LTO, and font bundling. |

---

### Spike 0.2: Iroh on Android Spike (Go / No-Go)

**Goal:** Prove that Iroh (QUIC transport, Endpoint, Router, iroh-docs, redb) functions stably on Android across mobile lifecycle events and network topologies.

#### Prototype Scope
A headless or minimal test harness running on Android invoking `iroh-docs` operations: create doc, insert entry, subscribe to remote changes, and read doc.

#### Tasks
- [ ] **0.2.1 Native Runtime & Storage Integration**:
  - Run Tokio multi-thread runtime inside Android process (via JNI/Rust bridge).
  - Map Iroh storage / `redb` path to Android internal storage directory (`context.getFilesDir()`).
  - Declare and verify Android permissions: `INTERNET`, `ACCESS_NETWORK_STATE`, and multicast lock for local discovery.
- [ ] **0.2.2 LAN Discovery & Direct QUIC Sync**:
  - Run two peers on the same local Wi-Fi (Device A physical, Device B emulator or second device).
  - Verify mDNS peer discovery and direct QUIC connection establishment.
  - Insert entries on Device A (`item/<id>/data`); assert Device B receives `LiveEvent::InsertRemote` within 1000ms.
- [ ] **0.2.3 Relay Fallback & NAT Traversal**:
  - Disconnect Device B from Wi-Fi and place on cellular data (CGNAT) or separate firewalled network.
  - Exchange list DocTicket containing relay node address (n0 public relay or self-hosted).
  - Assert that synchronization succeeds via end-to-end encrypted relay transport when direct UDP is blocked.
- [ ] **0.2.4 Mobile Process Lifecycle & Database Durability**:
  - **Backgrounding**: Background app during active sync; ensure connections cleanly suspend/resume without panic.
  - **App Kill Test**: Force-kill (`kill -9` or swipe away in Android task switcher) while writes are occurring.
  - **Restart & Recovery**: Re-open app, verify `redb` database opens without corruption or lock stale lockfile errors.
  - **Re-convergence**: Inserted items before kill persist; sync catches up missing updates immediately upon reconnect.

#### Evaluation & Exit Criteria
| Metric / Check | Target / Acceptance Criteria | Failure Action |
|---|---|---|
| **LAN Sync Latency** | $< 1.5\,\text{s}$ over local Wi-Fi | Tune mDNS poll interval and connection timeouts. |
| **Relay Sync Latency** | $< 3.0\,\text{s}$ across NAT/Cellular | Verify relay server connectivity and STUN/DERP fallback. |
| **redb Durability** | $0$ database corruption incidents across 10 force-kills | If redb locks fail on Android, test WAL/flush parameters or alternate backend. |
| **Resource Usage** | Zero background wake-locks when idle; sync only when active | Enforce strict idle teardown of QUIC endpoints on mobile. |

---

### Spike 0.3: CLI `serve` as Always-On Peer Spike (Go / No-Go: COMPLETE - GO)

**Goal:** Validate the asynchronous store-and-forward architecture (§5.1 & §8) where mobile peers sync via an always-on desktop/server CLI peer without needing an application server.

#### Test Topology
```
[Phone A]                [CLI "grocery serve"]                [Phone B]
(Online, adds Item 1) ------> Syncs to CLI
(Phone A goes Offline)          |
                                | (CLI retains replica on disk)
                                |
                                +------------------------> [Phone B comes Online]
                                                           (Receives Item 1)
                                                           (Checks Item 1 & adds Item 2)
                                <------------------------- [Phone B syncs to CLI]
                                                           (Phone B goes Offline)
[Phone A comes Online] <--------- Syncs from CLI
(Receives Item 2 & Checked state)
```

#### Tasks
- [x] **0.3.1 Minimal `crates/cli` Daemon**:
  - Implemented `grocery serve` command in `crates/cli`.
  - Configured persistent author identity and on-disk `redb` + blobs storage (`resolve_data_dir`).
  - Added list ticket joining (`grocery serve --ticket <DocTicket>`) and automatic multi-document reconciliation on startup.
  - Daemon runs indefinitely, accepting sync requests and shutting down cleanly on `Ctrl+C`.
- [x] **0.3.2 Headless Integration Test Harness**:
  - Implemented 3-node simulated integration test in [`crates/sync/tests/async_3node_convergence.rs`](file:///C:/projects/groshe/crates/sync/tests/async_3node_convergence.rs):
    1. Node 1 (Phone A) creates doc, adds `item/1/data = "Oat Milk"`.
    2. Node 2 (`grocery serve`) syncs and stores entry on disk.
    3. Node 1 shuts down completely.
    4. Node 3 (Phone B) boots up with ticket, joins list, syncs from Node 2 (`grocery serve`).
    5. Node 3 verifies "Oat Milk" is present, updates `item/1/check = true`, adds `item/2/data = "Apples"`.
    6. Node 3 shuts down completely.
    7. Node 1 boots up, syncs with Node 2 (`grocery serve`).
    8. Node 1 verifies "Oat Milk" is checked and "Apples" is present.

#### Evaluation & Exit Criteria
- **Pass Criteria:** 100% data consistency achieved on both intermittent nodes without Node 1 and Node 3 ever being online simultaneously. **[PASSED: Verified in `test_async_3node_peer_convergence` in 7.44s]**
- **Diagnostics:** Verified zero dropped entries and verified timestamp/LWW reconciliation behavior conforms to §6.3.
- **Decision:** **GO** — Asynchronous multi-peer convergence operates reliably through the always-on CLI peer replica without needing any centralized server infrastructure.

---

### Spike 0.4: Buck2 Spike on `crates/core` (Time-boxed: 2 Days, Non-Gating)

**Goal:** Test Buck2 compilation and rust-analyzer integration against `crates/core` strictly per §12.2.

#### Scope Constraint
- **Target:** `//crates/core` only (pure Rust, no `build.rs`, no C dependencies).
- **Tooling:** Buck2 via dotslash, Reindeer for third-party BUCK definitions from `Cargo.lock` .
- **Time Box:** 2 business days hard ceiling.

#### Tasks
- [ ] **0.4.1 Tooling & Environment**:
  - Configure dotslash wrapper for `buck2`.
  - Set up root [`.buckconfig`](file:///C:/projects/groshe/.buckconfig) targeting host Rust toolchain.
- [ ] **0.4.2 Dependency Vendoring via Reindeer**:
  - Configure `reindeer.toml` referencing `Cargo.lock`.
  - Run Reindeer to generate third-party BUCK definitions for `core` dependencies (`serde`, `uuid`, `thiserror`).
- [ ] **0.4.3 Core Rule Definition**:
  - Write `crates/core/BUCK`: define `rust_library` for `core` and `rust_test` for unit tests.
- [ ] **0.4.4 Benchmark & Developer Ergonomics**:
  - Verify `buck2 build //crates/core:...` and `buck2 test //crates/core:...`.
  - Generate `rust-project.json` and verify rust-analyzer functions correctly (goto definition, completions).
  - Compare clean and incremental build times: `buck2` vs `cargo`.
- [ ] **0.4.5 Spike Conclusion & Decision**:
  - Document findings in `DESIGN.md` changelog.
  - Execute Keep / Expand / Delete decision: if deleted, purge all BUCK files cleanly.

---

## Phase 0 Milestones & Sequencing

```
Week 1                                          Week 2
[0.0 Workspace Baseline: COMPLETE]
   +---> [0.1 Slint Android Spike: IN PROGRESS] -> [Slint Go/No-Go Review]
   +---> [0.2 Iroh Android Spike] -------------> [Iroh Go/No-Go Review]
   +---> [0.3 CLI serve Spike: COMPLETE (GO)] -> [Async Convergence Review]
   +---> [0.4 Buck2 Spike (2 days max)] -------> [Buck2 Keep/Delete Decision]
                                                       |
                                               Phase 0 Exit Gate
                                                       |
                                                       v
                                            Phase 1 Kickoff
```

1. **Days 1–2:** Complete 0.0 (Workspace scaffolding, toolchains, justfile) [DONE] + start 0.4 (Buck2 spike 2-day clock).
2. **Days 3–6:** Execute 0.1 (Slint Android APK, IME, gestures, TalkBack) [IN PROGRESS] and 0.2 (Iroh on Android LAN & Relay).
3. **Days 7–8:** Execute 0.3 (CLI `serve` 3-node async convergence test) [COMPLETE - GO] + close out 0.4 (Buck2 decision).
4. **Days 9–10:** Consolidate spike findings, run formal Go/No-Go evaluation, update [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) changelog, and prepare Phase 1 implementation tickets.

---

## Exit Deliverables for Phase 0

1. **Android Slint Prototype APK**: Validated on physical reference phone with positive TalkBack, IME, and touch interaction sign-off.
2. **Iroh Mobile Sync Report**: Confirmed LAN mDNS and WAN Relay connectivity, with zero redb corruptions across forced terminations.
3. **Working `grocery serve` Prototype**: Automated 3-node test demonstrating asynchronous relaying between intermittent peers. [COMPLETE - GO]
4. **Buck2 Evaluation Record**: Written recommendation in `DESIGN.md` (Keep, Expand, or Clean Removal).
5. **Phase 1 Readiness Sign-off**: Core data structures and sync interfaces ready for production implementation.
