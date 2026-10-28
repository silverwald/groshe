# Groshe (Гроше)

Groshe is a local-first, peer-to-peer shared grocery list system built in Rust with **Iroh** (`iroh`, `iroh-docs`, `iroh-blobs`, `iroh-gossip`) and **Slint**. It is designed for offline-first resilience, end-to-end encryption, privacy, and zero mandatory cloud infrastructure.

---

## Key Features

- **Peer-to-Peer CRDT Synchronization**: Synchronizes directly over LAN (mDNS) or encrypted Iroh relays with Last-Writer-Wins (LWW) conflict resolution per field (`meta`, `item/<id>/data`, `item/<id>/check`, `item/<id>/pos`) and 30-day tombstone retention for undo.
- **Household & Multi-List Grouping**: Share an entire family's lists (`Weekly Groceries`, `Costco Run`, `Hardware Store`) with a single Household ticket. New lists and rotated lists automatically propagate to all household members.
- **In-Store Mode & Live Peer Presence**: Ephemeral presence heartbeats (45s TTL) surface real-time `"🛒 <Peer> shopping now"` and `"● <Peer> viewing"` indicators across Mobile and Web.
- **Inline Shorthand Quick-Add Grammar**: Parse quantities, units, fractions, and categories inline (e.g., `2 gal organic milk @Dairy`, `1/2 lb coffee beans @Pantry`, `apples 3 lbs`).
- **Fractional Indexing & Drag-to-Reorder**: Reorder items arbitrarily via HTML5 drag-and-drop (Web), Up/Down controls (Mobile), or `--before`/`--after`/`--top`/`--bottom` flags (CLI) without renumbering neighboring items.
- **Photo Attachments & Frequent Suggestions**: Attach content-addressed item thumbnails (`<256KB` via `iroh-blobs`) and surface recency-weighted frequent item suggestions from local shopping history.
- **Capability Rotation & Member Revocation**: Rotate individual list capabilities or rotate an entire Household while evicting specific member author IDs.

---

## Workspace Architecture

| Crate / App | Path | Description |
| :--- | :--- | :--- |
| `groshe-core` | `crates/core` | Pure-Rust domain models (`ListMeta`, `ItemData`, `ItemCheck`, `HouseholdMeta`, `HouseholdMember`, `HouseholdListRef`), quick-add parser, fractional indexing (`ItemPosition`), and CBOR schemas (`DocKey`, `HouseholdDocKey`). |
| `groshe-store` | `crates/store` | Materialized in-memory projections (`ListProjection`, `HouseholdProjection`), LWW resolution, 30-day tombstone undo, and recency-decayed `ItemHistoryTracker`. |
| `groshe-sync` | `crates/sync` | `GrosheSyncNode` managing Iroh `Endpoint`, `iroh-docs`, `iroh-blobs`, `iroh-gossip`, `PresenceTracker`, OS Keychain / file identity storage, QR rendering, and household reconciliation. |
| `groshe-cli` | `crates/cli` | `grocery` full-peer CLI binary and headless `serve --web` daemon. |
| `groshe-web` | `apps/web` | Embedded Axum REST & WebSocket Web Companion served by an anchor peer. |
| `groshe-android` | `apps/android` | Slint mobile UI (`ListsScreen`, `DetailScreen`, `InStoreScreen`, `ShareJoinScreen`, `SettingsScreen`) and Android JNI runtime. |
| `groshe-ios` | `apps/ios` | iOS staticlib C-FFI coordinator and lifecycle manager with Apple Keychain integration. |

---

## CLI & Web Companion Usage

```bash
# 1. Initialize local device identity and node keys
grocery init

# 2. Create or join a shared Household (auto-discovers all household lists)
grocery household create "Smith Family"
grocery household share
grocery household join <household-ticket>

# 3. Create and manage grocery lists
grocery list create "Weekly Groceries"
grocery list ls

# 4. Add, edit, check, and reorder items
grocery add "2 gal organic milk @Dairy" --note " whole or 2%"
grocery edit "organic milk" --qty 3 --unit "half-gal"
grocery pos "organic milk" --top
grocery check "organic milk"
grocery ls --by-category

# 5. Suggestions, undo, and backups
grocery suggest --add-top
grocery undo
grocery export --format markdown --output groceries.md

# 6. Rotate a list or household to revoke access
grocery list rotate
grocery household rotate --evict <revoked-author-prefix>

# 7. Launch always-on sync daemon with embedded Web Companion on port 8080
grocery serve --web 127.0.0.1:8080
```

Open `http://127.0.0.1:8080` in any desktop or mobile browser to switch households, create lists, add/edit/reorder items via drag-and-drop, toggle **In-Store Mode**, and see live peer presence in real time.

---

## Web Companion HTTP & WebSocket API

| Endpoint | Method | Description |
| :--- | :--- | :--- |
| `/api/status` | `GET` | Node ID, Author ID, relay mode, and local list count. |
| `/api/households` | `GET`, `POST` | List local households or create a new household. |
| `/api/households/join` | `POST` | Join a household by `DocTicket` and auto-import its lists. |
| `/api/households/:id/lists` | `POST` | Create or register a list inside a household. |
| `/api/households/:id/rotate` | `POST` | Rotate a household and all its active lists (with optional `evicted_authors`). |
| `/api/lists` | `GET` | List all active grocery lists (after reconciling households). |
| `/api/lists/:id` | `GET` | Full list projection including sorted items and `active_peers`. |
| `/api/lists/:id/rotate` | `POST` | Rotate list capability (auto-updating any owning household). |
| `/api/lists/:id/presence` | `POST` | Announce local presence (`in_store`, `author_name`) and return active peers. |
| `/api/lists/:id/items` | `POST` | Add an item using inline quick-add syntax and optional base64 thumbnail. |
| `/api/lists/:id/items/:item_id` | `PATCH`, `DELETE` | Edit an item in place or delete (tombstone) an item. |
| `/api/lists/:id/pos/:item_id` | `POST` | Reorder an item (`direction`: `up`/`down`/`top`/`bottom`, `before_id`, or `after_id`). |
| `/api/lists/:id/check/:item_id` | `POST` | Toggle item checked state. |
| `/api/photos/:hash` | `GET` | Fetch a 32-byte Blake3 photo thumbnail from `iroh-blobs`. |
| `/api/ws/:id` | `GET` (WS) | Real-time WebSocket stream emitting `initial` and `update` snapshots (`items` + `active_peers`). |

---

## Development & Verification

```bash
# Lint all workspace crates and targets with zero warnings
cargo clippy --workspace --all-targets -- -D warnings

# Run all unit, property, API, and multi-node E2E convergence tests
cargo test --workspace -j 2 -- --test-threads=1
```
