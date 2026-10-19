//! Materialized projection and query store for Groshe.
//!
//! Frontends (Slint mobile UI, CLI, and Web) read from this in-memory materialized
//! projection rather than querying the P2P sync layer directly on render passes.
//!
//! Handles multi-key aggregation (`meta`, `item/<id>/data`, `item/<id>/check`, `item/<id>/pos`),
//! field-level Last-Writer-Wins (LWW) resolution, 30-day tombstone tracking for undo,
//! and real-time reactive event broadcasting.

use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use groshe_core::{from_cbor, CoreError, DocKey, ItemCheck, ItemData, ItemId, ItemPosition, ListId, ListMeta};
use thiserror::Error;

pub mod history;
pub use history::{canonicalize_name, FrequentSuggestion, HistoryEntry, ItemHistoryTracker};

/// 30-day tombstone retention period in seconds per §6.3.
pub const TOMBSTONE_RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("List not found: {0}")]
    ListNotFound(ListId),

    #[error("Item not found: {0}")]
    ItemNotFound(ItemId),

    #[error("No recently deleted items to restore")]
    NoDeletedItems,

    #[error("Core domain error: {0}")]
    Core(#[from] CoreError),
}

/// Real-time change events emitted by the store to subscribed frontends.
#[derive(Debug, Clone, PartialEq)]
pub enum StoreEvent {
    ListMetaUpdated {
        list_id: ListId,
        title: String,
    },
    ItemAdded {
        list_id: ListId,
        item: ItemView,
    },
    ItemUpdated {
        list_id: ListId,
        item: ItemView,
    },
    ItemChecked {
        list_id: ListId,
        item_id: ItemId,
        checked: bool,
    },
    ItemPositionUpdated {
        list_id: ListId,
        item_id: ItemId,
        position: ItemPosition,
    },
    ItemDeleted {
        list_id: ListId,
        item_id: ItemId,
    },
    ItemRestored {
        list_id: ListId,
        item: ItemView,
    },
    ListReset {
        list_id: ListId,
    },
}

/// Fully materialized item view combining data, checked status, and ordering.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemView {
    pub id: ItemId,
    pub data: ItemData,
    pub check: ItemCheck,
    pub position: ItemPosition,
    pub has_data: bool,
    pub data_updated_at: u64,
    pub check_updated_at: u64,
    pub pos_updated_at: u64,
}

impl ItemView {
    /// Constructs a new ItemView with default unchecked state and initial position.
    pub fn new(id: ItemId, data: ItemData, timestamp: u64) -> Self {
        Self {
            id,
            data,
            check: ItemCheck::unchecked(),
            position: ItemPosition::initial(),
            has_data: true,
            data_updated_at: timestamp,
            check_updated_at: 0,
            pos_updated_at: 0,
        }
    }

    pub fn is_checked(&self) -> bool {
        self.check.checked
    }

    pub fn name(&self) -> &str {
        &self.data.name
    }

    pub fn category(&self) -> Option<&str> {
        self.data.category.as_deref()
    }
}

/// Record of a recently deleted item kept for undo operations.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentlyDeletedItem {
    pub item: ItemView,
    pub deleted_at: u64,
}

/// In-memory materialized projection of a single grocery list document.
#[derive(Debug, Clone, PartialEq)]
pub struct ListProjection {
    pub id: ListId,
    pub meta: Option<ListMeta>,
    pub meta_updated_at: u64,
    pub items: BTreeMap<ItemId, ItemView>,
    pub recently_deleted: Vec<RecentlyDeletedItem>,
}

impl ListProjection {
    pub fn new(id: ListId) -> Self {
        Self {
            id,
            meta: None,
            meta_updated_at: 0,
            items: BTreeMap::new(),
            recently_deleted: Vec::new(),
        }
    }

    pub fn with_meta(id: ListId, meta: ListMeta, timestamp: u64) -> Self {
        Self {
            id,
            meta: Some(meta),
            meta_updated_at: timestamp,
            items: BTreeMap::new(),
            recently_deleted: Vec::new(),
        }
    }

    pub fn title(&self) -> &str {
        self.meta.as_ref().map(|m| m.title.as_str()).unwrap_or("Untitled List")
    }

