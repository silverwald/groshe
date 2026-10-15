//! Sync layer for Groshe using Iroh.
//!
//! Manages Iroh Endpoint, iroh-docs replicas, DocTicket parsing/creation,
//! and network presence.

use groshe_core::ListId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Failed to parse ticket: {0}")]
    InvalidTicket(String),
    #[error("Iroh endpoint error: {0}")]
    Endpoint(String),
    #[error("Document sync error: {0}")]
    Doc(String),
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

/// Placeholder for sync service state during Phase 0 de-risking.
#[derive(Debug, Default)]
pub struct SyncManager {
    // Phase 0/1 will hook Endpoint & Doc replicas here
}

impl SyncManager {
    pub fn new() -> Self {
        Self::default()
    }
}
