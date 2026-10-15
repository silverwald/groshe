//! Materialized projection and query store for Groshe.
//!
//! Frontends read from this projected state rather than querying the P2P sync
//! layer directly on render passes.

use groshe_core::{ItemCheck, ItemData, ItemId, ItemPosition, ListId, ListMeta};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("List not found: {0}")]
    ListNotFound(ListId),
    #[error("Item not found: {0}")]
    ItemNotFound(ItemId),
}

/// Fully materialized item view combining data, checked status, and ordering.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemView {
    pub id: ItemId,
    pub data: ItemData,
    pub check: ItemCheck,
    pub position: ItemPosition,
}

/// In-memory projection of a single grocery list.
#[derive(Debug, Clone, PartialEq)]
pub struct ListProjection {
    pub id: ListId,
    pub meta: ListMeta,
    pub items: BTreeMap<ItemId, ItemView>,
}

impl ListProjection {
    pub fn new(id: ListId, meta: ListMeta) -> Self {
        Self {
            id,
            meta,
            items: BTreeMap::new(),
        }
    }

    /// Returns items sorted by their fractional position.
    pub fn sorted_items(&self) -> Vec<&ItemView> {
        let mut list: Vec<&ItemView> = self.items.values().collect();
        list.sort_by(|a, b| a.position.cmp(&b.position));
        list
    }
}
