//! Sync layer for Groshe using Iroh.
//!
//! Manages Iroh Endpoint, iroh-docs replicas, DocTicket parsing/creation,
//! list rotation, and network presence.

pub mod config;
pub mod identity;
pub mod presence;
pub mod qr;

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use futures_util::StreamExt;
use groshe_core::{
    from_cbor, to_cbor, DocKey, HouseholdDocKey, HouseholdListRef, HouseholdMember, HouseholdMeta,
    ListId, ListMeta,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use config::GrosheConfig;
pub use identity::{
    DeviceIdentity, FileIdentityStorage, IdentityStorage, KeychainIdentityStorage,
    KEYCHAIN_AUTHOR_ID_ACCOUNT, KEYCHAIN_DEFAULT_SERVICE, KEYCHAIN_SECRET_KEY_ACCOUNT,
};
pub use presence::{derive_presence_topic, ActivePeerPresence, PresencePayload, PresenceTracker, DEFAULT_PRESENCE_TTL_SECS};
pub use qr::render_qr_terminal;

pub use iroh::endpoint::presets;
pub use iroh::protocol::Router;
pub use iroh::{Endpoint, RelayMap, RelayMode, RelayUrl};
pub use iroh_blobs::api::Store as BlobsStore;
pub use iroh_blobs::store::fs::FsStore;
pub use iroh_blobs::store::mem::MemStore;
pub use iroh_blobs::{BlobsProtocol, Hash};
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
    #[error("Core domain error: {0}")]
    Core(#[from] groshe_core::CoreError),
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
    blobs_store: BlobsStore,
    gossip: Gossip,
    router: Arc<Router>,
    author_id: AuthorId,
    config: GrosheConfig,
    presence: Arc<tokio::sync::RwLock<PresenceTracker>>,
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
            .spawn(endpoint.clone(), blobs_api.clone(), gossip.clone())
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;

        let router = Router::builder(endpoint.clone())
            .accept(iroh_blobs::ALPN, blobs.clone())
            .accept(iroh_gossip::ALPN, gossip.clone())
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
            blobs_store: blobs_api,
            gossip,
            router: Arc::new(router),
            author_id,
            config: GrosheConfig::default(),
            presence: Arc::new(tokio::sync::RwLock::new(PresenceTracker::default())),
        })
    }

    /// Create a persistent sync node stored in `data_dir`.
    pub async fn persistent(data_dir: impl AsRef<Path>) -> Result<Self, SyncError> {
        let dir = data_dir.as_ref();
        let identity = DeviceIdentity::load_or_create(dir).await?;
        Self::persistent_with_identity(dir, identity).await
    }

    /// Create a persistent sync node stored in `data_dir` with a custom [DeviceIdentity].
    pub async fn persistent_with_identity(
        data_dir: impl AsRef<Path>,
        mut identity: DeviceIdentity,
    ) -> Result<Self, SyncError> {
        let dir = data_dir.as_ref();
        tokio::fs::create_dir_all(dir).await?;

        // Load or create configuration (groshe.toml)
        let config = GrosheConfig::load_or_create(dir).await?;

        let relay_mode = if config.is_relay_disabled() {
            RelayMode::Disabled
        } else if let Some(url_str) = config.custom_relay_url() {
            match url_str.parse::<RelayUrl>() {
                Ok(url) => RelayMode::Custom(RelayMap::from(url)),
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse custom relay URL '{}': {}. Falling back to default relay.",
                        url_str,
                        e
                    );
                    RelayMode::Default
                }
            }
        } else {
            RelayMode::Default
        };

        let endpoint = Endpoint::builder(presets::N0)
            .secret_key(identity.secret_key.clone())
            .relay_mode(relay_mode)
            .bind()
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
            .spawn(endpoint.clone(), blobs_api.clone(), gossip.clone())
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;

        let router = Router::builder(endpoint.clone())
            .accept(iroh_blobs::ALPN, blobs.clone())
            .accept(iroh_gossip::ALPN, gossip.clone())
            .accept(iroh_docs::ALPN, docs.clone())
            .spawn();

        let author_id = if let Some(aid) = identity.author_id {
            docs.author_set_default(aid)
                .await
                .map_err(|e| SyncError::Storage(e.to_string()))?;
            aid
        } else {
            let aid = docs
                .author_create()
                .await
                .map_err(|e| SyncError::Storage(e.to_string()))?;
            docs.author_set_default(aid)
                .await
                .map_err(|e| SyncError::Storage(e.to_string()))?;
            identity.save_author_id(aid).await?;
            aid
        };

        Ok(Self {
            endpoint,
            docs,
            blobs,
            blobs_store: blobs_api,
            gossip,
            router: Arc::new(router),
            author_id,
            config,
            presence: Arc::new(tokio::sync::RwLock::new(PresenceTracker::default())),
        })
    }

    pub fn config(&self) -> &GrosheConfig {
        &self.config
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

    pub fn blobs_store(&self) -> &BlobsStore {
        &self.blobs_store
    }

    pub fn gossip(&self) -> &Gossip {
        &self.gossip
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

    /// Creates a new document initialized with a `meta` entry.
    pub async fn create_list_with_meta(
        &self,
        title: &str,
        author_name: &str,
    ) -> Result<(Doc, DocTicket, NamespaceId), SyncError> {
        let (doc, ticket) = self.create_list().await?;
        let ns_id = doc.id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let meta = ListMeta::new(title, author_name, now)?;
        let meta_bytes = to_cbor(&meta)?;
        self.set_entry(&doc, DocKey::Meta.to_key_bytes(), meta_bytes)
            .await?;

        Ok((doc, ticket, ns_id))
    }

    /// Generates a sharing DocTicket for an existing document (alias for `share_ticket`).
    pub async fn ticket(&self, doc: &Doc) -> Result<DocTicket, SyncError> {
        self.share_ticket(doc).await
    }

    /// Shut down the underlying Iroh router and protocols.
    pub async fn shutdown(self) -> Result<(), SyncError> {
        if let Ok(router) = Arc::try_unwrap(self.router) {
            router
                .shutdown()
                .await
                .map_err(|e| SyncError::Endpoint(e.to_string()))?;
        }
        Ok(())
    }

    pub async fn share_ticket(&self, doc: &Doc) -> Result<DocTicket, SyncError> {
        doc.share(ShareMode::Write, AddrInfoOptions::RelayAndAddresses)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Renders a terminal ASCII QR code for a list ticket.
    pub fn render_ticket_qr(&self, ticket: &DocTicket) -> Result<String, SyncError> {
        render_qr_terminal(&ticket.to_string())
    }

    /// Deletes/drops a list document from local storage.
    pub async fn delete_list(&self, id: NamespaceId) -> Result<(), SyncError> {
        self.docs
            .drop_doc(id)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Executes the list rotation flow per §7:
    /// 1. Reads all active (non-tombstone) entries from `old_id`.
    /// 2. Creates a brand-new namespace.
    /// 3. Re-writes all active metadata and item records into the new doc.
    /// 4. Drops the old document from local storage.
    /// 5. Returns `(new_doc, new_ticket, new_namespace_id)`.
    pub async fn rotate_list(
        &self,
        old_id: NamespaceId,
        author_name: &str,
    ) -> Result<(Doc, DocTicket, NamespaceId), SyncError> {
        let old_doc = self
            .open_list(old_id)
            .await?
            .ok_or_else(|| SyncError::Doc(format!("Old list namespace not found: {old_id}")))?;

        // Query all existing entries
        let entries = self.get_entries_prefix(&old_doc, "").await?;

        // Create new replacement doc
        let (new_doc, new_ticket) = self.create_list().await?;
        let new_id = new_doc.id();

        // Copy over active entries (skipping empty tombstone values)
        for (key, val) in entries {
            if !val.is_empty() {
                // If key is meta, update author attribution if provided
                if key == DocKey::Meta.to_key_bytes() {
                    if let Ok(mut meta) = from_cbor::<ListMeta>(&val) {
                        meta.created_by = author_name.to_string();
                        if let Ok(updated_bytes) = to_cbor(&meta) {
                            self.set_entry(&new_doc, key, updated_bytes).await?;
                            continue;
                        }
                    }
                }
                self.set_entry(&new_doc, key, val).await?;
            }
        }

        // Close and drop the old document to cease syncing with revoked peers
        let _ = old_doc.close().await;
        drop(old_doc);
        let _ = self.delete_list(old_id).await;

        Ok((new_doc, new_ticket, new_id))
    }

    /// Import a list from a DocTicket and join peers.
    pub async fn import_list(&self, ticket: DocTicket) -> Result<Doc, SyncError> {
        self.docs
            .import(ticket)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Import a list from a stringified ticket.
    pub async fn import_list_str(&self, ticket_str: &str) -> Result<(Doc, DocTicket), SyncError> {
        let ticket = DocTicket::from_str(ticket_str.trim())
            .map_err(|e| SyncError::InvalidTicket(e.to_string()))?;
        let doc = self.import_list(ticket.clone()).await?;
        Ok((doc, ticket))
    }

    /// Import a list and return its live event stream.
    pub async fn import_list_with_events(
        &self,
        ticket: DocTicket,
    ) -> Result<
        (
            Doc,
            impl futures_util::Stream<Item = anyhow::Result<LiveEvent>>,
        ),
        SyncError,
    > {
        self.docs
            .import_and_subscribe(ticket)
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Subscribe to live events for an existing open document.
    pub async fn subscribe_doc(
        &self,
        doc: &Doc,
    ) -> Result<impl futures_util::Stream<Item = anyhow::Result<LiveEvent>>, SyncError> {
        doc.subscribe()
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

    /// Start syncing an existing document with peers in a DocTicket.
    pub async fn join_ticket(&self, doc: &Doc, ticket: &DocTicket) -> Result<(), SyncError> {
        doc.start_sync(ticket.nodes.clone())
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Checks whether a document is a Household document (has a `household/meta` entry).
    pub async fn is_household_doc(&self, doc: &Doc) -> Result<bool, SyncError> {
        Ok(self
            .get_entry(doc, HouseholdDocKey::Meta.to_key_bytes())
            .await?
            .is_some())
    }

    /// List all local grocery list document NamespaceIds (excluding Household documents).
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
                Ok((id, _state)) => {
                    let is_hh = if let Ok(Some(doc)) = self.open_list(id).await {
                        self.is_household_doc(&doc).await.unwrap_or(false)
                    } else {
                        false
                    };
                    if !is_hh {
                        list.push(id);
                    }
                }
                Err(e) => return Err(SyncError::Doc(e.to_string())),
            }
        }
        Ok(list)
    }

    /// List all local Household document NamespaceIds.
    pub async fn household_ids(&self) -> Result<Vec<NamespaceId>, SyncError> {
        let stream = self
            .docs
            .list()
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))?;
        let mut households = Vec::new();
        tokio::pin!(stream);
        while let Some(res) = stream.next().await {
            match res {
                Ok((id, _state)) => {
                    if let Ok(Some(doc)) = self.open_list(id).await {
                        if self.is_household_doc(&doc).await.unwrap_or(false) {
                            households.push(id);
                        }
                    }
                }
                Err(e) => return Err(SyncError::Doc(e.to_string())),
            }
        }
        Ok(households)
    }

    /// Creates a new Household document initialized with `household/meta` and the creator's member entry (§11).
    pub async fn create_household(
        &self,
        name: &str,
        author_name: &str,
    ) -> Result<(Doc, DocTicket, NamespaceId), SyncError> {
        let (doc, ticket) = self.create_list().await?;
        let ns_id = doc.id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let meta = HouseholdMeta::new(name, author_name, now)?;
        self.set_entry(&doc, HouseholdDocKey::Meta.to_key_bytes(), to_cbor(&meta)?)
            .await?;

        let author_str = self.author_id.to_string();
        let member = HouseholdMember::new(&author_str, author_name, now)?;
        self.set_entry(
            &doc,
            HouseholdDocKey::member(&author_str).to_key_bytes(),
            to_cbor(&member)?,
        )
        .await?;

        Ok((doc, ticket, ns_id))
    }

    /// Joins an existing Household document from a sharing ticket, registers the local member,
    /// and reconciles any active household lists (§11).
    pub async fn join_household(
        &self,
        ticket_str: &str,
        author_name: &str,
    ) -> Result<(Doc, DocTicket, NamespaceId), SyncError> {
        let (doc, ticket) = self.import_list_str(ticket_str).await?;
        let ns_id = doc.id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let author_str = self.author_id.to_string();
        let member = HouseholdMember::new(&author_str, author_name, now)?;
        self.set_entry(
            &doc,
            HouseholdDocKey::member(&author_str).to_key_bytes(),
            to_cbor(&member)?,
        )
        .await?;

        let _ = self.reconcile_household_lists(&doc).await;
        Ok((doc, ticket, ns_id))
    }

    /// Registers a grocery list inside a Household document so all household members auto-discover it.
    pub async fn register_list_in_household(
        &self,
        household_doc: &Doc,
        list_doc: &Doc,
        list_name: &str,
    ) -> Result<HouseholdListRef, SyncError> {
        let ticket = self.share_ticket(list_doc).await?;
        let list_id_str = list_doc.id().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let list_ref = HouseholdListRef::new(&list_id_str, list_name, ticket.to_string(), now)?;
        self.set_entry(
            household_doc,
            HouseholdDocKey::list(&list_id_str).to_key_bytes(),
            to_cbor(&list_ref)?,
        )
        .await?;
        Ok(list_ref)
    }

    /// Reads and deserializes the `household/meta` entry of a Household document.
    pub async fn get_household_meta(&self, doc: &Doc) -> Result<Option<HouseholdMeta>, SyncError> {
        if let Some(bytes) = self
            .get_entry(doc, HouseholdDocKey::Meta.to_key_bytes())
            .await?
        {
            if bytes.is_empty() {
                return Ok(None);
            }
            let meta: HouseholdMeta = from_cbor(&bytes)?;
            Ok(Some(meta))
        } else {
            Ok(None)
        }
    }

    /// Reads all active members registered in a Household document.
    pub async fn get_household_members(&self, doc: &Doc) -> Result<Vec<HouseholdMember>, SyncError> {
        let entries = self.get_entries_prefix(doc, "household/members/").await?;
        let mut members = Vec::new();
        for (_k, v) in entries {
            if !v.is_empty() {
                if let Ok(m) = from_cbor::<HouseholdMember>(&v) {
                    members.push(m);
                }
            }
        }
        members.sort_by(|a, b| {
            a.joined_at
                .cmp(&b.joined_at)
                .then_with(|| a.author_id.cmp(&b.author_id))
        });
        Ok(members)
    }

    /// Reads all list references registered in a Household document.
    pub async fn get_household_lists(&self, doc: &Doc) -> Result<Vec<HouseholdListRef>, SyncError> {
        let entries = self.get_entries_prefix(doc, "household/lists/").await?;
        let mut lists = Vec::new();
        for (_k, v) in entries {
            if !v.is_empty() {
                if let Ok(l) = from_cbor::<HouseholdListRef>(&v) {
                    lists.push(l);
                }
            }
        }
        lists.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.list_id.cmp(&b.list_id))
        });
        Ok(lists)
    }

    /// Automatically imports and subscribes to any non-archived lists referenced in `household_doc`
    /// that are not yet present in local storage. Returns the newly imported list `NamespaceId`s.
    pub async fn reconcile_household_lists(
        &self,
        household_doc: &Doc,
    ) -> Result<Vec<NamespaceId>, SyncError> {
        let list_refs = self.get_household_lists(household_doc).await?;
        let existing_ids = self.list_ids().await?;
        let mut newly_joined = Vec::new();

        for lref in list_refs {
            if lref.archived {
                continue;
            }
            let target_ns = NamespaceId::from_str(&lref.list_id).ok().or_else(|| {
                DocTicket::from_str(&lref.ticket)
                    .ok()
                    .map(|t| t.capability.id())
            });
            let Some(ns_id) = target_ns else {
                continue;
            };

            if !existing_ids.contains(&ns_id) {
                if let Ok((doc, _ticket)) = self.import_list_str(&lref.ticket).await {
                    newly_joined.push(doc.id());
                }
            } else if let Ok(ticket) = DocTicket::from_str(&lref.ticket) {
                if let Ok(Some(doc)) = self.open_list(ns_id).await {
                    let _ = self.join_ticket(&doc, &ticket).await;
                }
            }
        }

        Ok(newly_joined)
    }

    /// Reconciles all local Household documents, importing any newly discovered shared lists.
    pub async fn reconcile_all_households(&self) -> Result<Vec<NamespaceId>, SyncError> {
        let hh_ids = self.household_ids().await?;
        let mut all_new = Vec::new();
        for hh_id in hh_ids {
            if let Ok(Some(hh_doc)) = self.open_list(hh_id).await {
                if let Ok(mut new_ids) = self.reconcile_household_lists(&hh_doc).await {
                    all_new.append(&mut new_ids);
                }
            }
        }
        Ok(all_new)
    }

    /// Delete an entry or key prefix in a document using the node's author identity.
    pub async fn delete_entry(&self, doc: &Doc, key: impl AsRef<[u8]>) -> Result<usize, SyncError> {
        doc.del(self.author_id, key.as_ref().to_vec())
            .await
            .map_err(|e| SyncError::Doc(e.to_string()))
    }

    /// Set a key-value entry in a document using the node's author identity.
    pub async fn set_entry(
        &self,
        doc: &Doc,
        key: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Result<(), SyncError> {
        doc.set_bytes(
            self.author_id,
            key.as_ref().to_vec(),
            value.as_ref().to_vec(),
        )
        .await
        .map_err(|e| SyncError::Doc(e.to_string()))?;
        Ok(())
    }

    /// Reads and deserializes the `meta` key of a list document.
    pub async fn get_list_meta(&self, doc: &Doc) -> Result<Option<ListMeta>, SyncError> {
        if let Some(bytes) = self.get_entry(doc, DocKey::Meta.to_key_bytes()).await? {
            let meta: ListMeta = from_cbor(&bytes)?;
            Ok(Some(meta))
        } else {
            Ok(None)
        }
    }

    /// Get the latest value of an exact key across all authors.
    pub async fn get_entry(
        &self,
        doc: &Doc,
        key: impl AsRef<[u8]>,
    ) -> Result<Option<Vec<u8>>, SyncError> {
        let query = Query::single_latest_per_key()
            .key_exact(key.as_ref())
            .build();
        let entry = doc
            .get_one(query)
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

    /// Query all latest entries matching a prefix across all authors.
    pub async fn get_entries_prefix(
        &self,
        doc: &Doc,
        prefix: impl AsRef<[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, SyncError> {
        let query = Query::single_latest_per_key()
            .key_prefix(prefix.as_ref())
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

    /// Store a photo attachment blob in Iroh-Blobs (capped at 256KB thumbnail per §15).
    pub async fn add_photo_blob(&self, data: &[u8]) -> Result<[u8; 32], SyncError> {
        const MAX_PHOTO_SIZE: usize = 256 * 1024; // 256KB cap
        if data.len() > MAX_PHOTO_SIZE {
            return Err(SyncError::Other(format!(
                "Photo size ({} bytes) exceeds maximum allowed thumbnail limit of 256KB",
                data.len()
            )));
        }
        let tag_info = self
            .blobs_store
            .add_slice(data)
            .with_tag()
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(*tag_info.hash.as_bytes())
    }

    /// Retrieve a photo attachment blob by its 32-byte Blake3 / Iroh content hash.
    pub async fn get_photo_blob(&self, hash_bytes: &[u8; 32]) -> Result<Vec<u8>, SyncError> {
        let hash = iroh_blobs::Hash::from(*hash_bytes);
        let bytes = self
            .blobs_store
            .get_bytes(hash)
            .await
            .map_err(|e| SyncError::Storage(e.to_string()))?;
        Ok(bytes.to_vec())
    }

    /// Announces local presence for a list, recording it in the local `PresenceTracker`,
    /// writing an ephemeral presence heartbeat entry to the list replica, and broadcasting over gossip.
    pub async fn announce_presence(
        &self,
        namespace_id: &NamespaceId,
        author_name: &str,
        in_store: bool,
    ) -> Result<ActivePeerPresence, SyncError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let payload = PresencePayload::new(self.author_id, author_name, in_store, now);
        let active = {
            let mut tracker = self.presence.write().await;
            tracker.record_heartbeat(namespace_id, &payload)
        };

        if let Ok(Some(doc)) = self.open_list(*namespace_id).await {
            let key = format!("presence/{}", self.author_id);
            let _ = self.set_entry(&doc, key.into_bytes(), payload.to_bytes()).await;
        }

        let _ = self.publish_presence(namespace_id, in_store, author_name).await;
        Ok(active)
    }

    /// Ingests raw CBOR presence payload bytes into the node's `PresenceTracker`.
    pub async fn ingest_presence_bytes(
        &self,
        namespace_id: &NamespaceId,
        bytes: &[u8],
    ) -> Option<ActivePeerPresence> {
        let payload = PresencePayload::from_bytes(bytes)?;
        let mut tracker = self.presence.write().await;
        Some(tracker.record_heartbeat(namespace_id, &payload))
    }

    /// Returns all currently active (non-expired) peers on the specified list.
    pub async fn active_peers(
        &self,
        namespace_id: &NamespaceId,
    ) -> Result<Vec<ActivePeerPresence>, SyncError> {
        if let Ok(Some(doc)) = self.open_list(*namespace_id).await {
            if let Ok(entries) = self.get_entries_prefix(&doc, "presence/").await {
                let mut tracker = self.presence.write().await;
                for (_k, v) in entries {
                    if let Some(payload) = PresencePayload::from_bytes(&v) {
                        tracker.record_heartbeat(namespace_id, &payload);
                    }
                }
            }
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut tracker = self.presence.write().await;
        Ok(tracker.active_peers(namespace_id, now))
    }

    /// Broadcast a presence update for a list over gossip.
    pub async fn publish_presence(
        &self,
        namespace_id: &NamespaceId,
        in_store: bool,
        author_name: &str,
    ) -> Result<(), SyncError> {
        let topic_id = derive_presence_topic(namespace_id);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let payload = PresencePayload::new(self.author_id, author_name, in_store, now);
        let mut topic = self
            .gossip
            .subscribe(topic_id, Vec::new())
            .await
            .map_err(|e| SyncError::Other(e.to_string()))?;
        topic
            .broadcast(payload.to_bytes().into())
            .await
            .map_err(|e| SyncError::Other(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_groshe_sync_node_memory() {
        let node = GrosheSyncNode::memory().await.unwrap();
        let (doc, ticket) = node.create_list().await.unwrap();
        assert_eq!(ticket.capability.id(), doc.id());

        node.set_entry(&doc, b"test_key", b"test_value")
            .await
            .unwrap();
        let val = node.get_entry(&doc, b"test_key").await.unwrap();
        assert_eq!(val, Some(b"test_value".to_vec()));
    }

    #[tokio::test]
    async fn test_groshe_sync_node_persistent() {
        let tmp = tempfile::tempdir().unwrap();
        let node = GrosheSyncNode::persistent(tmp.path()).await.unwrap();
        let (doc, _ticket, id) = node
            .create_list_with_meta("Safeway", "alice")
            .await
            .unwrap();

        let meta = node.get_list_meta(&doc).await.unwrap().unwrap();
        assert_eq!(meta.title, "Safeway");
        assert_eq!(meta.created_by, "alice");

        // Verify list_ids contains the new namespace
        let ids = node.list_ids().await.unwrap();
        assert!(ids.contains(&id));
    }

    #[tokio::test]
    async fn test_persistent_node_with_custom_relay() {
        let tmp = tempfile::tempdir().unwrap();
        let toml_content = r#"
[network]
relay_url = "https://relay.sovereign.local"
"#;
        tokio::fs::write(tmp.path().join("groshe.toml"), toml_content).await.unwrap();

        let node = GrosheSyncNode::persistent(tmp.path()).await.unwrap();
        assert_eq!(
            node.config().custom_relay_url(),
            Some("https://relay.sovereign.local")
        );
        assert!(!node.config().is_relay_disabled());
        node.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_persistent_node_with_disabled_relay() {
        let tmp = tempfile::tempdir().unwrap();
        let toml_content = r#"
[network]
relay_url = "disabled"
"#;
        tokio::fs::write(tmp.path().join("groshe.toml"), toml_content).await.unwrap();

        let node = GrosheSyncNode::persistent(tmp.path()).await.unwrap();
        assert_eq!(node.config().custom_relay_url(), None);
        assert!(node.config().is_relay_disabled());
        node.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_photo_blob_storage_and_cap() {
        let node = GrosheSyncNode::memory().await.unwrap();

        // 1. Valid photo within 256KB cap
        let photo_bytes = vec![0x42u8; 1024]; // 1KB sample image
        let hash = node.add_photo_blob(&photo_bytes).await.unwrap();
        assert_ne!(hash, [0u8; 32]);

        // Retrieve and verify roundtrip
        let retrieved = node.get_photo_blob(&hash).await.unwrap();
        assert_eq!(retrieved, photo_bytes);

        // 2. Reject photo exceeding 256KB cap
        let oversized = vec![0xFFu8; 256 * 1024 + 1];
        let err = node.add_photo_blob(&oversized).await.unwrap_err();
        assert!(err
            .to_string()
            .contains("exceeds maximum allowed thumbnail limit"));
    }

    #[tokio::test]
    async fn test_list_rotation_flow() {
        let node = GrosheSyncNode::memory().await.unwrap();
        let (doc, _ticket, old_id) = node.create_list_with_meta("Costco", "alice").await.unwrap();

        // Add some items
        let item_id = groshe_core::ItemId::new_v7();
        let item = groshe_core::ItemData::new("Paper Towels").unwrap();
        let item_bytes = to_cbor(&item).unwrap();
        let key = DocKey::item_data(item_id).to_key_bytes();
        node.set_entry(&doc, &key, &item_bytes).await.unwrap();

        // Add an item then delete it via tombstone
        let del_id = groshe_core::ItemId::new_v7();
        let del_key = DocKey::item_data(del_id).to_key_bytes();
        node.set_entry(&doc, &del_key, &item_bytes).await.unwrap();
        node.delete_entry(&doc, &del_key).await.unwrap(); // tombstone

        // Close original doc handle so replica can be dropped cleanly
        let _ = doc.close().await;
        drop(doc);

        // Rotate list
        let (new_doc, new_ticket, new_id) = node.rotate_list(old_id, "alice").await.unwrap();
        assert_ne!(old_id, new_id);

        // Verify old doc is dropped from list_ids
        let ids = node.list_ids().await.unwrap();
        assert!(!ids.contains(&old_id));
        assert!(ids.contains(&new_id));

        // Verify active item exists in new doc
        let item_val = node.get_entry(&new_doc, &key).await.unwrap();
        assert!(item_val.is_some());
        let recovered_item: groshe_core::ItemData = from_cbor(&item_val.unwrap()).unwrap();
        assert_eq!(recovered_item.name, "Paper Towels");

        // Verify tombstoned item was NOT migrated
        let del_val = node.get_entry(&new_doc, &del_key).await.unwrap();
        assert!(del_val.is_none());

        // Verify ticket can render QR code
        let qr = node.render_ticket_qr(&new_ticket).unwrap();
        assert!(!qr.is_empty());
    }

    #[tokio::test]
    async fn test_persistent_node_with_keychain_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = Arc::new(KeychainIdentityStorage::new("test.service"));
        let identity = DeviceIdentity::load_or_create_with_storage(storage).await.unwrap();

        let node = GrosheSyncNode::persistent_with_identity(tmp.path(), identity).await.unwrap();
        let (doc, ticket) = node.create_list().await.unwrap();
        assert_eq!(ticket.capability.id(), doc.id());
        node.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_household_lifecycle_and_reconciliation() {
        let node_a = GrosheSyncNode::memory().await.unwrap();
        let node_b = GrosheSyncNode::memory().await.unwrap();

        // 1. Alice creates a Household and two grocery lists
        let (hh_doc_a, hh_ticket_a, hh_id) = node_a
            .create_household("Smith Family", "Alice")
            .await
            .unwrap();
        let (weekly_doc_a, _, weekly_id) = node_a
            .create_list_with_meta("Weekly Groceries", "Alice")
            .await
            .unwrap();
        let (costco_doc_a, _, costco_id) = node_a
            .create_list_with_meta("Costco Run", "Alice")
            .await
            .unwrap();

        // Verify household is excluded from list_ids() and included in household_ids()
        let lists_a = node_a.list_ids().await.unwrap();
        assert!(lists_a.contains(&weekly_id));
        assert!(lists_a.contains(&costco_id));
        assert!(!lists_a.contains(&hh_id));
        let hhs_a = node_a.household_ids().await.unwrap();
        assert_eq!(hhs_a, vec![hh_id]);

        // Register both lists in the Household
        node_a
            .register_list_in_household(&hh_doc_a, &weekly_doc_a, "Weekly Groceries")
            .await
            .unwrap();
        node_a
            .register_list_in_household(&hh_doc_a, &costco_doc_a, "Costco Run")
            .await
            .unwrap();

        // 2. Bob joins the Household using only the Household ticket
        let (hh_doc_b, _, _) = node_b
            .join_household(&hh_ticket_a.to_string(), "Bob")
            .await
            .unwrap();

        // Wait for household entries to sync to Bob and reconcile lists
        let start = std::time::Instant::now();
        let mut bob_lists = Vec::new();
        while start.elapsed() < std::time::Duration::from_secs(15) {
            let _ = node_b.reconcile_household_lists(&hh_doc_b).await;
            bob_lists = node_b.list_ids().await.unwrap();
            if bob_lists.contains(&weekly_id) && bob_lists.contains(&costco_id) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(
            bob_lists.contains(&weekly_id) && bob_lists.contains(&costco_id),
            "Bob failed to auto-discover household lists: {:?}",
            bob_lists
        );
    }
}
