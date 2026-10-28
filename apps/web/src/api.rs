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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivePeerDto {
    pub author_id: String,
    pub author_name: String,
    pub in_store: bool,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdatePresenceRequest {
    pub in_store: bool,
    #[serde(default)]
    pub author_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListDetailDto {
    pub id: String,
    pub title: String,
    pub created_by: String,
    pub created_at: u64,
    pub items: Vec<ItemDto>,
    #[serde(default)]
    pub active_peers: Vec<ActivePeerDto>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateItemRequest {
    /// Optional updated item name
    #[serde(default)]
    pub name: Option<String>,
    /// Optional updated quantity
    #[serde(default)]
    pub quantity: Option<f64>,
    /// Optional updated unit
    #[serde(default)]
    pub unit: Option<String>,
    /// Optional updated category override
    #[serde(default)]
    pub category: Option<String>,
    /// Optional updated note
    #[serde(default)]
    pub note: Option<String>,
    /// Optional raw input for quick-add re-parsing
    #[serde(default)]
    pub raw_input: Option<String>,
    /// Optional updated base64-encoded thumbnail photo
    #[serde(default)]
    pub photo_base64: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReorderItemRequest {
    /// Place item immediately before this item ID
    #[serde(default)]
    pub before_id: Option<String>,
    /// Place item immediately after this item ID
    #[serde(default)]
    pub after_id: Option<String>,
    /// Relative directional step: "up", "down", "top", or "bottom"
    #[serde(default)]
    pub direction: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HouseholdMemberDto {
    pub author_id: String,
    pub display_name: String,
    pub joined_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HouseholdListDto {
    pub list_id: String,
    pub name: String,
    pub archived: bool,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HouseholdDto {
    pub id: String,
    pub name: String,
    pub ticket: String,
    pub members: Vec<HouseholdMemberDto>,
    pub lists: Vec<HouseholdListDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHouseholdRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinHouseholdRequest {
    pub ticket: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddHouseholdListRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub list_id: Option<String>,
}
