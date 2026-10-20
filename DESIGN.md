# Shared Grocery List App — Design Doc

- **Status:** Phase 1, 2, 4, and 5 complete; proceeding to Phase 3 (iOS)
- **Date:** 2026-10-14
- **Platform priority:** Android (primary), iOS, CLI (full peer), Web (companion)
- **Language goal:** As close to pure Rust as practical across all platforms

## 1. Product Summary

A shared grocery list app that syncs peer-to-peer with no application server as a
middleman. Members of a list can add, edit, check off, reorder, and delete items
while offline; changes converge automatically when peers reconnect. There are no
accounts and no cloud copy of list data held by the project — every full peer
holds its own replica.

## 2. Goals and Non-Goals

### Goals (v1)
- Multiple named grocery lists, shared per list.
- Real-time-ish convergence between members who are online at the same time,
  and eventual convergence for everyone else.
- Full offline use on Android, iOS, and CLI.
- Sharing by QR code / link (a list ticket), no sign-up.
- In-store usability: large touch targets, one-handed operation, fast checking.

### Non-Goals (v1)
- No accounts, passwords, or email-based discovery.
- No cloud backend, push-notification service, or analytics.
- No household/group abstraction (planned as a future iteration — see §11).
- Web is not a full peer (see §9).
- No recipes, price tracking, barcode scanning, or store integrations.

## 3. Decisions Made

| Decision | Choice | Notes |
|---|---|---|
| Primary UI framework | **Slint** | GPUI rejected (see §13). Slint's licensing model (GPL / royalty-free / commercial) is acceptable; the exact license route must be picked before public distribution. |
| CRDT / sync layer | **iroh-docs first** | iroh-docs is itself an eventually-consistent CRDT. Do not stack a second CRDT (Loro/Automerge) underneath in v1. Loro is the documented escape hatch (see §6.4). |
| P2P transport | **Iroh** | Endpoint + Router; iroh-docs for sync, iroh-gossip for presence/live fan-out, iroh-blobs later for photos. |
| Build system | **Cargo is canonical** | Single source of truth for all crates and platforms. Plus a time-boxed, non-gating Buck2 spike for `crates/core` only (see §12). Bazel/Buck2 as primary build system rejected (see §13). |
| Web role | **Companion only in v1** | Live-session client; not a durable replica, no offline editing. |
| CLI role | **Full peer** | First-class product surface, including a headless `serve` mode. |
| Sharing unit | **Per list** | One iroh-doc namespace per list. Household concepts are a future iteration (see §11). |

## 4. Architecture

Cargo workspace. The core rule: `core`, `sync`, and `store` never import any UI
code. The UI framework decision stays reversible and the CRDT logic stays
testable headless.

```
crates/
  core/    # Pure Rust, no I/O. Domain: List, Item, Category, quantity/unit,
           # validation, fractional-index ordering. Minimal deps only
           # (serde / uuid / thiserror class — no build.rs, no C deps; this
           # constraint is also what keeps the Buck2 spike feasible).
  sync/    # Iroh Endpoint + Router, iroh-docs replica management, gossip
           # presence, ticket create/parse, discovery, relay configuration.
  store/   # Persistence and the materialized projection the frontends read.
           # Owns on-disk replica storage (redb, via iroh-docs) plus any local
           # query index. Frontends never query Iroh directly in a render loop.
  cli/     # Full-peer CLI binary (see §8). Depends on core + sync + store.
apps/
  android/ # Slint app packaged for Android (cargo-ndk + Gradle).
  ios/     # Slint app packaged for iOS (Xcode project around the same libs).
  web/     # WASM companion client (see §9).
```

Frontends (Slint app, CLI, web) are thin: they translate user input into core
operations and render the store projection, which is updated live from
iroh-docs subscription events (`LiveEvent::InsertRemote`, `ContentReady`, etc.).

## 5. Sync Design (Iroh)

- One Iroh `Endpoint` per device, one `Router` multiplexing protocols by ALPN:
  iroh-docs sync, iroh-gossip, iroh-blobs (later).
- Discovery: mDNS on the local network; pkarr/DHT-based discovery for remote
  peers; relay fallback when direct QUIC hole-punching fails.
- Presence: gossip topic derived from the list's namespace. Used for "who else
  is online / in-store" and live change fan-out; correctness never depends on
  gossip — docs set reconciliation is the source of convergence.
