//! List presence and gossip channel abstractions for Groshe.
//!
//! Enables lightweight peer presence indicators ("who is currently in-store")
//! without relying on centralized application servers.

use std::collections::BTreeMap;

use groshe_core::{from_cbor, to_cbor};
use iroh_docs::{AuthorId, NamespaceId};
use iroh_gossip::proto::TopicId;
use serde::{Deserialize, Serialize};

/// Default time-to-live (in seconds) before a peer presence heartbeat expires (§5, §10).
pub const DEFAULT_PRESENCE_TTL_SECS: u64 = 45;

/// Derives a deterministic 32-byte gossip topic identifier from a list's `NamespaceId`.
pub fn derive_presence_topic(namespace_id: &NamespaceId) -> TopicId {
    let bytes = *namespace_id.as_bytes();
    TopicId::from_bytes(bytes)
}

/// A lightweight presence heartbeat payload broadcast over gossip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresencePayload {
    pub author_id: AuthorId,
    pub author_name: String,
    pub in_store: bool,
    pub timestamp: u64,
}

impl PresencePayload {
    pub fn new(
        author_id: AuthorId,
        author_name: impl Into<String>,
        in_store: bool,
        timestamp: u64,
    ) -> Self {
        Self {
            author_id,
            author_name: author_name.into(),
            in_store,
            timestamp,
        }
    }

    /// Serializes payload to CBOR bytes for gossip broadcast.
    pub fn to_bytes(&self) -> Vec<u8> {
        to_cbor(self).unwrap_or_default()
    }

    /// Parses payload from received gossip bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        from_cbor(bytes).ok()
    }
}


/// Materialized presence status for an active peer on a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivePeerPresence {
    pub author_id: String,
    pub author_name: String,
    pub in_store: bool,
    pub last_seen: u64,
}

/// Tracks active peer heartbeats per list namespace with automatic TTL expiration.
#[derive(Debug, Clone)]
pub struct PresenceTracker {
    ttl_secs: u64,
    peers_by_list: BTreeMap<[u8; 32], BTreeMap<String, ActivePeerPresence>>,
}

impl PresenceTracker {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            ttl_secs,
            peers_by_list: BTreeMap::new(),
        }
    }

    /// Records a presence heartbeat for a given list namespace using Last-Writer-Wins semantics.
    pub fn record_heartbeat(
        &mut self,
        namespace_id: &NamespaceId,
        payload: &PresencePayload,
    ) -> ActivePeerPresence {
        let list_key = *namespace_id.as_bytes();
        let author_key = payload.author_id.to_string();
        let peers = self.peers_by_list.entry(list_key).or_default();

        if let Some(existing) = peers.get_mut(&author_key) {
            if payload.timestamp >= existing.last_seen {
                existing.author_name = payload.author_name.clone();
                existing.in_store = payload.in_store;
                existing.last_seen = payload.timestamp;
            }
            return existing.clone();
        }

        let entry = ActivePeerPresence {
            author_id: author_key.clone(),
            author_name: payload.author_name.clone(),
            in_store: payload.in_store,
            last_seen: payload.timestamp,
        };
        peers.insert(author_key, entry.clone());
        entry
    }

    /// Prunes expired peer heartbeats across all tracked lists relative to `now_secs`.
    pub fn prune_expired(&mut self, now_secs: u64) {
        let ttl = self.ttl_secs;
        for peers in self.peers_by_list.values_mut() {
            peers.retain(|_, p| now_secs.saturating_sub(p.last_seen) <= ttl);
        }
    }

    /// Returns all currently active peers for `namespace_id` after pruning expired entries.
    /// In-store shoppers are sorted first, followed by alphabetical `author_name`.
    pub fn active_peers(
        &mut self,
        namespace_id: &NamespaceId,
        now_secs: u64,
    ) -> Vec<ActivePeerPresence> {
        self.prune_expired(now_secs);
        let list_key = *namespace_id.as_bytes();
        let Some(peers) = self.peers_by_list.get(&list_key) else {
            return Vec::new();
        };
        let mut active: Vec<ActivePeerPresence> = peers.values().cloned().collect();
        active.sort_by(|a, b| {
            b.in_store
                .cmp(&a.in_store)
                .then_with(|| a.author_name.cmp(&b.author_name))
                .then_with(|| a.author_id.cmp(&b.author_id))
        });
        active
    }
}

impl Default for PresenceTracker {
    fn default() -> Self {
        Self::new(DEFAULT_PRESENCE_TTL_SECS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_topic_derivation_and_payload() {
        let secret = iroh::SecretKey::generate();
        let ns = NamespaceId::from(*secret.public().as_bytes());
        let topic = derive_presence_topic(&ns);
        assert_eq!(topic.as_bytes(), ns.as_bytes());

        let author = AuthorId::from(*secret.public().as_bytes());
        let payload = PresencePayload::new(author, "Alice", true, 12345);
        let bytes = payload.to_bytes();
        let decoded = PresencePayload::from_bytes(&bytes).unwrap();
        assert_eq!(payload, decoded);
    }

    #[test]
    fn test_presence_tracker_ttl_and_sorting() {
        let s1 = iroh::SecretKey::generate();
        let s2 = iroh::SecretKey::generate();
        let ns = NamespaceId::from(*s1.public().as_bytes());
        let a1 = AuthorId::from(*s1.public().as_bytes());
        let a2 = AuthorId::from(*s2.public().as_bytes());

        let mut tracker = PresenceTracker::new(45);
        tracker.record_heartbeat(&ns, &PresencePayload::new(a1, "Alice", false, 100));
        tracker.record_heartbeat(&ns, &PresencePayload::new(a2, "Bob", true, 110));

        // At t=120, both are active; Bob (in_store=true) is sorted first
        let active = tracker.active_peers(&ns, 120);
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].author_name, "Bob");
        assert!(active[0].in_store);
        assert_eq!(active[1].author_name, "Alice");
        assert!(!active[1].in_store);

        // At t=150, Alice (100 + 45 < 150) has expired, Bob (110 + 45 >= 150) remains active
        let active_later = tracker.active_peers(&ns, 150);
        assert_eq!(active_later.len(), 1);
        assert_eq!(active_later[0].author_name, "Bob");
    }
}
