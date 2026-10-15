//! Sync bridge linking Slint UI event loop with GrosheSyncNode background worker.
//!
//! Bridges user mutations from Slint event handlers into Iroh CRDT document updates,
//! and forwards remote live events (via `LiveEvent`) to Slint using `upgrade_in_event_loop`.

use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use groshe_sync::{DocTicket, GrosheSyncNode, LiveEvent};
use slint::{Model, ModelRc, VecModel};
use tokio::sync::mpsc;

use crate::{AppWindow, GroceryItemData};

/// Commands issued from the UI to the background sync engine.
#[derive(Debug)]
pub enum SyncCommand {
    AddItem {
        id: String,
        name: String,
        quantity: String,
        category: String,
    },
    ToggleItemCheck {
        id: String,
        checked: bool,
    },
    DeleteItem {
        id: String,
    },
    JoinTicket {
        ticket_str: String,
    },
}

/// Handle to communicate with the background sync worker.
#[derive(Clone)]
pub struct SyncBridgeHandle {
    sender: mpsc::UnboundedSender<SyncCommand>,
}

impl SyncBridgeHandle {
    pub fn send(&self, cmd: SyncCommand) {
        if let Err(e) = self.sender.send(cmd) {
            tracing::error!("Failed to dispatch sync command: {}", e);
        }
    }
}

/// Launch the sync worker on the provided Tokio runtime.
pub fn spawn_sync_bridge(
    runtime: &tokio::runtime::Runtime,
    storage_dir: PathBuf,
    app_weak: slint::Weak<AppWindow>,
) -> SyncBridgeHandle {
    let (tx, mut rx) = mpsc::unbounded_channel::<SyncCommand>();

    runtime.spawn(async move {
        tracing::info!("Starting background sync bridge in {:?}", storage_dir);

        let node = match GrosheSyncNode::persistent(&storage_dir).await {
            Ok(n) => n,
            Err(e) => {
                tracing::error!("Failed to initialize GrosheSyncNode: {}", e);
                return;
            }
        };

        // Open existing document or create a new one
        let doc_ids = node.list_ids().await.unwrap_or_default();
        let (doc, ticket) = if let Some(id) = doc_ids.first() {
            match node.open_list(*id).await {
                Ok(Some(d)) => {
                    let ticket = node.ticket(&d).await.ok();
                    (d, ticket)
                }
                _ => match node.create_list().await {
                    Ok((d, t)) => (d, Some(t)),
                    Err(e) => {
                        tracing::error!("Failed to create document: {}", e);
                        return;
                    }
                },
            }
        } else {
            match node.create_list().await {
                Ok((d, t)) => (d, Some(t)),
                Err(e) => {
                    tracing::error!("Failed to create document: {}", e);
                    return;
                }
            }
        };

        if let Some(t) = ticket {
            tracing::info!("Groshe list ticket: {}", t);
        }

        // Subscribe to remote document events
        let events_res = node.subscribe_doc(&doc).await;
        let mut events_stream = match events_res {
            Ok(s) => Some(Box::pin(s)),
            Err(e) => {
                tracing::warn!("Could not subscribe to live doc events: {}", e);
                None
            }
        };

        // Populate any pre-existing items from the database into the UI model
        if let Ok(entries) = node.get_entries_prefix(&doc, "item/").await {
            for (k_bytes, v_bytes) in entries {
                let k_str = String::from_utf8_lossy(&k_bytes);
                let v_str = String::from_utf8_lossy(&v_bytes);

                if let Some(rest) = k_str.strip_prefix("item/") {
                    if let Some((id, prop)) = rest.split_once('/') {
                        let id = id.to_string();
                        let app_weak_ui = app_weak.clone();
                        let val_string = v_str.to_string();
                        let prop = prop.to_string();

                        let _ = app_weak_ui.upgrade_in_event_loop(move |app| {
                            let mut current_items: Vec<GroceryItemData> =
                                app.get_items().iter().collect();
                            let mut found = false;
                            for item in current_items.iter_mut() {
                                if item.id == id.as_str() {
                                    found = true;
                                    if prop == "data" {
                                        item.name = val_string.clone().into();
                                    } else if prop == "check" {
                                        item.checked = val_string == "true";
                                    }
                                    break;
                                }
                            }
                            if !found && prop == "data" {
                                current_items.push(GroceryItemData {
                                    id: id.into(),
                                    name: val_string.into(),
                                    quantity: "1".into(),
                                    category: "General".into(),
                                    checked: false,
                                });
                            }
                            app.set_items(ModelRc::new(VecModel::from(current_items)));
                        });
                    }
                }
            }
        }

        let doc_arc = Arc::new(doc);
        let node_arc = Arc::new(node);

        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        SyncCommand::AddItem { id, name, quantity, category: _ } => {
                            let val = format!("{} ({})", name, quantity);
                            let _ = node_arc.set_entry(&doc_arc, format!("item/{}/data", id), val).await;
                            let _ = node_arc.set_entry(&doc_arc, format!("item/{}/check", id), "false").await;
                        }
                        SyncCommand::ToggleItemCheck { id, checked } => {
                            let _ = node_arc.set_entry(&doc_arc, format!("item/{}/check", id), if checked { "true" } else { "false" }).await;
                        }
                        SyncCommand::DeleteItem { id } => {
                            let _ = node_arc.set_entry(&doc_arc, format!("item/{}/deleted", id), "true").await;
                        }
                        SyncCommand::JoinTicket { ticket_str } => {
                            if let Ok(ticket) = ticket_str.parse::<DocTicket>() {
                                let _ = node_arc.join_ticket(&doc_arc, &ticket).await;
                            }
                        }
                    }
                }
                Some(event_res) = async {
                    match &mut events_stream {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Ok(LiveEvent::ContentReady { hash }) = event_res {
                        if let Ok(bytes) = node_arc.blobs().get_bytes(hash).await {
                            let content_str = String::from_utf8_lossy(&bytes).to_string();
                            tracing::info!("Received remote sync update: {}", content_str);
                        }
                    }
                }
                else => break,
            }
        }
    });

    SyncBridgeHandle { sender: tx }
}
