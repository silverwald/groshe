//! Core domain entities for lists and items.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::CoreError;

/// Unique ID for a grocery list (UUIDv7, time-ordered).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ListId(pub Uuid);

impl ListId {
    /// Generate a new time-ordered UUIDv7 list identifier.
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    /// Access the underlying inner UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ListId {
    fn default() -> Self {
        Self::new_v7()
    }
}

impl fmt::Display for ListId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ListId {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| CoreError::InvalidUuid(format!("Invalid ListId '{s}': {e}")))
    }
}

/// Unique ID for an item within a grocery list (UUIDv7, time-ordered).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ItemId(pub Uuid);

impl ItemId {
    /// Generate a new time-ordered UUIDv7 item identifier.
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    /// Access the underlying inner UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ItemId {
    fn default() -> Self {
        Self::new_v7()
    }
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ItemId {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| CoreError::InvalidUuid(format!("Invalid ItemId '{s}': {e}")))
    }
}

/// Metadata stored at `meta` key in the list doc per §6.2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListMeta {
    pub schema_version: u32,
    pub title: String,
    pub created_at: u64,
    pub created_by: String,
    pub category_order: Vec<String>,
}

impl ListMeta {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Creates a new validated `ListMeta` instance.
    pub fn new(
        title: impl Into<String>,
        created_by: impl Into<String>,
        created_at: u64,
    ) -> Result<Self, CoreError> {
        let title = title.into();
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(CoreError::EmptyListTitle);
        }

        Ok(Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            title: trimmed.to_string(),
            created_at,
            created_by: created_by.into(),
            category_order: Vec::new(),
        })
    }

    /// Appends or updates custom category aisle ordering.
    pub fn with_category_order(mut self, categories: Vec<String>) -> Self {
        self.category_order = categories;
        self
    }
}

/// Core item data stored at `item/<uuid>/data` per §6.2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemData {
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub category: Option<String>,
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo_hash: Option<[u8; 32]>,
}

impl ItemData {
    /// Creates a new item data record with validated non-empty name.
    pub fn new(name: impl Into<String>) -> Result<Self, CoreError> {
        let name = name.into();
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(CoreError::EmptyItemName);
        }

        Ok(Self {
            name: trimmed.to_string(),
            quantity: None,
            unit: None,
            category: None,
            note: None,
            photo_hash: None,
        })
    }

    /// Sets quantity and optional unit of measurement.
    pub fn with_quantity(mut self, qty: f64, unit: Option<String>) -> Result<Self, CoreError> {
        if qty <= 0.0 || qty.is_nan() || qty.is_infinite() {
            return Err(CoreError::InvalidQuantity);
        }
        self.quantity = Some(qty);
        self.unit = unit;
        Ok(self)
    }

    /// Sets item category/aisle.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Sets optional user note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Sets optional photo attachment hash (Blake3 / Iroh blob hash).
    pub fn with_photo_hash(mut self, hash: [u8; 32]) -> Self {
        self.photo_hash = Some(hash);
        self
    }

    /// Returns hex-encoded representation of the photo Blake3 hash if present.
    pub fn photo_hash_hex(&self) -> Option<String> {
        self.photo_hash
            .map(|h| h.iter().map(|b| format!("{:02x}", b)).collect())
    }
}

/// Item checked state stored at `item/<uuid>/check` per §6.2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCheck {
    pub checked: bool,
    pub checked_at: Option<u64>,
    pub checked_by: Option<String>,
}

impl ItemCheck {
    /// Unchecked initial state.
    pub fn unchecked() -> Self {
        Self {
            checked: false,
            checked_at: None,
            checked_by: None,
        }
    }

    /// Checked state with author identifier and timestamp.
    pub fn checked(checked_at: u64, checked_by: impl Into<String>) -> Self {
        Self {
            checked: true,
            checked_at: Some(checked_at),
            checked_by: Some(checked_by.into()),
        }
    }
}