- Mobile lifecycle: sync on app open / foreground and opportunistically while
  open. Do not attempt a persistent always-on QUIC connection on mobile;
  Android doze/battery policy and iOS background limits make it unreliable.
  iOS expectation for v1 is explicitly "syncs when open."

### 5.1 The honest "no server" caveat
"No app server" does not mean "no infrastructure":
- When hole-punching fails, native peers fall back to relay servers. Browser
  peers are relay-only by necessity, because browsers cannot send UDP packets.
  Relay traffic is end-to-end encrypted; a relay cannot read list contents.
- Default to public (n0) relays/discovery presets; the relay URL must be
  configurable and self-hostable for users who want zero third-party infra.
- If no member peer for a list is online, nothing syncs for that list. This is
  inherent to a serverless design. The supported mitigation is a user-run
  always-on peer — most naturally the CLI in `serve` mode (§8) on a desktop,
  home server, or Raspberry Pi.

## 6. Data and CRDT Design

### 6.1 Why iroh-docs alone
An iroh-docs document is a multi-writer, eventually-consistent key-value
replica. Entries are keyed by (namespace, author, key); concurrent writes to
the same key resolve last-writer-wins (timestamp, then content hash); deletion
is represented by an empty entry, i.e. a signed, replicated tombstone. Peers
converge via range-based set reconciliation, whose cost is proportional to the
difference between replicas, not their size.

Stacking Loro or Automerge on top in v1 would require reliably transporting a
second CRDT's updates (version vectors, missed updates, compaction), which
duplicates what iroh-docs reconciliation already does. Grocery lists do not
need collaborative text merging.

### 6.2 One namespace per list
Each shared list is one iroh-docs namespace (document). Sharing, membership,
size, and revocation all scope to a single list.

Entry layout (values are CBOR/serde-encoded structs):
- `meta` → list title, category/aisle ordering, schema version, created_by.
- `item/<uuidv7>/data` → name, quantity, unit, category, note.
- `item/<uuidv7>/check` → checked flag, checked_at, checked_by.
- `item/<uuidv7>/pos` → fractional-index position string (manual ordering).

The split into `data` / `check` / `pos` keys is deliberate: checking an item,
editing its quantity, and reordering it are the three most common concurrent
operations, and as separate keys they do not last-writer-wins-clobber each
other.

IDs are UUIDv7 (time-ordered, globally unique) — see also the household
forward-compatibility rules in §11.

### 6.3 Accepted v1 conflict semantics
- Concurrent edits to the *same field* of the same item: last writer wins.
  Acceptable for groceries; noted in user-facing copy nowhere, but documented
  here as a known limitation.
- Deletes win by tombstone; a locally maintained 30-day "recently deleted"
  view provides undo at the UX level.
- Ordering conflicts: fractional indexes may interleave; items remain usable
  and can be re-dragged. No total ordering guarantee is promised.

### 6.4 Escape hatch
If real use shows same-field or ordering conflicts are painful, evaluate Loro
for the item list only (its movable-list type fits reordering). The `sync`
crate boundary (a trait over replica operations) exists so this change does
not touch `core` or the frontends.

## 7. Identity, Sharing, and Security

- Identity: one author keypair per device/install. Stored in Android Keystore,
  iOS Keychain, OS-protected file with strict permissions for the CLI, and
  WebCrypto for the web companion (weakest of the four — one more reason web
  is not a full peer).
- Joining: the list owner displays a QR code / link containing a DocTicket
  for that list's namespace. Scanning/opening it joins the list. No directory
  or account lookup exists.
- Threat model (v1): anyone holding a list's write ticket can read and write
  that list. Connections are end-to-end encrypted and entries are
  author-signed, but content is not additionally encrypted against other list
  members. Membership is an abstraction in code, not hardcoded assumptions,
  so richer access control can be added later.
- Revocation: iroh-docs write capability is effectively all-or-nothing per
  namespace, so removing a member means *rotation*: create a new namespace,
  copy current state, re-share tickets to remaining members, stop syncing the
  old namespace. This "rotate list" flow is a v1 feature, not an afterthought,
  in Android, iOS, and the CLI.

## 8. CLI — Full Peer (v1)

The CLI ships in Phase 1 and is a product surface, not a test harness.

- It holds its own identity and persistent replica, works fully offline, and
  converges exactly like a phone. It can create, join, share, and rotate lists.
