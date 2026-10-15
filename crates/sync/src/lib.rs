//! Sync layer for Groshe using Iroh.
//!
//! Manages Iroh Endpoint, iroh-docs replicas, DocTicket parsing/creation,
//! and network presence.

use std::path::Path;
use std::sync::Arc;

use futures_util::StreamExt;
use groshe_core::ListId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use iroh::endpoint::presets;
pub use iroh::protocol::Router;
pub use iroh::Endpoint;
pub use iroh_blobs::api::Store as BlobsStore;
pub use iroh_blobs::store::fs::FsStore;
pub use iroh_blobs::store::mem::MemStore;
pub use iroh_blobs::BlobsProtocol;
pub use iroh_docs::api::protocol::{AddrInfoOptions, ShareMode};
pub use iroh_docs::api::Doc;
pub use iroh_docs::engine::LiveEvent;
pub use iroh_docs::protocol::Docs;
pub use iroh_docs::store::Query;
pub use iroh_docs::{AuthorId, DocTicket, NamespaceId, NamespaceSecret};
pub use iroh_gossip::net::Gossip;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Failed to parse ticket: {0}")]
    InvalidTicket(String),
    #[error("Iroh endpoint error: {0}")]
    Endpoint(String),
    #[error("Document sync error: {0}")]
    Doc(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("General error: {0}")]
    Other(String),
}

/// A serialized ticket for sharing a list with other peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListTicket {
    pub list_id: ListId,
    pub ticket_str: String,
}

impl ListTicket {
    pub fn new(list_id: ListId, ticket_str: impl Into<String>) -> Self {
        Self {
            list_id,
            ticket_str: ticket_str.into(),
        }
    }
}

/// A Groshe sync node encapsulating Endpoint, Docs, Blobs, and Gossip.
#[derive(Clone)]
pub struct GrosheSyncNode {
    endpoint: Endpoint,
    docs: Docs,
    blobs: BlobsProtocol,
    router: Arc<Router>,
    author_id: AuthorId,
}

impl GrosheSyncNode {
    /// Create an in-memory sync node (primarily for tests).
    pub async fn memory() -> Result<Self, SyncError> {
        let endpoint = Endpoint::bind(presets::N0)
            .await
            .map_err(|e| SyncError::Endpoint(e.to_string()))?;

        let gossip = Gossip::builder().spawn(endpoint.clone());
        let blobs_store = MemStore::new();
        let blobs_api: BlobsStore = blobs_store.into();
        let blobs = BlobsProtocol::new(&blobs_api, None);

        let docs = Docs::memory()
            .spawn(endpoint.clone(), blobs_api, gossip.clone())
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;

        let router = Router::builder(endpoint.clone())
            .accept(iroh_blobs::ALPN, blobs.clone())
            .accept(iroh_gossip::ALPN, gossip)
            .accept(iroh_docs::ALPN, docs.clone())
            .spawn();

        let author_id = docs
            .author_create()
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        docs.author_set_default(author_id)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;

        Ok(Self {
            endpoint,
            docs,
            blobs,
            router: Arc::new(router),
            author_id,
        })
    }

