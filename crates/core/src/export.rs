//! Backup, export, and import formatting for Groshe lists per DESIGN.md §8 & §15.
//!
//! Provides full-fidelity JSON serialization for complete backups/restores, and
//! human-readable GitHub Flavored Markdown (GFM) checklist export and parsing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::model::ItemId;
use crate::parser::parse_quick_add;
use crate::position::ItemPosition;

/// DTO representing a single grocery item during export/import.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportItem {
    pub id: ItemId,
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub category: Option<String>,
    pub note: Option<String>,
    pub checked: bool,
    pub position: String,
}

/// DTO representing a complete grocery list backup snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportList {
    pub title: String,
    pub schema_version: u32,
    pub created_by: String,
    pub created_at: u64,
    pub items: Vec<ExportItem>,
}

impl ExportList {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Creates a new ExportList snapshot.
    pub fn new(
        title: impl Into<String>,
        created_by: impl Into<String>,
        created_at: u64,
        items: Vec<ExportItem>,
    ) -> Self {
        Self {
            title: title.into(),
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            created_by: created_by.into(),
            created_at,
            items,
        }
    }

    /// Serializes to a pretty-printed JSON backup string.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes from a JSON backup string.
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Formats the list into a human-readable Markdown checklist grouped by category.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", self.title));

        // Group unchecked items by category
        let mut by_cat: BTreeMap<String, Vec<&ExportItem>> = BTreeMap::new();
        let mut checked_items: Vec<&ExportItem> = Vec::new();

        for item in &self.items {
            if item.checked {
                checked_items.push(item);
            } else {
                let cat = item.category.as_deref().unwrap_or("General").to_string();
                by_cat.entry(cat).or_default().push(item);
            }
        }

        for (category, cat_items) in by_cat {
            out.push_str(&format!("## {}\n", category));
            for item in cat_items {
                let qty_str = format_quantity_suffix(item.quantity, item.unit.as_deref());
                let note_str = item
                    .note
                    .as_deref()
                    .map(|n| format!(" *({})*", n))
                    .unwrap_or_default();
                out.push_str(&format!("- [ ] {}{}{}\n", item.name, qty_str, note_str));
            }
            out.push('\n');
        }

        if !checked_items.is_empty() {
            out.push_str("## Completed\n");
            for item in checked_items {
                let qty_str = format_quantity_suffix(item.quantity, item.unit.as_deref());
                let note_str = item
                    .note
                    .as_deref()
                    .map(|n| format!(" *({})*", n))
                    .unwrap_or_default();
                out.push_str(&format!("- [x] {}{}{}\n", item.name, qty_str, note_str));
            }
            out.push('\n');
        }

        out
    }

    /// Parses a Markdown document into an `ExportList`.
    ///
    /// Recognizes:
    /// - `# Title` for list title.
    /// - `## Category` or `### Category` for aisle headings.
    /// - `- [ ] item` / `* [ ] item` for unchecked items.
    /// - `- [x] item` / `- [X] item` for checked items.
    /// - Plain `- item` for unchecked items.
    /// - Automatic quantity, unit, and name extraction via `parse_quick_add`.
    pub fn from_markdown(default_title: &str, content: &str) -> Self {
        let mut title = default_title.to_string();
        let mut current_category = None;
        let mut items = Vec::new();
        let mut last_pos: Option<ItemPosition> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // List Title (# Title)
            if trimmed.starts_with("# ") && !trimmed.starts_with("## ") {
                title = trimmed[2..].trim().to_string();
                continue;
            }

            // Category heading (## Category or ### Category)
            if trimmed.starts_with("## ") || trimmed.starts_with("### ") {
                let cat = trimmed.trim_start_matches('#').trim();
                if cat.eq_ignore_ascii_case("completed") || cat.eq_ignore_ascii_case("done") {
                    current_category = None;
                } else {
                    current_category = Some(cat.to_string());
                }
                continue;
            }

            // Checklist item
            let (is_checked, raw_text) =
                if trimmed.starts_with("- [ ] ") || trimmed.starts_with("* [ ] ") {
                    (false, &trimmed[6..])
                } else if trimmed.starts_with("- [x] ")
                    || trimmed.starts_with("- [X] ")
                    || trimmed.starts_with("* [x] ")
                    || trimmed.starts_with("* [X] ")
                {
                    (true, &trimmed[6..])
                } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                    (false, &trimmed[2..])
                } else {
                    continue;
                };

            let item_text = raw_text.trim();
            if item_text.is_empty() {
                continue;
            }

            // Check for trailing note in parentheses *(note)* or (note)
            let (core_text, note) = extract_note(item_text);

            let parsed = parse_quick_add(&core_text);
            if parsed.name.is_empty() {
                continue;
            }

            let next_pos = if let Some(ref prev) = last_pos {
                ItemPosition::after(prev).unwrap_or_else(|_| ItemPosition::initial())
            } else {
                ItemPosition::initial()
            };
            last_pos = Some(next_pos.clone());

            let category = current_category.clone().or(parsed.category);

            items.push(ExportItem {
                id: ItemId::new_v7(),
                name: parsed.name,
                quantity: parsed.quantity,
                unit: parsed.unit,
                category,
                note,
                checked: is_checked,
                position: next_pos.to_string(),
            });
        }

        Self {
            title,
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            created_by: "import".to_string(),
            created_at: 0,
            items,
        }
    }
}

