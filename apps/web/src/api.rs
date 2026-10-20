//! DTOs for the Groshe Web Companion REST & WebSocket API.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListSummaryDto {
    pub id: String,
    pub title: String,
    pub created_by: String,
    pub created_at: u64,
    pub item_count: usize,
    pub checked_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemDto {
    pub id: String,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub category: Option<String>,
    pub note: Option<String>,
    pub checked: bool,
    pub position: String,
    pub photo_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListDetailDto {
    pub id: String,
    pub title: String,
    pub created_by: String,
    pub created_at: u64,
    pub items: Vec<ItemDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddItemRequest {
    /// Raw quick-add string supporting inline shorthand grammar, e.g. "2 gal milk @dairy"
    pub raw_input: String,
    /// Explicit category override, if any
    #[serde(default)]
    pub category: Option<String>,
    /// Optional note
    #[serde(default)]
    pub note: Option<String>,
    /// Optional base64-encoded thumbnail photo (<256KB)
    #[serde(default)]
    pub photo_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToggleCheckRequest {
    pub checked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatusDto {
    pub node_id: String,
    pub author_id: String,
    pub relay: String,
    pub lists_count: usize,
}
