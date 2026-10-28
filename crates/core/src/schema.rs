//! Document key schemas and CBOR serialization layouts matching DESIGN.md §6.2.

use std::fmt;
use std::str::FromStr;
use serde::{de::DeserializeOwned, Serialize};

use crate::error::CoreError;
use crate::model::ItemId;

/// Canonical key structure for iroh-docs entries in a grocery list document.
///
/// Per §6.2:
/// - `meta` -> List title, schema version, creation info, category order
/// - `item/<uuidv7>/data` -> Name, quantity, unit, category, note
/// - `item/<uuidv7>/check` -> Checked flag, timestamp, author
/// - `item/<uuidv7>/pos` -> Fractional index position string
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DocKey {
    /// Document-wide metadata stored at `meta`.
    Meta,
    /// Core item data stored at `item/<id>/data`.
    ItemData(ItemId),
    /// Item check state stored at `item/<id>/check`.
    ItemCheck(ItemId),
    /// Item fractional position stored at `item/<id>/pos`.
    ItemPos(ItemId),
}

impl DocKey {
    pub const META_KEY: &'static str = "meta";

    /// Constructs an `ItemData` key.
    pub fn item_data(id: ItemId) -> Self {
        Self::ItemData(id)
    }

    /// Constructs an `ItemCheck` key.
    pub fn item_check(id: ItemId) -> Self {
        Self::ItemCheck(id)
    }

    /// Constructs an `ItemPos` key.
    pub fn item_pos(id: ItemId) -> Self {
        Self::ItemPos(id)
    }

    /// Renders the canonical ASCII string key for iroh-docs entry storage.
    pub fn to_key_string(&self) -> String {
        match self {
            Self::Meta => Self::META_KEY.to_string(),
            Self::ItemData(id) => format!("item/{id}/data"),
            Self::ItemCheck(id) => format!("item/{id}/check"),
            Self::ItemPos(id) => format!("item/{id}/pos"),
        }
    }

    /// Converts the key to a byte vector.
    pub fn to_key_bytes(&self) -> Vec<u8> {
        self.to_key_string().into_bytes()
    }

    /// Parses an iroh-docs key from a byte slice.
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, CoreError> {
        let s = std::str::from_utf8(bytes)
            .map_err(|e| CoreError::InvalidKey(format!("Key is not valid UTF-8: {e}")))?;
        Self::from_str(s)
    }

    /// Returns the associated item ID if this key targets a specific item.
    pub fn item_id(&self) -> Option<ItemId> {
        match self {
            Self::Meta => None,
            Self::ItemData(id) | Self::ItemCheck(id) | Self::ItemPos(id) => Some(*id),
        }
    }
}

impl fmt::Display for DocKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_key_string())
    }
}

impl FromStr for DocKey {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == Self::META_KEY {
            return Ok(Self::Meta);
        }

        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() == 3 && parts[0] == "item" {
            let id = ItemId::from_str(parts[1])?;
            match parts[2] {
                "data" => Ok(Self::ItemData(id)),
                "check" => Ok(Self::ItemCheck(id)),
                "pos" => Ok(Self::ItemPos(id)),
                other => Err(CoreError::InvalidKey(format!(
                    "Unknown item sub-key '{other}' in '{s}'"
                ))),
            }
        } else {
            Err(CoreError::InvalidKey(format!("Unrecognized key format: '{s}'")))
        }
    }
}


/// Canonical key structure for iroh-docs entries in a household document per §11.
///
/// - `household/meta` -> Household name, schema version, creation info
/// - `household/members/<author_id>` -> Member display name and join timestamp
/// - `household/lists/<list_id>` -> Shared list reference and DocTicket
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HouseholdDocKey {
    /// Household-wide metadata stored at `household/meta`.
    Meta,
    /// Household member entry stored at `household/members/<author_id>`.
    Member(String),
    /// Household list reference stored at `household/lists/<list_id>`.
    List(String),
}

impl HouseholdDocKey {
    pub const META_KEY: &'static str = "household/meta";

    /// Constructs a `Member` key.
    pub fn member(author_id: impl Into<String>) -> Self {
        Self::Member(author_id.into())
    }

    /// Constructs a `List` key.
    pub fn list(list_id: impl Into<String>) -> Self {
        Self::List(list_id.into())
    }

    /// Renders the canonical ASCII string key for iroh-docs entry storage.
    pub fn to_key_string(&self) -> String {
        match self {
            Self::Meta => Self::META_KEY.to_string(),
            Self::Member(author_id) => format!("household/members/{author_id}"),
            Self::List(list_id) => format!("household/lists/{list_id}"),
        }
    }