- Indicative command surface:
  - `grocery init` — create/load device identity
  - `grocery list create|ls|join|share|rotate|rm`
  - `grocery add "<name>" [--qty N] [--unit u] [--list L]`
  - `grocery check|uncheck|rm <item>` , `grocery ls [--list L]` (show items)
  - `grocery peers` — known/connected peers and sync state per list
  - `grocery export|import` — human-readable (JSON/text) backup of a list
  - `grocery serve` — headless daemon: stay online, hold replicas, sync
    opportunistically; suitable for systemd/launchd on a home machine
- `serve` is the recommended always-on peer (§5.1) and the anchor the web
  companion connects to (§9).
- The CLI doubles as the integration-test driver for two-peer convergence
  tests (§14).

## 9. Web — Companion Only (v1)

Definition, concretely:
- The web client participates in a *live session only*, over relay (browsers
  cannot open UDP sockets, so direct connections and hole-punching are
  impossible from a browser; all browser traffic is relayed, end-to-end
  encrypted).
- It is usable while at least one full peer for that list (phone or CLI,
  typically a CLI in `serve` mode) is online. In session it can view the list
  and add/check items.
- It makes no durability promise: no offline editing, no guarantee of holding
  a complete persistent replica after the tab closes. With no peer online, it
  may show only its last cached snapshot, read-only, clearly labelled stale.
- Consequence: v1 web does not depend on iroh-docs persistence working
  perfectly in WASM/IndexedDB — the riskiest unknown in the original plan.
  Promoting web to a full peer is a post-v1 investigation, gated on a spike
  into WASM iroh-docs storage (IndexedDB/OPFS) and background behaviour.

## 10. Mobile UI (Slint)

Screens:
1. **Lists** — list cards with peer/sync status and last activity.
2. **List detail** — grouped by category/aisle, unchecked items first, large
   check targets, swipe-to-delete with undo, drag to reorder.
3. **Add item** — sticky input, recent/frequent chips, quantity stepper + unit.
4. **In-store mode** — larger type, keep-screen-awake, checked items collapse.
5. **Share / peers** — QR code and ticket link, member/peer list, rotate list.
6. **Settings** — identity, relay configuration, export/backup.

Guidelines:
- Material 3 semantics on Android (Slint widgets themed to match, not
  pixel-perfect Material). System font. Dark mode from day one.
- Every item row must be fully operable one-handed.
- UI is written in Slint's `.slint` DSL (AOT-compiled); all logic stays in Rust.
- Accessibility gate: usable with TalkBack (Android) and VoiceOver (iOS),
  minimum 48dp touch targets, working soft-keyboard/IME behaviour for add/edit.
  These are Phase 0/2 exit criteria, not polish items (§15).

## 11. Sharing Model: Per-List Now, Household Later

v1 shares individual lists only. Sharing five lists means five tickets; that
UX cost is accepted for v1.

Forward-compatibility rules so the household iteration is additive, not a
migration:
1. Globally unique UUIDv7 IDs for lists, items, and any future grouping.
2. Versioned `meta` schema on every list document from day one.
3. Membership/authorization kept behind an abstraction in `sync`/`core`,
   never baked into the item model.

Future household design (sketch, not v1 scope): a household is a *separate*
index document holding a shared roster and references (namespace IDs) to
member lists. Joining a household auto-shares its current and future lists.
Existing per-list documents are not merged or rewritten.

## 12. Build System: Cargo + Buck2 Spike

### 12.1 Cargo (canonical)
- All crates, all platforms, all releases build with Cargo. Nothing ships
  that can only be built another way.
- Reproducibility: committed `Cargo.lock`, pinned toolchain in
  `rust-toolchain.toml`, builds run `--locked`; `cargo-deny` in CI for
  licenses/advisories (this is also where the Slint license choice is
  enforced); sccache / CI cache for speed.
- Keep the workspace Buck-friendly at zero cost: small crates, explicit
  minimal dependencies, no gratuitous `build.rs` in first-party crates.

### 12.2 Buck2 spike (time-boxed, non-gating)
- **Scope:** `//crates/core` only — `buck2 build` and `buck2 test`, host
  Linux/macOS only. No Android/iOS/WASM cross-compilation, no Iroh, no Slint.
- **Core dependency constraint:** pure, no-`build.rs`, no C dependencies
  (serde/uuid/thiserror class) so the spike measures Buck2, not fixups.
- **Setup in scope:** Buck2 via dotslash, `.buckconfig` and Rust toolchain,
  Reindeer-generated third-party BUCK files, generated `rust-project.json`
  so rust-analyzer works.
