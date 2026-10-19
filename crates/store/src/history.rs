//! Purchase and item addition frequency tracker for smart suggestions per DESIGN.md §10 & §15.
//!
//! Tracks historical grocery items added and checked off across lists, computing
//! recency-decayed frequency scores to surface 1-tap quick suggestions for frequent staples
//! (e.g., Milk, Eggs, Bread, Bananas, Butter) while excluding items currently active on the list.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use groshe_core::{from_cbor, to_cbor, CoreError};

/// Normalized grocery item history entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Canonical lowercase normalized name used as index key (e.g. "whole milk").
    pub canonical_name: String,
    /// Preserved user-facing display name with original casing (e.g. "Whole Milk").
    pub display_name: String,
    /// Associated grocery category (e.g. "Dairy", "Produce").
    pub category: Option<String>,
    /// Typical quantity added (e.g. 1.0, 2.0).
    pub default_quantity: Option<f64>,
    /// Typical unit of measurement (e.g. "gal", "lbs", "dozen").
    pub default_unit: Option<String>,
    /// Lifetime count of times this item was added or checked off.
    pub count: u64,
    /// Unix timestamp (seconds) when this item was first introduced.
    pub first_seen_at: u64,
    /// Unix timestamp (seconds) of the most recent interaction with this item.
    pub last_used_at: u64,
}

impl HistoryEntry {
    /// Calculates recency-decayed frequency score at a given reference timestamp.
    ///
    /// Score formula: `count / (1.0 + age_in_days * 0.05)`
    /// This rewards frequent staples while prioritizing recently interacted items.
    pub fn score_at(&self, now_secs: u64) -> f64 {
        let age_secs = now_secs.saturating_sub(self.last_used_at);
        let age_days = (age_secs as f64) / 86400.0;
        (self.count as f64) / (1.0 + age_days * 0.05)
    }
}

/// Scored suggestion surfaced to the frontend quick-add bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrequentSuggestion {
    pub display_name: String,
    pub category: Option<String>,
    pub default_quantity: Option<f64>,
    pub default_unit: Option<String>,
    pub count: u64,
    pub score: f64,
}

/// Normalizes raw item text into a canonical lookup key.
pub fn canonicalize_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// In-memory and persistent tracker of grocery item usage history.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemHistoryTracker {
    /// Map of canonical name -> HistoryEntry.
    entries: BTreeMap<String, HistoryEntry>,
}

