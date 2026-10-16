//! Fractional-index ordering engine for deterministic, conflict-tolerant manual list ordering.
//!
//! Based on variable-length lexicographically sorted ASCII string intervals.
//! Allows inserting between any two existing items without re-indexing other items.

use std::fmt;
use serde::{Deserialize, Serialize};
use crate::error::CoreError;

/// Midpoint character between two ASCII lowercase characters.
#[inline]
fn midpoint(a: char, b: char) -> char {
    let mid = ((a as u16 + b as u16) / 2) as u8;
    mid as char
}

/// Computes a string lexicographically strictly between `prev` and `next`.
///
/// Precondition: If both `prev` and `next` are provided, `prev < next` must hold.
pub fn between_strings(prev: Option<&str>, next: Option<&str>) -> Result<String, CoreError> {
    if let (Some(p), Some(n)) = (prev, next) {
        if p >= n {
            return Err(CoreError::InvalidPosition(format!(
                "prev '{p}' must be strictly less than next '{n}'"
            )));
        }
    }

    match (prev, next) {
        // No boundaries: return midpoint of lowercase alphabet
        (None, None) => Ok("n".to_string()),

        // Prepend: strictly before `next`
        (None, Some(n)) => {
            if n.is_empty() {
                return Ok("n".to_string());
            }
            let first = n.chars().next().unwrap();
            if first > 'b' {
                let mid = midpoint('a', first);
                Ok(mid.to_string())
            } else {
                // If first is 'b' or 'a', we must prefix with 'a' and recurse on remainder
                let rest = if n.len() > 1 { Some(&n[1..]) } else { None };
                let sub = between_strings(None, rest)?;
                Ok(format!("a{sub}"))
            }
        }

        // Append: strictly after `prev`
        (Some(p), None) => {
            if p.is_empty() {
                return Ok("n".to_string());
            }
            let first = p.chars().next().unwrap();
            if first < 'y' {
                let mid = midpoint(first, 'z');
                Ok(mid.to_string())
            } else {
                let rest = if p.len() > 1 { Some(&p[1..]) } else { None };
                let sub = between_strings(rest, None)?;
                Ok(format!("{first}{sub}"))
            }
        }

        // Insert between `prev` and `next`
        (Some(p), Some(n)) => {
            let mut prefix_len = 0;
            let p_chars: Vec<char> = p.chars().collect();
            let n_chars: Vec<char> = n.chars().collect();

            while prefix_len < p_chars.len()
                && prefix_len < n_chars.len()
                && p_chars[prefix_len] == n_chars[prefix_len]
            {
                prefix_len += 1;
            }

            let prefix: String = p_chars[..prefix_len].iter().collect();
            let p_rem: String = p_chars[prefix_len..].iter().collect();
            let n_rem: String = n_chars[prefix_len..].iter().collect();

            if p_rem.is_empty() {
                // p is a prefix of n
                let sub = between_strings(None, Some(&n_rem))?;
                Ok(format!("{prefix}{sub}"))
            } else {
                let cp = p_rem.chars().next().unwrap();
                let cn = n_rem.chars().next().unwrap();
                let diff = (cn as u8) - (cp as u8);

                if diff > 1 {
                    let mid = midpoint(cp, cn);
                    Ok(format!("{prefix}{mid}"))
                } else {
                    // Adjacent characters: cn - cp == 1
                    let p_sub = if p_rem.len() > 1 { Some(&p_rem[1..]) } else { None };
                    let sub = between_strings(p_sub, None)?;
                    Ok(format!("{prefix}{cp}{sub}"))
                }
            }
        }
    }
}

/// Fractional index position stored at `item/<uuid>/pos`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ItemPosition(pub String);

impl ItemPosition {
    pub const INITIAL: &'static str = "n";

    pub fn new(pos: impl Into<String>) -> Result<Self, CoreError> {
        let s = pos.into();
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(CoreError::InvalidPosition("Position string cannot be empty".to_string()));
        }
        for ch in trimmed.chars() {
            if !ch.is_ascii_lowercase() {
                return Err(CoreError::InvalidPosition(format!(
                    "Position '{trimmed}' contains non-lowercase ASCII character '{ch}'"
                )));
            }
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Returns the initial position for the first item in an empty list.
    pub fn initial() -> Self {
        Self(Self::INITIAL.to_string())
    }

    /// Access the underlying position string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Generates a position strictly between `prev` and `next`.
    pub fn between(prev: Option<&ItemPosition>, next: Option<&ItemPosition>) -> Result<Self, CoreError> {
        let p_str = prev.map(|p| p.as_str());
        let n_str = next.map(|n| n.as_str());
        let key = between_strings(p_str, n_str)?;
        Ok(Self(key))
    }

    /// Generates a position strictly before `next`.
    pub fn before(next: &ItemPosition) -> Result<Self, CoreError> {
        Self::between(None, Some(next))
    }

    /// Generates a position strictly after `prev`.
    pub fn after(prev: &ItemPosition) -> Result<Self, CoreError> {
        Self::between(Some(prev), None)
    }
}

impl Default for ItemPosition {
    fn default() -> Self {
        Self::initial()
    }
}

impl fmt::Display for ItemPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_and_validation() {
        let p = ItemPosition::initial();
        assert_eq!(p.as_str(), "n");
        assert_eq!(ItemPosition::new("abc").unwrap().as_str(), "abc");
        assert!(ItemPosition::new("").is_err());
        assert!(ItemPosition::new("ABC").is_err());
        assert!(ItemPosition::new("123").is_err());
    }

    #[test]
    fn test_between_basic() {
        let mid = ItemPosition::between(None, None).unwrap();
        assert_eq!(mid.as_str(), "n");

        let before = ItemPosition::before(&mid).unwrap();
        assert!(before < mid, "before {:?} should be < mid {:?}", before, mid);

        let after = ItemPosition::after(&mid).unwrap();
        assert!(mid < after, "mid {:?} should be < after {:?}", mid, after);

        let in_between = ItemPosition::between(Some(&before), Some(&mid)).unwrap();
        assert!(before < in_between);
        assert!(in_between < mid);
    }

    #[test]
    fn test_sequential_appends() {
        let mut curr = ItemPosition::initial();
        for _ in 0..50 {
            let next = ItemPosition::after(&curr).unwrap();
            assert!(curr < next);
            curr = next;
        }
    }

    #[test]
    fn test_sequential_prepends() {
        let mut curr = ItemPosition::initial();
        for _ in 0..50 {
            let prev = ItemPosition::before(&curr).unwrap();
            assert!(prev < curr);
            curr = prev;
        }
    }

    #[test]
    fn test_invalid_bounds() {
        let a = ItemPosition::new("b").unwrap();
        let b = ItemPosition::new("a").unwrap();
        assert!(ItemPosition::between(Some(&a), Some(&b)).is_err());
        assert!(ItemPosition::between(Some(&a), Some(&a)).is_err());
    }
}