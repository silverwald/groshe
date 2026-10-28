//! Pure Rust domain models, fractional ordering, and CBOR schemas for Groshe.
//!
//! This crate contains zero I/O and minimal dependencies, keeping it portable
//! across mobile, desktop, CLI, and WASM environments.

pub mod error;
pub mod export;
pub mod model;
pub mod parser;
pub mod position;
pub mod schema;

// Re-export common domain types for ergonomic workspace usage
pub use error::CoreError;
pub use export::{hex_32, parse_hex_32, ExportItem, ExportList};
pub use model::{
    HouseholdId, HouseholdListRef, HouseholdMember, HouseholdMeta, ItemCheck, ItemData, ItemId,
    ListId, ListMeta,
};
pub use parser::{parse_quick_add, suggest_category, ParsedItem};
pub use position::{between_strings, ItemPosition};
pub use schema::{from_cbor, to_cbor, DocKey, HouseholdDocKey};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_meta_validation() {
        assert_eq!(
            ListMeta::new("", "alice", 100),
            Err(CoreError::EmptyListTitle)
        );
        assert_eq!(
            ListMeta::new("   ", "alice", 100),
            Err(CoreError::EmptyListTitle)
        );

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
        let p1 = ItemPosition::initial();
        let p2 = ItemPosition::after(&p1).unwrap();
        let p_mid = ItemPosition::between(Some(&p1), Some(&p2)).unwrap();

        assert!(p1 < p2, "p1 ({:?}) should be less than p2 ({:?})", p1, p2);
        assert!(p1 < p_mid, "p1 should be less than p_mid");
        assert!(p_mid < p2, "p_mid should be less than p2");
    }
}