- **Time box:** 2 days of effort, hard stop. Stop early if per-crate fixups
  beyond core's small dep set are needed, or if Buck2 dependency versions
  would have to diverge from `Cargo.lock` .
- **Non-gating:** separate, non-required CI job. A failing Buck2 job never
  blocks a merge or release.
- **Exit criteria:** clean-checkout build + tests pass, incremental rebuild
  not worse than Cargo, IDE works. Ends with an explicit keep / expand /
  delete decision recorded in this doc's changelog. If deleted, the BUCK
  files are deleted with it — no zombie build system.
- **Evaluation Outcome & Decision (2026-10-15):** Concluded with **DELETE**.
  - *Build Performance:* Cargo demonstrated faster incremental (455ms vs 885ms zero-work) and single-crate clean builds (517ms vs 2011ms).
  - *Host Compatibility:* Buck2 relies on symlinks during action staging, failing on Windows with OS Error 1314 unless Windows Developer Mode is enabled or elevated; Cargo requires no special OS privileges.
  - *Maintenance Overhead:* Even on pure-Rust `crates/core`, transitive dependencies (`serde_core 1.0`, `thiserror 2.0`, `serde_json 1.0`) required manual Reindeer build script fixups (`[buildscript] run = true` and `cargo_env = true`).
  - *IDE Ergonomics:* Required generating synthetic `rust-project.json` files via Buck2 BXL queries instead of native Cargo manifest discovery.
  - *Action:* Purged all BUCK and Reindeer configuration files per §12.2 ("no zombie build system"); Cargo remains the sole canonical build toolchain.

### 12.3 Revisit triggers for a primary non-Cargo build system
Re-evaluate (Bazel first, for its larger open-source Rust/Android/Apple rule
ecosystem) only if: more than ~5 regular contributors, CI build time over
~20 minutes despite caching, or substantial first-party Kotlin/Swift/C++
appears.

## 13. Alternatives Considered and Rejected

- **GPUI (as primary UI):** Zed's framework; upstream targets desktop
  (macOS/Linux/Windows). Android/iOS/Web support exists only as experimental
  community-fork work, the API is git-pinned and fast-breaking, and a fork
  split (upstream / CE / gpui-kit) makes it a risky foundation for an
  Android-primary product. Because `core`/`sync` are UI-agnostic, a GPUI
  desktop shell remains possible later at low cost.
- **Dioxus:** strong web story (real DOM) and in-Rust RSX, but mobile is a
  WebView; fallback only if web is ever promoted to co-primary.
- **Makepad:** proven across all platforms but a small community and sparse
  documentation. **egui:** fast to prototype, weak accessibility/native feel
  for a consumer app (retained as an option for internal tools only).
  **Tauri 2:** viable, but moves UI to web tech and gives up the pure-Rust UI.
- **Bazel or Buck2 as the primary build system:** both derive their Rust
  graphs from Cargo (crate_universe / Reindeer), so Cargo never goes away —
  a second, hand-maintained graph is added. The Iroh/Slint stack is heavy in
  `build.rs` crates and proc macros (serde/tokio derives, ring, slint-build),
  each needing per-crate fixups/annotations; cross-compilation requires
  declaring host (exec) and target triples per platform including Cargo
  feature differences (e.g. Iroh `default-features = false` on WASM); Gradle
  and Xcode packaging are not eliminated; rust-analyzer needs generated
  project files. The benefits (hermeticity, remote execution, unified
  multi-language graph) pay off at a scale this project is not at.
- **Loro/Automerge as the primary CRDT under Iroh:** rejected for v1 (see
  §6.1/§6.4); kept as the documented escape hatch.

## 14. Development Setup and Testing

- Toolchain: stable Rust, pinned via `rust-toolchain.toml`. Targets:
  `aarch64-linux-android`, `x86_64-linux-android` (emulator),
  `aarch64-apple-ios` (+ simulator), `wasm32-unknown-unknown`, host targets.
- Platform tooling: Android SDK/NDK via Android Studio + `cargo-ndk`;
  Xcode on macOS for iOS; `wasm-bindgen-cli` + `wasm-opt` for web (note:
  `wasm-pack` is archived — call cargo + wasm-bindgen directly);
  `just` as the task runner.
- CI (GitHub Actions): core/sync/store tests on Linux; Android build on
  Linux; iOS build on a macOS runner; WASM build check; Buck2 spike job
  (non-required, §12.2). One physical Android device is the reference device;
  no device farm in v1.