    /// Converts the key to a byte vector.
    pub fn to_key_bytes(&self) -> Vec<u8> {
        self.to_key_string().into_bytes()
    }

    /// Parses an iroh-docs household key from a byte slice.
    pub fn parse_bytes(bytes: &[u8]) -> Result<Self, CoreError> {
        let s = std::str::from_utf8(bytes)
            .map_err(|e| CoreError::InvalidKey(format!("Household key is not valid UTF-8: {e}")))?;
        Self::from_str(s)
    }
}

impl fmt::Display for HouseholdDocKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_key_string())
    }
}

impl FromStr for HouseholdDocKey {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == Self::META_KEY || s == "meta" {
            return Ok(Self::Meta);
        }

        let parts: Vec<&str> = s.split('/').collect();
        match parts.as_slice() {
            ["household", "members", id] | ["members", id] if !id.trim().is_empty() => {
                Ok(Self::Member(id.to_string()))
            }
            ["household", "lists", id] | ["lists", id] if !id.trim().is_empty() => {
                Ok(Self::List(id.to_string()))
            }
            _ => Err(CoreError::InvalidKey(format!(
                "Unrecognized household key format: '{s}'"
            ))),
        }
    }
}

/// Encodes a value into canonical CBOR bytes.
pub fn to_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, CoreError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf)
        .map_err(|e| CoreError::CborEncode(e.to_string()))?;
    Ok(buf)
}

/// Decodes a value from canonical CBOR bytes.
pub fn from_cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CoreError> {
    ciborium::from_reader(bytes)
        .map_err(|e| CoreError::CborDecode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ItemCheck, ItemData, ListMeta};
    use crate::position::ItemPosition;

    #[test]
    fn test_doc_key_roundtrip() {
        let meta_key = DocKey::Meta;
        assert_eq!(meta_key.to_key_string(), "meta");
        assert_eq!(DocKey::from_str("meta").unwrap(), meta_key);
        assert_eq!(DocKey::parse_bytes(b"meta").unwrap(), meta_key);
        assert_eq!(meta_key.item_id(), None);

        let item_id = ItemId::new_v7();
        let data_key = DocKey::item_data(item_id);
        let check_key = DocKey::item_check(item_id);
        let pos_key = DocKey::item_pos(item_id);

        assert_eq!(data_key.to_key_string(), format!("item/{item_id}/data"));
        assert_eq!(check_key.to_key_string(), format!("item/{item_id}/check"));
        assert_eq!(pos_key.to_key_string(), format!("item/{item_id}/pos"));

        assert_eq!(DocKey::from_str(&data_key.to_key_string()).unwrap(), data_key);
        assert_eq!(DocKey::from_str(&check_key.to_key_string()).unwrap(), check_key);
        assert_eq!(DocKey::from_str(&pos_key.to_key_string()).unwrap(), pos_key);

        assert_eq!(data_key.item_id(), Some(item_id));
        assert_eq!(check_key.item_id(), Some(item_id));
        assert_eq!(pos_key.item_id(), Some(item_id));

        assert!(DocKey::from_str("invalid").is_err());
        assert!(DocKey::from_str("item/not-a-uuid/data").is_err());
        assert!(DocKey::from_str("item/123/unknown").is_err());
    }

    #[test]
    fn test_cbor_serialization_roundtrip() {
        // 1. ListMeta
        let meta = ListMeta::new("Groceries", "author-1", 1700000000)
            .unwrap()
            .with_category_order(vec!["Produce".to_string(), "Bakery".to_string()]);
        let meta_bytes = to_cbor(&meta).unwrap();
        let decoded_meta: ListMeta = from_cbor(&meta_bytes).unwrap();
        assert_eq!(meta, decoded_meta);

        // 2. ItemData
        let item = ItemData::new("Bananas")
            .unwrap()
            .with_quantity(1.5, Some("kg".to_string()))
            .unwrap()
            .with_category("Produce")
            .with_note("Organic only");
        let item_bytes = to_cbor(&item).unwrap();
        let decoded_item: ItemData = from_cbor(&item_bytes).unwrap();
        assert_eq!(item, decoded_item);

        // 3. ItemCheck
        let check = ItemCheck::checked(1700000500, "author-1");
        let check_bytes = to_cbor(&check).unwrap();
        let decoded_check: ItemCheck = from_cbor(&check_bytes).unwrap();
        assert_eq!(check, decoded_check);

        // 4. ItemPosition
        let pos = ItemPosition::new("am").unwrap();
        let pos_bytes = to_cbor(&pos).unwrap();
        let decoded_pos: ItemPosition = from_cbor(&pos_bytes).unwrap();
        assert_eq!(pos, decoded_pos);
    }
}