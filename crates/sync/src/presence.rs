//! List presence and gossip channel abstractions for Groshe.
//!
//! Enables lightweight peer presence indicators ("who is currently in-store")
//! without relying on centralized application servers.

use groshe_core::{from_cbor, to_cbor};
use iroh_docs::{AuthorId, NamespaceId};
use iroh_gossip::proto::TopicId;
use serde::{Deserialize, Serialize};

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
}