fn format_quantity_suffix(qty: Option<f64>, unit: Option<&str>) -> String {
    if let Some(q) = qty {
        let q_str = if q.fract() == 0.0 {
            format!("{}", q as i64)
        } else {
            format!("{:.2}", q)
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        };
        if let Some(u) = unit {
            format!(" (qty: {} {})", q_str, u)
        } else {
            format!(" (qty: {})", q_str)
        }
    } else {
        String::new()
    }
}

fn extract_note(text: &str) -> (String, Option<String>) {
    let trimmed = text.trim();
    let stripped = trimmed.trim_end_matches(['*', '_', ' ']);
    if let Some(idx) = stripped.rfind('(') {
        if stripped.ends_with(')') {
            let core = trimmed[..idx].trim().trim_end_matches(['*', '_', ' ']).trim();
            let raw_note = &stripped[idx + 1..stripped.len() - 1];
            let note = raw_note.trim().trim_matches(['*', '_', ' ']).trim();
            if !note.is_empty() && !core.is_empty() {
                return (core.to_string(), Some(note.to_string()));
            }
        }
    }
    (trimmed.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_json_roundtrip() {
        let item1 = ExportItem {
            id: ItemId::new_v7(),
            name: "Apples".to_string(),
            quantity: Some(2.0),
            unit: Some("lbs".to_string()),
            category: Some("Produce".to_string()),
            note: Some("Honeycrisp".to_string()),
            checked: false,
            position: "a0".to_string(),
        };

        let item2 = ExportItem {
            id: ItemId::new_v7(),
            name: "Whole Milk".to_string(),
            quantity: Some(1.0),
            unit: Some("gal".to_string()),
            category: Some("Dairy".to_string()),
            note: None,
            checked: true,
            position: "a1".to_string(),
        };

        let list = ExportList::new("Weekly Groceries", "alice", 1234567, vec![item1, item2]);
        let json = list.to_json_pretty().unwrap();

        let decoded = ExportList::from_json(&json).unwrap();
        assert_eq!(list, decoded);
    }

    #[test]
    fn test_to_markdown_formatting() {
        let item1 = ExportItem {
            id: ItemId::new_v7(),
            name: "Avocados".to_string(),
            quantity: Some(3.0),
            unit: None,
            category: Some("Produce".to_string()),
            note: None,
            checked: false,
            position: "a0".to_string(),
        };

        let item2 = ExportItem {
            id: ItemId::new_v7(),
            name: "Whole Milk".to_string(),
            quantity: Some(1.0),
            unit: Some("gal".to_string()),
            category: Some("Dairy".to_string()),
            note: Some("Organic".to_string()),
            checked: true,
            position: "a1".to_string(),
        };

        let list = ExportList::new("Supermarket Trip", "alice", 1000, vec![item1, item2]);
        let md = list.to_markdown();

        assert!(md.contains("# Supermarket Trip"));
        assert!(md.contains("## Produce"));
        assert!(md.contains("- [ ] Avocados (qty: 3)"));
        assert!(md.contains("## Completed"));
        assert!(md.contains("- [x] Whole Milk (qty: 1 gal) *(Organic)*"));
    }

    #[test]
    fn test_from_markdown_parsing() {
        let raw_md = r#"
# Weekend BBQ

## Produce
- [ ] 3 avocados
- [ ] 2 lbs apples *(Honeycrisp)*

## Meat
- [ ] 4 ribeye steaks

## Completed
- [x] Burger Buns (qty: 2 packs)
"#;

        let parsed = ExportList::from_markdown("Default", raw_md);
        assert_eq!(parsed.title, "Weekend BBQ");
        assert_eq!(parsed.items.len(), 4);

        let avocados = &parsed.items[0];
        assert_eq!(avocados.name, "avocados");
        assert_eq!(avocados.quantity, Some(3.0));
        assert_eq!(avocados.category.as_deref(), Some("Produce"));
        assert!(!avocados.checked);

        let apples = &parsed.items[1];
        assert_eq!(apples.name, "apples");
        assert_eq!(apples.quantity, Some(2.0));
        assert_eq!(apples.unit.as_deref(), Some("lbs"));
        assert_eq!(apples.note.as_deref(), Some("Honeycrisp"));

        let steaks = &parsed.items[2];
        assert_eq!(steaks.name, "ribeye steaks");
        assert_eq!(steaks.quantity, Some(4.0));
        assert_eq!(steaks.category.as_deref(), Some("Meat"));

        let buns = &parsed.items[3];
        assert_eq!(buns.name, "Burger Buns");
        assert!(buns.checked);
    }
}
