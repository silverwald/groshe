# Phase 3 Detailed Execution Plan: iOS Platform & Keychain Identity

**Status:** Complete (Milestones 3.1, 3.2, 3.3 & 3.4 Complete)  
**Context:** [DESIGN.md](file:///C:/projects/groshe/DESIGN.md) §4 (Architecture), §7 (Identity & Security), §10 (Mobile UI), §14 (Development & Tooling), §15 (Phase 3)  
**Target Branch:** `feature/phase-3-ios-support`  

---

## Executive Summary

Phase 1 established the pure Rust domain, reactive store projection, Iroh P2P sync, and full-peer CLI.
Phase 2 delivered the Slint Android MVP with mobile lifecycle and JNI runtime.
Phase 5 brought frequent-item suggestions, backup/export, item photos, and self-hosted relay hardening.
Phase 4 delivered the live web companion SPA and WebSocket streaming daemon.

**Phase 3** expands Groshe to Apple platforms (iOS), satisfying the platform requirements documented in DESIGN.md:
1. **Keychain Identity & Keyring Abstraction (`crates/sync`)**:
   - Secure storage of the device Iroh `SecretKey` and docs `AuthorId` using the Apple Keychain Services API (`kSecClassGenericPassword`), protected by device passcode / biometric hardware.
   - Pluggable `IdentityStorage` trait supporting file-based (CLI/desktop), mock (test environments), and Keychain (iOS/macOS).
2. **iOS C-FFI & Swift/Cocoa Runtime Bridge (`apps/ios`)**:
   - C-ABI entry points (`groshe_ios_init`, `groshe_ios_run`, `groshe_ios_on_foreground`, `groshe_ios_on_background`, `groshe_ios_cleanup`) allowing native Swift/SwiftUI and UIKit `AppDelegate`/`SceneDelegate` lifecycle management.
   - C headers (`groshe_ios.h`) and static library packaging (`libgroshe_ios.a`).
3. **Shared Slint UI Mobile Experience**:
   - Reusable Slint mobile components adhering to Apple Human Interface Guidelines: iOS system typography, 48pt touch targets, VoiceOver accessibility semantics, and fluid dark/light adaptation.
4. **Foreground-Only Sync Model**:
   - Opportunistic document sync and presence advertisement when the app transitions into the foreground.
   - Clean socket flush and checkpoint persistence when entering the background, fully respecting iOS 30-second background execution constraints.
5. **Xcode Integration Scaffolding & Tooling**:
   - Complete Xcode project template (`apps/ios/xcode/`) and Swift runner.
   - Justfile recipes for cross-compiling to `aarch64-apple-ios` and simulator targets.

---

## Milestones Breakdown

### Milestone 3.1: Keychain Identity & Secure Keyring Architecture (`crates/sync`) (COMPLETE)
**Goal:** Deliver hardware-backed identity security on Apple platforms while preserving zero-overhead testability.
- [x] **3.1.1 `IdentityStorage` Abstraction**:
  - Trait `IdentityStorage: Send + Sync` defining `load_secret_key()`, `save_secret_key()`, `load_author_id()`, and `save_author_id()`.
  - Implement `FileIdentityStorage` retaining current permissions-hardened filesystem storage for CLI/daemon and development environments.
  - Implement `KeychainIdentityStorage` utilizing `security-framework` on Apple targets (`cfg(target_vendor = "apple")`), and an in-memory / secure mock fallback on other platforms.
- [x] **3.1.2 Refactor `DeviceIdentity`**:
  - Update `DeviceIdentity` to support arbitrary `IdentityStorage` backends.
  - Retain backwards-compatible `DeviceIdentity::load_or_create(dir)` for CLI and Android.
  - Add `DeviceIdentity::load_or_create_with_storage(storage)`.
- [x] **3.1.3 Unit & Integration Tests**:
  - Unit tests verifying mock keychain storage, secret key roundtrip, author ID update, and corruption resilience.

### Milestone 3.2: iOS Shared App Scaffolding & C-FFI Bridge (`apps/ios`) (COMPLETE)
**Goal:** Provide an idiomatic, high-performance static library integration for iOS applications.
- [x] **3.2.1 `apps/ios` Cargo Crate Scaffolding**:
  - Manifest configured with `crate-type = ["staticlib", "cdylib", "rlib"]`.
  - Workspace inclusion in root `Cargo.toml`.
  - Shared Slint UI compilation bridging `apps/android/ui/app.slint`.
- [x] **3.2.2 C FFI Header & Runtime Coordinator**:
  - Export C symbols in `apps/ios/src/lib.rs` and generate/provide `apps/ios/include/groshe_ios.h`.
  - Runtime coordinator (`apps/ios/src/ios_runtime.rs`) managing the Tokio runtime on a dedicated background thread pool.
  - Sandboxed path resolver extracting `NSApplicationSupportDirectory` and `NSDocumentDirectory`.

### Milestone 3.3: Mobile Lifecycle & Foreground Sync Guard (COMPLETE)
**Goal:** Guarantee deterministic convergence under iOS background execution limitations.
- [x] **3.3.1 Lifecycle Hooks**:
  - `groshe_ios_on_foreground()`: Wake up sync engine, re-establish QUIC connections to known relays/peers, advertise gossip presence.
  - `groshe_ios_on_background()`: Gracefully close active QUIC streams, flush redb and history buffers, pause gossip heartbeats.
- [x] **3.3.2 Integration Test for iOS Lifecycle Protocol**:
  - In-process test simulating foreground wake -> mutation -> background sleep -> foreground resume cycle.

### Milestone 3.4: Xcode Project Scaffolding & Developer Ergonomics (COMPLETE)
**Goal:** Turnkey setup for building and testing Groshe on iOS devices and simulators.
- [x] **3.4.1 Swift Application Template**:
  - `apps/ios/xcode/` containing Swift app wrapper, Bridging Header, and SwiftUI container.
- [x] **3.4.2 Cross-Compilation Justfile Commands**:
  - `just ios-build` compiling staticlib for `aarch64-apple-ios`.
  - `just ios-test` running lifecycle and FFI test suite.
  - `just ios-check` running static type verification.

---

## Commit & Quality Guidelines
- Pure Rust domain separation preserved.
- Zero compiler warnings (`-D warnings`).
- 100% test pass rate across all workspace crates (49/49 tests passing).