    pub fn get_item(&self, id: &ItemId) -> Option<&ItemView> {
        self.items.get(id)
    }

    pub fn item_count(&self) -> usize {
        self.items.values().filter(|i| i.has_data).count()
    }

    pub fn checked_count(&self) -> usize {
        self.items.values().filter(|i| i.has_data && i.is_checked()).count()
    }

    /// Returns items sorted by their fractional position ascending.
    pub fn sorted_items(&self) -> Vec<&ItemView> {
        let mut list: Vec<&ItemView> = self.items.values().filter(|i| i.has_data).collect();
        list.sort_by(|a, b| a.position.cmp(&b.position));
        list
    }

    /// Returns items grouped by category, adhering to `category_order` with uncategorized last.
    pub fn items_by_category(&self) -> BTreeMap<String, Vec<&ItemView>> {
        let mut map: BTreeMap<String, Vec<&ItemView>> = BTreeMap::new();
        for item in self.sorted_items() {
            let cat = item.category().unwrap_or("Uncategorized").to_string();
            map.entry(cat).or_default().push(item);
        }
        map
    }

    /// Applies a raw entry update (key bytes + value bytes) with LWW conflict resolution.
    pub fn apply_entry(
        &mut self,
        key_bytes: &[u8],
        value_bytes: &[u8],
        timestamp: u64,
    ) -> Result<Option<StoreEvent>, StoreError> {
        let doc_key = DocKey::parse_bytes(key_bytes)?;

        match doc_key {
            DocKey::Meta => {
                if value_bytes.is_empty() {
                    // Tombstone on list meta
                    self.meta = None;
                    self.meta_updated_at = timestamp;
                    return Ok(Some(StoreEvent::ListReset { list_id: self.id }));
                }

                if timestamp >= self.meta_updated_at {
                    let meta: ListMeta = from_cbor(value_bytes)?;
                    let title = meta.title.clone();
                    self.meta = Some(meta);
                    self.meta_updated_at = timestamp;
                    return Ok(Some(StoreEvent::ListMetaUpdated {
                        list_id: self.id,
                        title,
                    }));
                }
            }

            DocKey::ItemData(item_id) => {
                if value_bytes.is_empty() {
                    // Tombstone: item deleted with LWW timestamp semantics
                    if let Some(existing) = self.items.get_mut(&item_id) {
                        if timestamp >= existing.data_updated_at {
                            let mut item_snapshot = existing.clone();
                            item_snapshot.has_data = true;
                            self.recently_deleted.push(RecentlyDeletedItem {
                                item: item_snapshot,
                                deleted_at: timestamp,
                            });
                            existing.has_data = false;
                            existing.data_updated_at = timestamp;
                            return Ok(Some(StoreEvent::ItemDeleted {
                                list_id: self.id,
                                item_id,
                            }));
                        }
                    } else {
                        // Track tombstone timestamp so older inserts arriving out-of-order are rejected
                        let mut placeholder = ItemView::new(
                            item_id,
                            ItemData::new("Deleted item").expect("valid item name"),
                            timestamp,
                        );
                        placeholder.has_data = false;
                        self.items.insert(item_id, placeholder);
                    }
                    return Ok(None);
                }

                let data: ItemData = from_cbor(value_bytes)?;
                if let Some(existing) = self.items.get_mut(&item_id) {
                    if timestamp >= existing.data_updated_at {
                        existing.data = data;
                        existing.has_data = true;
                        existing.data_updated_at = timestamp;
                        let item_view = existing.clone();
                        return Ok(Some(StoreEvent::ItemUpdated {
                            list_id: self.id,
                            item: item_view,
                        }));
                    }
                } else {
                    let new_item = ItemView::new(item_id, data, timestamp);
                    let event = StoreEvent::ItemAdded {
                        list_id: self.id,
                        item: new_item.clone(),
                    };
                    self.items.insert(item_id, new_item);
                    return Ok(Some(event));
                }
            }

            DocKey::ItemCheck(item_id) => {
                if value_bytes.is_empty() {
                    return Ok(None);
                }

                let check: ItemCheck = from_cbor(value_bytes)?;
                if let Some(existing) = self.items.get_mut(&item_id) {
                    if timestamp >= existing.check_updated_at {
                        existing.check = check.clone();
                        existing.check_updated_at = timestamp;
                        return Ok(Some(StoreEvent::ItemChecked {
                            list_id: self.id,
                            item_id,
                            checked: check.checked,
                        }));
                    }
                } else {
                    // Create placeholder if check arrived before data
                    let mut placeholder = ItemView::new(
                        item_id,
                        ItemData::new("Pending item").expect("valid item name"),
                        0,
                    );
                    placeholder.has_data = false;
                    placeholder.check = check.clone();
                    placeholder.check_updated_at = timestamp;
                    self.items.insert(item_id, placeholder);
                    return Ok(Some(StoreEvent::ItemChecked {
                        list_id: self.id,
                        item_id,
                        checked: check.checked,
                    }));
                }
            }

            DocKey::ItemPos(item_id) => {
                if value_bytes.is_empty() {
                    return Ok(None);
                }

                let pos: ItemPosition = from_cbor(value_bytes)?;
                if let Some(existing) = self.items.get_mut(&item_id) {
                    if timestamp >= existing.pos_updated_at {
                        existing.position = pos.clone();
                        existing.pos_updated_at = timestamp;
                        return Ok(Some(StoreEvent::ItemPositionUpdated {
                            list_id: self.id,
                            item_id,
                            position: pos,
                        }));
                    }
                } else {
                    // Create placeholder if position arrived before data
                    let mut placeholder = ItemView::new(
                        item_id,
                        ItemData::new("Pending item").expect("valid item name"),
                        0,
                    );
                    placeholder.has_data = false;
                    placeholder.position = pos.clone();
                    placeholder.pos_updated_at = timestamp;
                    self.items.insert(item_id, placeholder);
                    return Ok(Some(StoreEvent::ItemPositionUpdated {
                        list_id: self.id,
                        item_id,
                        position: pos,
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Restores the most recently deleted item, or a specific item by ID.
    pub fn restore_deleted(&mut self, target_id: Option<ItemId>) -> Result<ItemView, StoreError> {
        let idx = if let Some(id) = target_id {
            self.recently_deleted
                .iter()
                .rposition(|r| r.item.id == id)
                .ok_or(StoreError::ItemNotFound(id))?
        } else {
            if self.recently_deleted.is_empty() {
                return Err(StoreError::NoDeletedItems);
            }
            self.recently_deleted.len() - 1
        };

        let mut restored = self.recently_deleted.remove(idx).item;
        restored.has_data = true;
        self.items.insert(restored.id, restored.clone());
        Ok(restored)
    }

    /// Prunes tombstones older than `TOMBSTONE_RETENTION_SECS` relative to `now`.
    pub fn prune_tombstones(&mut self, now: u64) {
        if now > TOMBSTONE_RETENTION_SECS {
            let cutoff = now - TOMBSTONE_RETENTION_SECS;
            self.recently_deleted.retain(|r| r.deleted_at >= cutoff);
        }
    }
}

/// Thread-safe reactive store managing multiple list projections with event broadcasting.
#[derive(Clone)]
pub struct ReactiveStore {
    lists: Arc<RwLock<BTreeMap<ListId, ListProjection>>>,
    sender: broadcast::Sender<StoreEvent>,
    history: Arc<RwLock<ItemHistoryTracker>>,
}

impl ReactiveStore {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            lists: Arc::new(RwLock::new(BTreeMap::new())),
            sender,
            history: Arc::new(RwLock::new(ItemHistoryTracker::new())),
        }
    }

    /// Constructs a reactive store with pre-loaded item history.
    pub fn with_history(capacity: usize, history: ItemHistoryTracker) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            lists: Arc::new(RwLock::new(BTreeMap::new())),
            sender,
            history: Arc::new(RwLock::new(history)),
        }
    }

    /// Access the underlying history tracker.
    pub fn history(&self) -> Arc<RwLock<ItemHistoryTracker>> {
        Arc::clone(&self.history)
    }

    /// Computes frequent suggestions for a given list, excluding currently active items.
    pub async fn suggest_frequent(
        &self,
        list_id: &ListId,
        limit: usize,
        now_secs: u64,
    ) -> Vec<FrequentSuggestion> {
        let guard = self.lists.read().await;
        let active_items: Vec<String> = if let Some(proj) = guard.get(list_id) {
            proj.items.values().filter(|i| i.has_data).map(|i| i.name().to_string()).collect()
        } else {
            Vec::new()
        };
        let hist = self.history.read().await;
        hist.suggest_frequent(&active_items, limit, now_secs)
    }

    /// Subscribes to real-time store mutation events.
    pub fn subscribe(&self) -> broadcast::Receiver<StoreEvent> {
        self.sender.subscribe()
    }

    /// Gets a cloned snapshot of a list projection.
    pub async fn get_list(&self, list_id: &ListId) -> Option<ListProjection> {
        let guard = self.lists.read().await;
        guard.get(list_id).cloned()
    }

    /// Lists all tracked list identifiers.
    pub async fn list_ids(&self) -> Vec<ListId> {
        let guard = self.lists.read().await;
        guard.keys().copied().collect()
    }

    /// Registers or resets a list projection.
    pub async fn register_list(&self, list_id: ListId, meta: Option<ListMeta>) {
        let mut guard = self.lists.write().await;
        let mut proj = ListProjection::new(list_id);
        if let Some(m) = meta {
            proj.meta = Some(m);
        }
        guard.insert(list_id, proj);
    }

    /// Applies a raw entry update to the specified list, broadcasting any emitted event.
    pub async fn apply_raw_entry(
        &self,
        list_id: ListId,
        key: &[u8],
        value: &[u8],
        timestamp: u64,
    ) -> Result<(), StoreError> {
        let mut guard = self.lists.write().await;
        let list = guard.entry(list_id).or_insert_with(|| ListProjection::new(list_id));

        if let Some(event) = list.apply_entry(key, value, timestamp)? {
            match &event {
                StoreEvent::ItemAdded { item, .. } | StoreEvent::ItemUpdated { item, .. } => {
                    let mut hist = self.history.write().await;
                    hist.record_item(
                        item.name(),
                        item.data.category.as_deref(),
                        item.data.quantity,
                        item.data.unit.as_deref(),
                        timestamp,
                    );
                }
                StoreEvent::ItemChecked { item_id, checked: true, .. } => {
                    if let Some(item) = list.get_item(item_id) {
                        let mut hist = self.history.write().await;
                        hist.record_item(
                            item.name(),
                            item.data.category.as_deref(),
                            item.data.quantity,
                            item.data.unit.as_deref(),
                            timestamp,
                        );
                    }
                }
                _ => {}
            }
            // Ignore send errors if there are currently no active receivers
            let _ = self.sender.send(event);
        }
        Ok(())
    }

    /// Restores a recently deleted item in the specified list.
    pub async fn restore_deleted(
        &self,
        list_id: ListId,
        item_id: Option<ItemId>,
    ) -> Result<ItemView, StoreError> {
        let mut guard = self.lists.write().await;
        let list = guard.get_mut(&list_id).ok_or(StoreError::ListNotFound(list_id))?;
        let restored = list.restore_deleted(item_id)?;
        let _ = self.sender.send(StoreEvent::ItemRestored {
            list_id,
            item: restored.clone(),
        });
        Ok(restored)
    }
}

impl Default for ReactiveStore {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use groshe_core::to_cbor;

    #[tokio::test]
    async fn test_store_multi_key_aggregation_and_lww() {
        let store = ReactiveStore::default();
        let mut rx = store.subscribe();
        let list_id = ListId::new_v7();
        let item_id = ItemId::new_v7();

        // 1. Apply Meta
        let meta = ListMeta::new("Groceries", "author-1", 100).unwrap();
        let meta_bytes = to_cbor(&meta).unwrap();
        store
            .apply_raw_entry(list_id, DocKey::Meta.to_key_bytes().as_slice(), &meta_bytes, 100)
            .await
            .unwrap();

        let event1 = rx.recv().await.unwrap();
        assert_eq!(
            event1,
            StoreEvent::ListMetaUpdated {
                list_id,
                title: "Groceries".to_string()
            }
        );

        // 2. Add Item Data
        let item_data = ItemData::new("Oat Milk").unwrap().with_quantity(2.0, None).unwrap();
        let item_data_bytes = to_cbor(&item_data).unwrap();
        let data_key = DocKey::item_data(item_id).to_key_bytes();
        store
            .apply_raw_entry(list_id, &data_key, &item_data_bytes, 105)
            .await
            .unwrap();

        let event2 = rx.recv().await.unwrap();
        match event2 {
            StoreEvent::ItemAdded { list_id: lid, item } => {
                assert_eq!(lid, list_id);
                assert_eq!(item.id, item_id);
                assert_eq!(item.name(), "Oat Milk");
                assert_eq!(item.data.quantity, Some(2.0));
                assert!(!item.is_checked());
            }
            other => panic!("Unexpected event: {:?}", other),
        }

        // 3. Toggle Check state without clobbering item data
        let check = ItemCheck::checked(110, "author-1");
        let check_bytes = to_cbor(&check).unwrap();
        let check_key = DocKey::item_check(item_id).to_key_bytes();
        store
            .apply_raw_entry(list_id, &check_key, &check_bytes, 110)
            .await
            .unwrap();

        let event3 = rx.recv().await.unwrap();
        assert_eq!(
            event3,
            StoreEvent::ItemChecked {
                list_id,
                item_id,
                checked: true
            }
        );

        // 4. Update Position
        let pos = ItemPosition::new("ab").unwrap();
        let pos_bytes = to_cbor(&pos).unwrap();
        let pos_key = DocKey::item_pos(item_id).to_key_bytes();
        store
            .apply_raw_entry(list_id, &pos_key, &pos_bytes, 115)
            .await
            .unwrap();

        let event4 = rx.recv().await.unwrap();
        assert_eq!(
            event4,
            StoreEvent::ItemPositionUpdated {
                list_id,
                item_id,
                position: pos.clone()
            }
        );

        // 5. Verify projection state has merged all 3 keys
        let proj = store.get_list(&list_id).await.unwrap();
        assert_eq!(proj.title(), "Groceries");
        assert_eq!(proj.item_count(), 1);
        let item = proj.get_item(&item_id).unwrap();
        assert_eq!(item.name(), "Oat Milk");
        assert_eq!(item.data.quantity, Some(2.0));
        assert!(item.is_checked());
        assert_eq!(item.position, pos);

        // 6. Test LWW rejection of older timestamp
        let older_data = ItemData::new("Old Stale Name").unwrap();
        let older_bytes = to_cbor(&older_data).unwrap();
        store
            .apply_raw_entry(list_id, &data_key, &older_bytes, 90) // older than 105
            .await
            .unwrap();

        let proj2 = store.get_list(&list_id).await.unwrap();
        assert_eq!(proj2.get_item(&item_id).unwrap().name(), "Oat Milk"); // Not clobbered!

        // 7. Delete Item via Tombstone (empty bytes)
        store
            .apply_raw_entry(list_id, &data_key, &[], 120)
            .await
            .unwrap();

        let event5 = rx.recv().await.unwrap();
        assert_eq!(event5, StoreEvent::ItemDeleted { list_id, item_id });

        let proj3 = store.get_list(&list_id).await.unwrap();
        assert_eq!(proj3.item_count(), 0);
        assert_eq!(proj3.recently_deleted.len(), 1);

        // 8. Restore deleted item
        let restored = store.restore_deleted(list_id, None).await.unwrap();
        assert_eq!(restored.id, item_id);
        assert_eq!(restored.name(), "Oat Milk");

        let event6 = rx.recv().await.unwrap();
        assert_eq!(
            event6,
            StoreEvent::ItemRestored {
                list_id,
                item: restored
            }
        );

        let proj4 = store.get_list(&list_id).await.unwrap();
        assert_eq!(proj4.item_count(), 1);

        // 9. Verify History Tracking via ReactiveStore
        let suggestions = store.suggest_frequent(&list_id, 5, 200).await;
        // Oat Milk is active on the list, so it should be excluded!
        assert!(suggestions.is_empty());

        let other_list = ListId::new_v7();
        let suggestions_other = store.suggest_frequent(&other_list, 5, 200).await;
        // On another list where Oat Milk is not active, it should be suggested!
        assert_eq!(suggestions_other.len(), 1);
        assert_eq!(suggestions_other[0].display_name, "Oat Milk");
    }
}