impl ItemHistoryTracker {
    /// Constructs a new empty history tracker.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Total number of distinct items tracked in history.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record an addition or check-off event for an item.
    pub fn record_item(
        &mut self,
        name: &str,
        category: Option<&str>,
        quantity: Option<f64>,
        unit: Option<&str>,
        timestamp: u64,
    ) {
        let canonical = canonicalize_name(name);
        if canonical.is_empty() {
            return;
        }

        let trimmed_name = name.trim();
        let cat_string = category
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty());
        let unit_string = unit.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());

        if let Some(entry) = self.entries.get_mut(&canonical) {
            entry.count = entry.count.saturating_add(1);
            entry.last_used_at = entry.last_used_at.max(timestamp);
            if let Some(c) = cat_string {
                entry.category = Some(c);
            }
            if let Some(q) = quantity {
                if q > 0.0 && !q.is_nan() && !q.is_infinite() {
                    entry.default_quantity = Some(q);
                }
            }
            if let Some(u) = unit_string {
                entry.default_unit = Some(u);
            }
        } else {
            let entry = HistoryEntry {
                canonical_name: canonical.clone(),
                display_name: trimmed_name.to_string(),
                category: cat_string,
                default_quantity: quantity.filter(|q| *q > 0.0 && !q.is_nan() && !q.is_infinite()),
                default_unit: unit_string,
                count: 1,
                first_seen_at: timestamp,
                last_used_at: timestamp,
            };
            self.entries.insert(canonical, entry);
        }
    }

    /// Retrieve an entry by its raw or canonical name.
    pub fn get(&self, name: &str) -> Option<&HistoryEntry> {
        let canonical = canonicalize_name(name);
        self.entries.get(&canonical)
    }

    /// Compute ranked frequent suggestions, excluding items currently active on the list.
    pub fn suggest_frequent(
        &self,
        current_active_items: &[impl AsRef<str>],
        limit: usize,
        now_secs: u64,
    ) -> Vec<FrequentSuggestion> {
        let excluded: std::collections::BTreeSet<String> = current_active_items
            .iter()
            .map(|s| canonicalize_name(s.as_ref()))
            .collect();

        let mut scored: Vec<FrequentSuggestion> = self
            .entries
            .values()
            .filter(|e| !excluded.contains(&e.canonical_name))
            .map(|e| {
                let score = e.score_at(now_secs);
                FrequentSuggestion {
                    display_name: e.display_name.clone(),
                    category: e.category.clone(),
                    default_quantity: e.default_quantity,
                    default_unit: e.default_unit.clone(),
                    count: e.count,
                    score,
                }
            })
            .collect();

        // Sort descending by score, tie-breaking by name
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.display_name.cmp(&b.display_name))
        });

        scored.truncate(limit);
        scored
    }

    /// Serializes tracker state to CBOR bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, CoreError> {
        to_cbor(self)
    }

    /// Deserializes tracker state from CBOR bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CoreError> {
        from_cbor(bytes)
    }

    /// Saves the history tracker to a file on disk atomically.
    pub fn save_to_path(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = self
            .to_bytes()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, bytes)?;
        fs::rename(temp_path, path)?;
        Ok(())
    }

    /// Loads the history tracker from a file on disk, returning an empty tracker if the file does not exist.
    pub fn load_from_path(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::new());
        }
        let bytes = fs::read(path)?;
        Self::from_bytes(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonicalization() {
        assert_eq!(canonicalize_name("  Organic Milk  "), "organic milk");
        assert_eq!(canonicalize_name("WHOLE   MILK"), "whole milk");
        assert_eq!(canonicalize_name("Eggs"), "eggs");
    }

    #[test]
    fn test_record_and_scoring() {
        let mut tracker = ItemHistoryTracker::new();
        let t0 = 100_000;

        // Record Milk 5 times
        for i in 0..5 {
            tracker.record_item(
                "Whole Milk",
                Some("Dairy"),
                Some(1.0),
                Some("gal"),
                t0 + i * 3600,
            );
        }

        // Record Bread 2 times
        for i in 0..2 {
            tracker.record_item("Sourdough Bread", Some("Bakery"), None, None, t0 + i * 3600);
        }

        // Record Butter once
        tracker.record_item("Butter", Some("Dairy"), None, None, t0);

        assert_eq!(tracker.len(), 3);

        let milk = tracker.get("whole milk").unwrap();
        assert_eq!(milk.count, 5);
        assert_eq!(milk.category.as_deref(), Some("Dairy"));
        assert_eq!(milk.default_quantity, Some(1.0));
        assert_eq!(milk.default_unit.as_deref(), Some("gal"));

        // Ask for suggestions without exclusions
        let suggestions = tracker.suggest_frequent(&[] as &[&str], 5, t0 + 5 * 3600);
        assert_eq!(suggestions.len(), 3);
        assert_eq!(suggestions[0].display_name, "Whole Milk");
        assert_eq!(suggestions[1].display_name, "Sourdough Bread");
        assert_eq!(suggestions[2].display_name, "Butter");

        // Ask for suggestions excluding active item "Whole Milk"
        let filtered = tracker.suggest_frequent(&["whole milk"], 5, t0 + 5 * 3600);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].display_name, "Sourdough Bread");
        assert_eq!(filtered[1].display_name, "Butter");
    }

    #[test]
    fn test_recency_decay() {
        let mut tracker = ItemHistoryTracker::new();
        let now = 10_000_000;

        // An item added 10 times 60 days ago
        let old_time = now - 60 * 86400;
        for _ in 0..10 {
            tracker.record_item("Old Staple", None, None, None, old_time);
        }

        // An item added 5 times yesterday
        let recent_time = now - 86400;
        for _ in 0..5 {
            tracker.record_item("Recent Staple", None, None, None, recent_time);
        }

        let suggestions = tracker.suggest_frequent(&[] as &[&str], 2, now);
        // Recent Staple: 5 / (1.0 + 1 * 0.05) = 5 / 1.05 = 4.76
        // Old Staple: 10 / (1.0 + 60 * 0.05) = 10 / 4.0 = 2.50
        assert_eq!(suggestions[0].display_name, "Recent Staple");
        assert_eq!(suggestions[1].display_name, "Old Staple");
    }

    #[test]
    fn test_serialization_and_disk_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("history.cbor");

        let mut tracker = ItemHistoryTracker::new();
        tracker.record_item("Bananas", Some("Produce"), Some(3.0), Some("lbs"), 5000);
        tracker.record_item("Apples", Some("Produce"), Some(2.0), Some("lbs"), 6000);

        tracker.save_to_path(&file_path).unwrap();

        let loaded = ItemHistoryTracker::load_from_path(&file_path).unwrap();
        assert_eq!(tracker, loaded);

        // Non-existent path returns empty tracker
        let non_existent = dir.path().join("does_not_exist.cbor");
        let empty = ItemHistoryTracker::load_from_path(&non_existent).unwrap();
        assert!(empty.is_empty());
    }
}