- Testing strategy — convergence is the product:
  - Property tests (`proptest`): generate random item operations on 2–3
    simulated replicas, shuffle/drop/delay delivery, assert all replicas
    converge to identical state.
  - In-process two-endpoint Iroh integration tests, plus CLI-driven
    two-peer tests (§8).
  - Store/projection tests from recorded doc event streams.
  - UI testing is manual against the Phase 0/2 accessibility checklist (§10).

## 15. Phased Plan

- **Phase 0 — De-risking spikes (all go/no-go):**
  1. Slint on a real Android phone: the §10 screen flows, IME, swipe, reorder,
     TalkBack, dark mode, cold start.
  2. Iroh on Android: two phones syncing a doc over LAN and over relay
     (NAT/firewall), including app kill/restart.
  3. CLI `serve` as an always-on peer for a phone.
  4. Bound separately: the Buck2 spike (§12.2) runs inside Phase 0–1 and
     does not gate any of the above.
- **Phase 1 — Core + sync + CLI full peer.** Domain, iroh-docs model,
  identity/tickets/rotation, CLI per §8, convergence tests green.
- **Phase 2 — Android MVP.** Slint app: lists, list detail, add, in-store
  mode, share/rotate, settings.
- **Phase 3 — iOS.** Same core/UI, Keychain identity, foreground-sync
  expectations, TestFlight distribution.
- **Phase 4 — Web companion.** Per the §9 definition, anchored by an online
  full peer.
- **Phase 5 — Hardening and extras.** Backup/export polish, item photos via
  iroh-blobs, frequent-item suggestions from history, self-hosted relay
  documentation.
- **Future iteration — Household.** Per the §11 sketch.

## 16. Risks and Open Items

- **Web promotion** beyond companion status depends on WASM iroh-docs
  persistence (IndexedDB/OPFS) and browser background behaviour; deliberately
  out of v1 scope (§9).
- **Relay dependence:** NAT'd and all browser traffic transits relays. Mitigated
  by end-to-end encryption and self-hostable relay configuration; residual
  risk is relay availability/metadata, accepted for v1.
- **Revocation UX:** rotation (§7) is disruptive (new tickets for everyone).
  Accepted for v1; household iteration should soften it.
- **Slint license route** (GPL vs royalty-free vs commercial) must be chosen
  before public distribution; `cargo-deny` will enforce the choice.
- **iOS background sync** is limited by the platform; v1 sets expectations
  rather than fighting it with background modes.
- **Open:** product name, minimum Android/iOS versions, and whether a
  self-hosted relay is offered by the project or strictly user-run.

## Changelog

- 2026-10-18 — Phase 4 completed: Web companion (`apps/web`) with Axum REST API, WebSocket live subscription streaming, responsive Material 3 single-page web app, and `grocery serve --web` integration. All 47 workspace tests pass.
- 2026-10-18 — Phase 5 completed: Hardening & extras (frequent-item suggestions with recency decay scoring, full JSON backup & Markdown export/import, item photo attachments via iroh-blobs with 256KB cap, configurable self-hosted relay support in `groshe.toml` with setup documentation).
- 2026-10-17 — Phase 2 completed: Android MVP with Slint UI architecture (lists, detail, in-store, add, settings), reactive sync bridge, quick-add parser, Android JNI haptic feedback, release packaging, and emulator automation.
- 2026-10-16 — Phase 1 completed: Core domain engine (CBOR serialization, fractional position ordering), Reactive projection store (LWW conflict resolution, tombstone retention, event broadcasting), Iroh sync engine (identity persistence, doc tickets, QR codes, gossip presence, list rotation), full-peer CLI binary (`grocery`), CRDT property testing suite (`proptest`), and 3-peer convergence integration verification. Ready for Phase 2 (Android MVP).
- 2026-10-15 — Spike 0.4 completed on `crates/core`. Buck2 build, test, and `rust-project.json` generation verified. Performance (Cargo 455ms vs Buck2 885ms incremental) and Windows Developer Mode symlink friction evaluated. Decision: **DELETE** (all BUCK files removed; no zombie build system). Cargo remains canonical.
- 2026-10-14 — Initial version. Decisions recorded: no GPUI; Slint UI;
  iroh-docs as the CRDT; Cargo canonical with a time-boxed Buck2 spike on
  `crates/core` only; web companion-only in v1; CLI as full peer; per-list
  sharing with household as a future iteration.
