//! Pure Rust domain models and ordering logic for Groshe.
//!
//! This crate contains zero I/O and minimal dependencies, keeping it portable
//! across mobile, desktop, CLI, and WASM environments.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum CoreError {
    #[error("Item name cannot be empty")]
    EmptyItemName,
    #[error("List title cannot be empty")]
    EmptyListTitle,
    #[error("Invalid quantity: must be positive")]
    InvalidQuantity,
    #[error("Invalid fractional index: {0}")]
    InvalidPosition(String),
}

/// Unique ID for a grocery list (UUIDv7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ListId(pub Uuid);

impl ListId {
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
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

/// Unique ID for an item within a grocery list (UUIDv7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(pub Uuid);

impl ItemId {
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
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

/// Metadata stored at `meta` key in the list doc.
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

    pub fn new(title: impl Into<String>, created_by: impl Into<String>, created_at: u64) -> Result<Self, CoreError> {
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
}

/// Core item data stored at `item/<uuid>/data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemData {
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub category: Option<String>,
    pub note: Option<String>,
}

impl ItemData {
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
        })
    }

    pub fn with_quantity(mut self, qty: f64, unit: Option<String>) -> Result<Self, CoreError> {
        if qty <= 0.0 {
            return Err(CoreError::InvalidQuantity);
        }
        self.quantity = Some(qty);
        self.unit = unit;
        Ok(self)
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }
}

/// Item checked state stored at `item/<uuid>/check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCheck {
    pub checked: bool,
    pub checked_at: Option<u64>,
    pub checked_by: Option<String>,
}

impl ItemCheck {
    pub fn unchecked() -> Self {
        Self {
            checked: false,
            checked_at: None,
            checked_by: None,
        }
    }

    pub fn checked(checked_at: u64, checked_by: impl Into<String>) -> Self {
        Self {
            checked: true,
            checked_at: Some(checked_at),
            checked_by: Some(checked_by.into()),
        }
    }
}

/// Fractional index position stored at `item/<uuid>/pos`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ItemPosition(pub String);

impl ItemPosition {
    pub const INITIAL: &'static str = "a0";

    pub fn new(pos: impl Into<String>) -> Self {
        Self(pos.into())
    }

    /// Generates a position lexicographically between `prev` and `next`.
    pub fn between(prev: Option<&str>, next: Option<&str>) -> Self {
        match (prev, next) {
            (None, None) => Self(Self::INITIAL.to_string()),
            (None, Some(n)) => {
                let first = n.chars().next().unwrap_or('a');
                if first > 'a' {
                    let prev_ch = ((first as u8) - 1) as char;
                    Self(format!("{}{}", prev_ch, "m"))
                } else {
                    Self(format!("a0{}", n))
                }
            }
            (Some(p), None) => {
                Self(format!("{}z", p))
            }
            (Some(p), Some(n)) => {
                if p < n {
                    Self(format!("{}m", p))
                } else {
                    // Fallback to append if order was non-standard
                    Self(format!("{}z", p))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_meta_validation() {
        assert_eq!(ListMeta::new("", "alice", 100), Err(CoreError::EmptyListTitle));
        assert_eq!(ListMeta::new("   ", "alice", 100), Err(CoreError::EmptyListTitle));

        let meta = ListMeta::new("Trader Joe's", "alice", 100).unwrap();
        assert_eq!(meta.title, "Trader Joe's");
        assert_eq!(meta.schema_version, 1);
    }

    #[test]
    fn test_item_data_validation() {
        assert_eq!(ItemData::new(""), Err(CoreError::EmptyItemName));
        let item = ItemData::new("Almond Milk")
            .unwrap()
            .with_quantity(2.0, Some("bottles".to_string()))
            .unwrap()
            .with_category("Dairy & Alternatives");

        assert_eq!(item.name, "Almond Milk");
        assert_eq!(item.quantity, Some(2.0));
        assert_eq!(item.unit.as_deref(), Some("bottles"));
    }

    #[test]
    fn test_fractional_position_ordering() {
        let p1 = ItemPosition::between(None, None);
        let p2 = ItemPosition::between(Some(&p1.0), None);
        let p_mid = ItemPosition::between(Some(&p1.0), Some(&p2.0));

        assert!(p1 < p2, "p1 ({:?}) should be less than p2 ({:?})", p1, p2);
        assert!(p1 < p_mid, "p1 should be less than p_mid");
        assert!(p_mid < p2, "p_mid should be less than p2");
    }
}