    /// Create a persistent sync node stored in `data_dir`.
    pub async fn persistent(data_dir: impl AsRef<Path>) -> Result<Self, SyncError> {
        let dir = data_dir.as_ref();
        tokio::fs::create_dir_all(dir).await?;

        let endpoint = Endpoint::bind(presets::N0)
            .await
            .map_err(|e| SyncError::Endpoint(e.to_string()))?;

        let gossip = Gossip::builder().spawn(endpoint.clone());

        let blobs_path = dir.join("blobs");
        tokio::fs::create_dir_all(&blobs_path).await?;
        let fs_store = FsStore::load(&blobs_path)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        let blobs_api: BlobsStore = fs_store.into();
        let blobs = BlobsProtocol::new(&blobs_api, None);

        let docs_path = dir.join("docs");
        tokio::fs::create_dir_all(&docs_path).await?;
        let docs = Docs::persistent(docs_path)
            .spawn(endpoint.clone(), blobs_api, gossip.clone())
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;

        let router = Router::builder(endpoint.clone())
            .accept(iroh_blobs::ALPN, blobs.clone())
            .accept(iroh_gossip::ALPN, gossip)
            .accept(iroh_docs::ALPN, docs.clone())
            .spawn();

        let author_id = match docs.author_default().await {
            Ok(id) => id,
            Err(_) => {
                let id = docs
                    .author_create()
                    .await
                    .map_err(|e| SyncError::Storage(e.to_string()))?;
                docs.author_set_default(id)
                    .await
                    .map_err(|e| SyncError::Storage(e.to_string()))?;
                id
            }
        };

        Ok(Self {
            endpoint,
            docs,
            blobs,
            router: Arc::new(router),
            author_id,
        })
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn docs(&self) -> &Docs {
        &self.docs
    }

    pub fn blobs(&self) -> &BlobsProtocol {
        &self.blobs
    }

    pub fn author_id(&self) -> AuthorId {
        self.author_id
    }

    /// Create a new document replica and generate a sharing DocTicket.
    pub async fn create_list(&self) -> Result<(Doc, DocTicket), SyncError> {
        let doc = self
            .docs
            .create()
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;
        let ticket = doc
            .share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;
        Ok((doc, ticket))
    }

    /// Import a list from a DocTicket and join peers.
    pub async fn import_list(&self, ticket: DocTicket) -> Result<Doc, SyncError> {
        self.docs
            .import(ticket)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Import a list and return its live event stream.
    pub async fn import_list_with_events(
        &self,
        ticket: DocTicket,
    ) -> Result<(Doc, impl futures_util::Stream<Item = anyhow::Result<LiveEvent>>), SyncError> {
        self.docs
            .import_and_subscribe(ticket)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Open an existing document by its NamespaceId.
    pub async fn open_list(&self, id: NamespaceId) -> Result<Option<Doc>, SyncError> {
        self.docs
            .open(id)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// List all local document NamespaceIds.
    pub async fn list_ids(&self) -> Result<Vec<NamespaceId>, SyncError> {
        let stream = self
            .docs
            .list()
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;
        let mut list = Vec::new();
        tokio::pin!(stream);
        while let Some(res) = stream.next().await {
            match res {
                Ok((id, _state)) => list.push(id),
                Err(e) => return Err(SyncError::Doc(e.to_string())),
            }
        }
        Ok(list)
    }

    /// Set a key-value entry in a document using the node's author identity.
    pub async fn set_entry(
        &self,
        doc: &Doc,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Result<(), SyncError> {
        doc.set_bytes(self.author_id, key.as_ref().to_vec(), value.as_ref().to_vec())
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;
        Ok(())
    }

    /// Get the value of an exact key in a document.
    pub async fn get_entry(
        &self,
        doc: &Doc,
        key: impl AsRef<[u8]>,
    ) -> Result<Option<Vec<u8>>, SyncError> {
        let entry = doc
            .get_exact(self.author_id, key.as_ref().to_vec(), false)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;
        if let Some(e) = entry {
            let bytes = self
                .blobs
                .get_bytes(e.content_hash())
                .await
                .map_err(|e| SyncError::Storage(e.to_string()))?;
            Ok(Some(bytes.to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Query all entries matching a prefix.
    pub async fn get_entries_prefix(
        &self,
        doc: &Doc,
        prefix: impl AsRef<[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, SyncError> {
        let query = Query::single_latest_per_key()
            .key_prefix(prefix.as_ref().to_vec())
            .build();
        let stream = doc
            .get_many(query)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;
        let mut entries = Vec::new();
        tokio::pin!(stream);
        while let Some(res) = stream.next().await {
            let entry = res.map_err(|e| SyncError::Doc(e.to_string()))?;
            let key = entry.key().to_vec();
            if let Ok(bytes) = self.blobs.get_bytes(entry.content_hash()).await {
                entries.push((key, bytes.to_vec()));
            }
        }
        Ok(entries)
    }

    /// Share an existing document with peers.
    pub async fn ticket(&self, doc: &Doc) -> Result<DocTicket, SyncError> {
        doc.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Shut down the underlying Iroh router and protocols.
    pub async fn shutdown(self) -> Result<(), SyncError> {
        self.router
            .shutdown()
            .await
            .map_err(|e| SyncError::Endpoint(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_groshe_sync_node_memory() {
        let node1 = GrosheSyncNode::memory().await.unwrap();
        let (doc1, ticket) = node1.create_list().await.unwrap();
        node1
            .set_entry(&doc1, "item/1/data", "Oat Milk")
            .await
            .unwrap();

        let val = node1.get_entry(&doc1, "item/1/data").await.unwrap();
        assert_eq!(val, Some(b"Oat Milk".to_vec()));

        // Node 2 imports list
        let node2 = GrosheSyncNode::memory().await.unwrap();
        let (doc2, mut events2) = node2.import_list_with_events(ticket).await.unwrap();

        let mut got_item = false;
        let timeout = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                Some(event) = events2.next() => {
                    if let Ok(LiveEvent::ContentReady { hash }) = event {
                        if let Ok(bytes) = node2.blobs().get_bytes(hash).await {
                            if bytes.as_ref() == b"Oat Milk" {
                                got_item = true;
                                break;
                            }
                        }
                    }
                }
                _ = &mut timeout => {
                    break;
                }
            }
        }

        if !got_item {
            let entries = node2.get_entries_prefix(&doc2, "item/").await.unwrap();
            for (k, v) in entries {
                if k == b"item/1/data" && v == b"Oat Milk" {
                    got_item = true;
                    break;
                }
            }
        }

        assert!(got_item, "Node 2 did not receive item from Node 1");
    }

    #[tokio::test]
    async fn test_groshe_sync_node_persistent() {
        let dir = tempfile::tempdir().unwrap();
        let node = GrosheSyncNode::persistent(dir.path()).await.unwrap();
        let (doc, ticket) = node.create_list().await.unwrap();
        node.set_entry(&doc, "item/test/data", "Persisted Oat Milk")
            .await
            .unwrap();

        let val = node.get_entry(&doc, "item/test/data").await.unwrap();
        assert_eq!(val, Some(b"Persisted Oat Milk".to_vec()));

        let ticket_str = ticket.to_string();
        let parsed_ticket: DocTicket = ticket_str.parse().unwrap();
        assert_eq!(parsed_ticket.to_string(), ticket.to_string());

        node.shutdown().await.unwrap();
    }
}
