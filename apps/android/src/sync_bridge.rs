//! Reactive sync bridge connecting Slint UI with GrosheSyncNode and ReactiveStore.
//!
//! Handles background Iroh P2P synchronization, CRDT document mutations,
//! multi-list management, live event streaming, and Slint view updates.

use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use groshe_core::{
    parse_quick_add, to_cbor, DocKey, ExportItem, ExportList, ItemCheck, ItemData, ItemId,
    ItemPosition, ListId,
};
use groshe_store::{ItemHistoryTracker, ListProjection};
use groshe_sync::{render_qr_terminal, Doc, GrosheSyncNode, LiveEvent, NamespaceId};
use slint::{ModelRc, VecModel};
use tokio::sync::mpsc;

use crate::{AppWindow, GroceryItemData, GroceryListInfo};

/// Commands dispatched from the Slint UI to the background sync engine.
#[derive(Debug)]
pub enum SyncCommand {
    CreateList { title: String },
    SelectList { id: String },
    DeleteList { id: String },
    RotateList { id: String },
    AddItem { raw_input: String },
    ToggleItemCheck { id: String },
    DeleteItem { id: String },
    UndoDelete,
    JoinTicket { ticket_str: String },
    PruneTombstones,
    ExportActiveListMarkdown,
    ExportActiveListJson,
}

/// Thread-safe handle for sending sync commands from Slint callbacks.
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

fn ns_to_list_id(ns_id: &NamespaceId) -> ListId {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&ns_id.as_bytes()[..16]);
    ListId(uuid::Uuid::from_bytes(bytes))
}

async fn load_projection(
    node: &GrosheSyncNode,
    doc: &Doc,
    list_id: ListId,
) -> Result<ListProjection, Box<dyn std::error::Error + Send + Sync>> {
    let mut proj = ListProjection::new(list_id);
    let entries = node.get_entries_prefix(doc, "").await?;
    for (k, v) in entries {
        let _ = proj.apply_entry(&k, &v, 0);
    }
    Ok(proj)
}

fn projection_to_slint_items(proj: &ListProjection) -> Vec<GroceryItemData> {
    proj.sorted_items()
        .into_iter()
        .map(|item| GroceryItemData {
            id: item.id.to_string().into(),
            name: item.name().into(),
            quantity: item
                .data
                .quantity
                .map(|q| q.to_string())
                .unwrap_or_default()
                .into(),
            unit: item.data.unit.clone().unwrap_or_default().into(),
            category: item.data.category.clone().unwrap_or_default().into(),
            note: item.data.note.clone().unwrap_or_default().into(),
            checked: item.is_checked(),
            position: item.position.to_string().into(),
            has_photo: item.data.photo_hash.is_some(),
        })
        .collect()
}

async fn load_lists_info(node: &GrosheSyncNode) -> Vec<GroceryListInfo> {
    let ids = node.list_ids().await.unwrap_or_default();
    let mut list_infos = Vec::new();
    for id in ids {
        if let Ok(Some(doc)) = node.open_list(id).await {
            let meta = node.get_list_meta(&doc).await.ok().flatten();
            let title = meta
                .as_ref()
                .map(|m| m.title.clone())
                .unwrap_or_else(|| "Untitled List".to_string());
            let list_id = ns_to_list_id(&id);
            let proj = load_projection(node, &doc, list_id)
                .await
                .unwrap_or_else(|_| ListProjection::new(list_id));
            list_infos.push(GroceryListInfo {
                id: id.to_string().into(),
                title: title.into(),
                item_count: proj.item_count() as i32,
                checked_count: proj.checked_count() as i32,
                peer_status: "Online".into(),
                is_online: true,
            });
        }
    }
    list_infos
}

