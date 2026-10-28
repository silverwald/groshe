# Implementation Plan: Household-Aware List Rotation & Member Revocation

## 1. Objective & Scope (`DESIGN.md` §7, §11, §16)
In `v1`, rotating a list (`rotate_list`) created a new document namespace and required every participant to scan a new `DocTicket`. As noted in `DESIGN.md` §16:
> *"Revocation UX: rotation (§7) is disruptive (new tickets for everyone). Accepted for v1; household iteration should soften it."*

This iteration implements two complementary revocation flows:
1. **Seamless Per-List Rotation Within a Household:** When any list belonging to a Household is rotated (`GrosheSyncNode::rotate_list`), the Household document is automatically updated: the old list entry (`household/lists/<old_id>`) is marked `archived = true`, and the newly rotated list (`household/lists/<new_id>`) is registered with its new `DocTicket`. During `reconcile_household_lists`, household members automatically import the new list and drop the archived old replica—requiring **zero ticket re-sharing** among household members.
2. **Full Household Rotation & Member Eviction (`GrosheSyncNode::rotate_household`):** When a member must be evicted from the Household itself, `rotate_household` creates a new Household namespace (excluding evicted `author_id`s), rotates every active list in the household into fresh namespaces, registers the new list tickets in the new Household document, and drops the old household and old list replicas. Remaining members scan a single new Household ticket to migrate all lists at once.

---

## 2. Phased Implementation

### Phase 1: Sync Engine Household-Aware List Rotation & Full Household Rotation (`crates/sync`)
1. **`GrosheSyncNode::rotate_list` (`crates/sync/src/lib.rs`)**:
   - After migrating active entries from `old_id` to `new_doc` (`new_id`, `new_ticket`) and dropping `old_id`:
   - Inspect all local households (`self.household_ids().await`).
   - For each household whose `get_household_lists` contains `old_id.to_string()`:
     - Mark `household/lists/<old_id>` as `archived = true`.
     - Register `new_doc` via `self.register_list_in_household(&hh_doc, &new_doc, &title).await`.
2. **`GrosheSyncNode::reconcile_household_lists` (`crates/sync/src/lib.rs`)**:
   - When iterating over `list_refs` from `household_doc`:
     - If `lref.archived` is `true` and `existing_ids.contains(&ns_id)`, close and drop `ns_id` from local storage (`self.delete_list(ns_id).await`) so revoked/rotated old lists are cleaned up automatically across all household peers.
3. **`GrosheSyncNode::rotate_household` (`crates/sync/src/lib.rs`)**:
   - Signature:
     ```rust
     pub async fn rotate_household(
         &self,
         old_hh_id: NamespaceId,
         author_name: &str,
         evicted_authors: &[String],
     ) -> Result<(Doc, DocTicket, NamespaceId), SyncError>
     ```
   - Reads `HouseholdMeta`, `HouseholdMember`s, and `HouseholdListRef`s from `old_hh_id`.
   - Creates a new Household document (`new_hh_doc`, `new_hh_ticket`, `new_hh_id`) with the existing household name.
   - Copies over all `HouseholdMember`s whose `author_id` is not in `evicted_authors` (matching exact ID or prefix).
   - For each non-archived `HouseholdListRef`:
     - Opens or imports the list, calls `self.rotate_list(list_ns, author_name).await` to migrate items to a fresh list namespace, and registers the new list in `new_hh_doc`.
   - Drops `old_hh_id` from local storage and returns `(new_hh_doc, new_hh_ticket, new_hh_id)`.
4. **Verification**:
   - Add unit tests `test_household_aware_list_rotation` and `test_full_household_rotation_and_eviction` in `crates/sync/src/lib.rs`.

---

### Phase 2: CLI & Web Companion Rotation Endpoints & UI (`crates/cli`, `apps/web`)
1. **CLI (`crates/cli/src/args.rs`, `crates/cli/src/main.rs`)**:
   - Add `HouseholdCommands::Rotate`:
     ```rust
     /// Rotate household capability and all member lists, optionally evicting specific author IDs
     Rotate {
         /// Target household ID or prefix
         #[arg(long)]
         household: Option<String>,
         /// Author IDs (or prefixes) to evict from the rotated household
         #[arg(long)]
         evict: Vec<String>,
     }
     ```
2. **Web Companion (`apps/web/src/api.rs`, `apps/web/src/server.rs`, `apps/web/src/frontend.rs`)**:
   - Add `RotateListResponseDto { old_id: String, new_id: String, ticket: String }` and `RotateHouseholdRequest { evicted_authors: Vec<String> }`.
   - Add routes:
     - `POST /api/lists/:id/rotate` -> rotates the specified list (and auto-updates any owning Household) and returns `Json<RotateListResponseDto>`.
     - `POST /api/households/:id/rotate` -> rotates the household and all its lists, returning `Json<HouseholdDto>`.
   - Add a `🔄 Rotate` button in the Web Companion UI (`apps/web/src/frontend.rs`) so users can trigger seamless list rotation directly from the browser.
3. **Verification**:
   - Add `test_web_list_and_household_rotation_endpoints` in `apps/web/tests/api_integration_tests.rs`.
   - Add cross-surface E2E test in `crates/sync/tests/e2e_cross_surface_stress.rs` verifying that when Peer A rotates a list via Web Companion, Peer B (a household member) automatically transitions to the new list and drops the old list without scanning a new ticket.