/// Spawns the background sync engine worker.
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

        let author_id = node.author_id().to_string();
        let author_name = format!("user-{}", &author_id[..6.min(author_id.len())]);
        let node_id = node.endpoint().addr().id.to_string();

        // Push identity info to Slint settings
        let aid_clone = author_id.clone();
        let nid_clone = node_id.clone();
        let _ = app_weak.upgrade_in_event_loop(move |app| {
            app.set_author_id(aid_clone.into());
            app.set_node_id(nid_clone.into());
            app.set_relay_status("Connected".into());
        });

        // Load existing document or create default list
        let doc_ids = node.list_ids().await.unwrap_or_default();
        let (mut active_doc, mut active_ns_id) = if let Some(id) = doc_ids.first() {
            match node.open_list(*id).await {
                Ok(Some(d)) => {
                    let _ticket = node.ticket(&d).await.ok();
                    (d, *id)
                }
                _ => match node.create_list_with_meta("My Groceries", &author_name).await {
                    Ok((d, _t, id)) => (d, id),
                    Err(e) => {
                        tracing::error!("Failed to create document: {}", e);
                        return;
                    }
                },
            }
        } else {
            match node.create_list_with_meta("My Groceries", &author_name).await {
                Ok((d, _t, id)) => (d, id),
                Err(e) => {
                    tracing::error!("Failed to create document: {}", e);
                    return;
                }
            }
        };

        // Subscribe to live events on active document
        let mut events_stream = node
            .subscribe_doc(&active_doc)
            .await
            .ok()
            .map(Box::pin);

        let history_path = storage_dir.join("history.cbor");
        let mut history = ItemHistoryTracker::load_from_path(&history_path).unwrap_or_default();

        let get_suggestions = |proj: &ListProjection, hist: &ItemHistoryTracker| -> Vec<slint::SharedString> {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let active_names: Vec<String> = proj.items.values().filter(|i| i.has_data).map(|i| i.name().to_string()).collect();
            hist.suggest_frequent(&active_names, 4, now).into_iter().map(|s| s.display_name.into()).collect()
        };

        // Initial UI state sync
        let list_id = ns_to_list_id(&active_ns_id);
        let mut current_proj = load_projection(&node, &active_doc, list_id)
            .await
            .unwrap_or_else(|_| ListProjection::new(list_id));

        let initial_items = projection_to_slint_items(&current_proj);
        let initial_lists = load_lists_info(&node).await;
        let ticket = node.ticket(&active_doc).await.ok();
        let ticket_str = ticket.as_ref().map(|t| t.to_string()).unwrap_or_default();
        let qr_ascii = render_qr_terminal(&ticket_str).unwrap_or_default();
        let title_str = current_proj.title().to_string();
        let active_id_str = active_ns_id.to_string();

        let initial_suggestions = get_suggestions(&current_proj, &history);
        let _ = app_weak.upgrade_in_event_loop(move |app| {
            app.set_active_list_id(active_id_str.into());
            app.set_active_list_title(title_str.into());
            app.set_ticket_str(ticket_str.into());
            app.set_qr_ascii(qr_ascii.into());
            app.set_items(ModelRc::new(VecModel::from(initial_items)));
            app.set_frequent_suggestions(ModelRc::new(VecModel::from(initial_suggestions)));
            app.set_lists(ModelRc::new(VecModel::from(initial_lists)));
        });

        // Main event loop
        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        SyncCommand::CreateList { title } => {
                            if let Ok((new_doc, new_ticket, new_id)) = node.create_list_with_meta(&title, &author_name).await {
                                active_doc = new_doc;
                                active_ns_id = new_id;
                                events_stream = node.subscribe_doc(&active_doc).await.ok().map(Box::pin);

                                let lid = ns_to_list_id(&active_ns_id);
                                current_proj = load_projection(&node, &active_doc, lid).await.unwrap_or_else(|_| ListProjection::new(lid));

                                let items = projection_to_slint_items(&current_proj);
                                let lists = load_lists_info(&node).await;
                                let t_str = new_ticket.to_string();
                                let qr_str = render_qr_terminal(&t_str).unwrap_or_default();
                                let id_str = active_ns_id.to_string();

                                let _ = app_weak.upgrade_in_event_loop(move |app| {
                                    app.set_active_list_id(id_str.into());
                                    app.set_active_list_title(title.into());
                                    app.set_ticket_str(t_str.into());
                                    app.set_qr_ascii(qr_str.into());
                                    app.set_items(ModelRc::new(VecModel::from(items)));
                                    app.set_lists(ModelRc::new(VecModel::from(lists)));
                                    app.set_active_tab(1); // Navigate to detail
                                });
                            }
                        }

                        SyncCommand::SelectList { id } => {
                            if let Ok(ns_id) = NamespaceId::from_str(&id) {
                                if let Ok(Some(d)) = node.open_list(ns_id).await {
                                    active_doc = d;
                                    active_ns_id = ns_id;
                                    events_stream = node.subscribe_doc(&active_doc).await.ok().map(Box::pin);

                                    let lid = ns_to_list_id(&active_ns_id);
                                    current_proj = load_projection(&node, &active_doc, lid).await.unwrap_or_else(|_| ListProjection::new(lid));

                                    let items = projection_to_slint_items(&current_proj);
                                    let ticket = node.ticket(&active_doc).await.ok();
                                    let t_str = ticket.as_ref().map(|t| t.to_string()).unwrap_or_default();
                                    let qr_str = render_qr_terminal(&t_str).unwrap_or_default();
                                    let title = current_proj.title().to_string();
                                    let id_str = active_ns_id.to_string();

                                    let _ = app_weak.upgrade_in_event_loop(move |app| {
                                        app.set_active_list_id(id_str.into());
                                        app.set_active_list_title(title.into());
                                        app.set_ticket_str(t_str.into());
                                        app.set_qr_ascii(qr_str.into());
                                        app.set_items(ModelRc::new(VecModel::from(items)));
                                    });
                                }
                            }
                        }

                        SyncCommand::DeleteList { id } => {
                            if let Ok(ns_id) = NamespaceId::from_str(&id) {
                                let _ = node.delete_list(ns_id).await;
                                let lists = load_lists_info(&node).await;

                                // If active list was deleted, select another
                                if ns_id == active_ns_id {
                                    if let Some(first_id) = node.list_ids().await.unwrap_or_default().first() {
                                        if let Ok(Some(d)) = node.open_list(*first_id).await {
                                            active_doc = d;
                                            active_ns_id = *first_id;
                                            let lid = ns_to_list_id(&active_ns_id);
                                            current_proj = load_projection(&node, &active_doc, lid).await.unwrap_or_else(|_| ListProjection::new(lid));
                                        }
                                    }
                                }

                                let items = projection_to_slint_items(&current_proj);
                                let title = current_proj.title().to_string();
                                let id_str = active_ns_id.to_string();

                                let _ = app_weak.upgrade_in_event_loop(move |app| {
                                    app.set_active_list_id(id_str.into());
                                    app.set_active_list_title(title.into());
                                    app.set_items(ModelRc::new(VecModel::from(items)));
                                    app.set_lists(ModelRc::new(VecModel::from(lists)));
                                });
                            }
                        }

                        SyncCommand::RotateList { id } => {
                            if let Ok(ns_id) = NamespaceId::from_str(&id) {
                                if let Ok((new_doc, new_ticket, new_id)) = node.rotate_list(ns_id, &author_name).await {
                                    active_doc = new_doc;
                                    active_ns_id = new_id;
                                    events_stream = node.subscribe_doc(&active_doc).await.ok().map(Box::pin);

                                    let lid = ns_to_list_id(&active_ns_id);
                                    current_proj = load_projection(&node, &active_doc, lid).await.unwrap_or_else(|_| ListProjection::new(lid));

                                    let items = projection_to_slint_items(&current_proj);
                                    let lists = load_lists_info(&node).await;
                                    let t_str = new_ticket.to_string();
                                    let qr_str = render_qr_terminal(&t_str).unwrap_or_default();
                                    let title = current_proj.title().to_string();
                                    let id_str = active_ns_id.to_string();

                                    let _ = app_weak.upgrade_in_event_loop(move |app| {
                                        app.set_active_list_id(id_str.into());
                                        app.set_active_list_title(title.into());
                                        app.set_ticket_str(t_str.into());
                                        app.set_qr_ascii(qr_str.into());
                                        app.set_items(ModelRc::new(VecModel::from(items)));
                                        app.set_lists(ModelRc::new(VecModel::from(lists)));
                                    });
                                }
                            }
                        }

                        SyncCommand::AddItem { raw_input } => {
                            let parsed = parse_quick_add(&raw_input);
                            if !parsed.name.is_empty() {
                                if let Ok(mut data) = ItemData::new(&parsed.name) {
                                    if let Some(q) = parsed.quantity {
                                        if let Ok(d) = data.clone().with_quantity(q, parsed.unit) {
                                            data = d;
                                        }
                                    }
                                    if let Some(c) = parsed.category {
                                        data = data.with_category(c);
                                    }

                                    let item_id = ItemId::new_v7();
                                    let last_pos = current_proj.sorted_items().last().map(|i| &i.position);
                                    let next_pos = if let Some(last) = last_pos {
                                        ItemPosition::after(last).unwrap_or_else(|_| ItemPosition::initial())
                                    } else {
                                        ItemPosition::initial()
                                    };

                                    if let Ok(data_bytes) = to_cbor(&data) {
                                        let _ = node.set_entry(&active_doc, DocKey::item_data(item_id).to_key_bytes(), data_bytes).await;
                                    }
                                    if let Ok(pos_bytes) = to_cbor(&next_pos) {
                                        let _ = node.set_entry(&active_doc, DocKey::item_pos(item_id).to_key_bytes(), pos_bytes).await;
                                    }

                                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                                    history.record_item(&parsed.name, data.category.as_deref(), data.quantity, data.unit.as_deref(), now);
                                    let _ = history.save_to_path(&history_path);

                                    let lid = ns_to_list_id(&active_ns_id);
                                    if let Ok(p) = load_projection(&node, &active_doc, lid).await {
                                        current_proj = p;
                                        let items = projection_to_slint_items(&current_proj);
                                        let suggestions = get_suggestions(&current_proj, &history);
                                        let lists = load_lists_info(&node).await;
                                        let _ = app_weak.upgrade_in_event_loop(move |app| {
                                            app.set_items(ModelRc::new(VecModel::from(items)));
                                            app.set_frequent_suggestions(ModelRc::new(VecModel::from(suggestions)));
                                            app.set_lists(ModelRc::new(VecModel::from(lists)));
                                        });
                                    }
                                }
                            }
                        }

                        SyncCommand::ToggleItemCheck { id } => {
                            if let Ok(item_id) = ItemId::from_str(&id) {
                                let new_checked = !current_proj.get_item(&item_id).map(|i| i.is_checked()).unwrap_or(false);
                                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                                let check = if new_checked {
                                    ItemCheck::checked(now, node.author_id().to_string())
                                } else {
                                    ItemCheck::unchecked()
                                };

                                if let Ok(check_bytes) = to_cbor(&check) {
                                    let _ = node.set_entry(&active_doc, DocKey::item_check(item_id).to_key_bytes(), check_bytes).await;
                                }

                                let lid = ns_to_list_id(&active_ns_id);
                                if let Ok(p) = load_projection(&node, &active_doc, lid).await {
                                    current_proj = p;
                                    let items = projection_to_slint_items(&current_proj);
                                    let lists = load_lists_info(&node).await;
                                    let _ = app_weak.upgrade_in_event_loop(move |app| {
                                        app.set_items(ModelRc::new(VecModel::from(items)));
                                        app.set_lists(ModelRc::new(VecModel::from(lists)));
                                    });
                                }
                            }
                        }

                        SyncCommand::DeleteItem { id } => {
                            if let Ok(item_id) = ItemId::from_str(&id) {
                                let _ = node.delete_entry(&active_doc, format!("item/{item_id}/")).await;

                                let lid = ns_to_list_id(&active_ns_id);
                                if let Ok(p) = load_projection(&node, &active_doc, lid).await {
                                    current_proj = p;
                                    let items = projection_to_slint_items(&current_proj);
                                    let lists = load_lists_info(&node).await;
                                    let _ = app_weak.upgrade_in_event_loop(move |app| {
                                        app.set_items(ModelRc::new(VecModel::from(items)));
                                        app.set_lists(ModelRc::new(VecModel::from(lists)));
                                        app.set_undo_toast_message("Item deleted".into());
                                        app.set_show_undo_toast(true);
                                    });
                                }
                            }
                        }

                        SyncCommand::UndoDelete => {
                            if let Ok(restored) = current_proj.restore_deleted(None) {
                                let item_id = restored.id;
                                if let Ok(data_bytes) = to_cbor(&restored.data) {
                                    let _ = node.set_entry(&active_doc, DocKey::item_data(item_id).to_key_bytes(), data_bytes).await;
                                }
                                if let Ok(pos_bytes) = to_cbor(&restored.position) {
                                    let _ = node.set_entry(&active_doc, DocKey::item_pos(item_id).to_key_bytes(), pos_bytes).await;
                                }

                                let lid = ns_to_list_id(&active_ns_id);
                                if let Ok(p) = load_projection(&node, &active_doc, lid).await {
                                    current_proj = p;
                                    let items = projection_to_slint_items(&current_proj);
                                    let lists = load_lists_info(&node).await;
                                    let _ = app_weak.upgrade_in_event_loop(move |app| {
                                        app.set_items(ModelRc::new(VecModel::from(items)));
                                        app.set_lists(ModelRc::new(VecModel::from(lists)));
                                        app.set_show_undo_toast(false);
                                    });
                                }
                            }
                        }

                        SyncCommand::JoinTicket { ticket_str } => {
                            if let Ok((doc, ticket)) = node.import_list_str(&ticket_str).await {
                                active_doc = doc;
                                active_ns_id = active_doc.id();
                                events_stream = node.subscribe_doc(&active_doc).await.ok().map(Box::pin);

                                let lid = ns_to_list_id(&active_ns_id);
                                current_proj = load_projection(&node, &active_doc, lid).await.unwrap_or_else(|_| ListProjection::new(lid));

                                let items = projection_to_slint_items(&current_proj);
                                let lists = load_lists_info(&node).await;
                                let t_str = ticket.to_string();
                                let qr_str = render_qr_terminal(&t_str).unwrap_or_default();
                                let title = current_proj.title().to_string();
                                let id_str = active_ns_id.to_string();

                                let _ = app_weak.upgrade_in_event_loop(move |app| {
                                    app.set_active_list_id(id_str.into());
                                    app.set_active_list_title(title.into());
                                    app.set_ticket_str(t_str.into());
                                    app.set_qr_ascii(qr_str.into());
                                    app.set_items(ModelRc::new(VecModel::from(items)));
                                    app.set_lists(ModelRc::new(VecModel::from(lists)));
                                    app.set_active_tab(1); // Navigate to detail
                                });
                            }
                        }

                        SyncCommand::PruneTombstones => {
                            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                            current_proj.prune_tombstones(now);
                        }

                        SyncCommand::ExportActiveListMarkdown => {
                            let meta = node.get_list_meta(&active_doc).await.ok().flatten();
                            let created_by = meta.as_ref().map(|m| m.created_by.clone()).unwrap_or_default();
                            let created_at = meta.as_ref().map(|m| m.created_at).unwrap_or(0);
                            let export_list = ExportList::new(
                                current_proj.title().to_string(),
                                created_by,
                                created_at,
                                current_proj
                                    .sorted_items()
                                    .into_iter()
                                    .map(|i| ExportItem {
                                        id: i.id,
                                        name: i.name().to_string(),
                                        quantity: i.data.quantity,
                                        unit: i.data.unit.clone(),
                                        category: i.data.category.clone(),
                                        note: i.data.note.clone(),
                                        checked: i.is_checked(),
                                        position: i.position.to_string(),
                                        photo_hash: i.data.photo_hash_hex(),
                                    })
                                    .collect(),
                            );
                            let md = export_list.to_markdown();
                            let exports_dir = storage_dir.join("exports");
                            let _ = std::fs::create_dir_all(&exports_dir);
                            let safe_title = current_proj.title().replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
                            let filename = format!("{}.md", safe_title);
                            let target_path = exports_dir.join(&filename);
                            let status_msg = match std::fs::write(&target_path, md) {
                                Ok(()) => format!("Exported to exports/{}", filename),
                                Err(e) => format!("Export failed: {}", e),
                            };
                            let _ = app_weak.upgrade_in_event_loop(move |app| {
                                app.set_export_status_message(status_msg.into());
                                app.set_show_export_status(true);
                            });
                        }

                        SyncCommand::ExportActiveListJson => {
                            let meta = node.get_list_meta(&active_doc).await.ok().flatten();
                            let created_by = meta.as_ref().map(|m| m.created_by.clone()).unwrap_or_default();
                            let created_at = meta.as_ref().map(|m| m.created_at).unwrap_or(0);
                            let export_list = ExportList::new(
                                current_proj.title().to_string(),
                                created_by,
                                created_at,
                                current_proj
                                    .sorted_items()
                                    .into_iter()
                                    .map(|i| ExportItem {
                                        id: i.id,
                                        name: i.name().to_string(),
                                        quantity: i.data.quantity,
                                        unit: i.data.unit.clone(),
                                        category: i.data.category.clone(),
                                        note: i.data.note.clone(),
                                        checked: i.is_checked(),
                                        position: i.position.to_string(),
                                        photo_hash: i.data.photo_hash_hex(),
                                    })
                                    .collect(),
                            );
                            let safe_title = current_proj.title().replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
                            let filename = format!("{}.json", safe_title);
                            let exports_dir = storage_dir.join("exports");
                            let _ = std::fs::create_dir_all(&exports_dir);
                            let target_path = exports_dir.join(&filename);
                            let status_msg = match export_list.to_json_pretty() {
                                Ok(json) => match std::fs::write(&target_path, json) {
                                    Ok(()) => format!("Backup saved to exports/{}", filename),
                                    Err(e) => format!("Backup write failed: {}", e),
                                },
                                Err(e) => format!("JSON encode failed: {}", e),
                            };
                            let _ = app_weak.upgrade_in_event_loop(move |app| {
                                app.set_export_status_message(status_msg.into());
                                app.set_show_export_status(true);
                            });
                        }
                    }
                }

                // Handle incoming remote sync events over QUIC / live Iroh doc stream
                Some(event_res) = async {
                    match &mut events_stream {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Ok(LiveEvent::InsertRemote { .. } | LiveEvent::ContentReady { .. }) = event_res {
                        let lid = ns_to_list_id(&active_ns_id);
                        if let Ok(p) = load_projection(&node, &active_doc, lid).await {
                            current_proj = p;
                            let items = projection_to_slint_items(&current_proj);
                            let lists = load_lists_info(&node).await;
                            let _ = app_weak.upgrade_in_event_loop(move |app| {
                                app.set_items(ModelRc::new(VecModel::from(items)));
                                app.set_lists(ModelRc::new(VecModel::from(lists)));
                            });
                        }
                    }
                }

                else => break,
            }
        }
    });

    SyncBridgeHandle { sender: tx }
}
