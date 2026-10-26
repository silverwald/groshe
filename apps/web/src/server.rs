//! Axum web server and REST/WebSocket endpoints for Groshe Web Companion.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use groshe_core::{
    from_cbor,
    model::{ItemCheck, ItemData, ItemId, ListId},
    position::ItemPosition,
    to_cbor, DocKey,
};
use groshe_store::ListProjection;
use groshe_sync::{Doc, GrosheSyncNode, NamespaceId};
use std::str::FromStr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::api::{
    AddItemRequest, ItemDto, ListDetailDto, ListSummaryDto, NodeStatusDto, ToggleCheckRequest,
    UpdateItemRequest,
};

#[derive(Clone)]
pub struct AppState {
    pub node: Arc<GrosheSyncNode>,
}

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

pub fn ns_to_list_id(ns_id: &NamespaceId) -> ListId {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&ns_id.as_bytes()[..16]);
    ListId(uuid::Uuid::from_bytes(bytes))
}

pub async fn load_projection(
    node: &GrosheSyncNode,
    doc: &Doc,
    list_id: ListId,
) -> Result<ListProjection, AppError> {
    let mut proj = ListProjection::new(list_id);
    let entries = node
        .get_entries_prefix(doc, "")
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    for (k, v) in entries {
        let _ = proj.apply_entry(&k, &v, 0);
    }
    Ok(proj)
}

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/lists", get(get_lists))
        .route("/api/lists/:id", get(get_list))
        .route("/api/lists/:id/items", post(add_item))
        .route("/api/lists/:id/items/:item_id", patch(update_item))
        .route("/api/lists/:id/check/:item_id", post(toggle_check))
        .route("/api/lists/:id/items/:item_id", delete(delete_item))
        .route("/api/photos/:hash", get(get_photo))
        .route("/api/ws/:id", get(ws_handler))
        .route("/", get(serve_index))
        .layer(cors)
        .with_state(state)
}

async fn get_status(State(state): State<AppState>) -> Result<Json<NodeStatusDto>, AppError> {
    let addr = state.node.endpoint().addr();
    let relay_str = if let Some(r) = state.node.config().custom_relay_url() {
        format!("Custom: {r}")
    } else if state.node.config().is_relay_disabled() {
        "Disabled (LAN only)".to_string()
    } else {
        "Default (Public n0 Production)".to_string()
    };
    let lists = state
        .node
        .list_ids()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(NodeStatusDto {
        node_id: addr.id.to_string(),
        author_id: state.node.author_id().to_string(),
        relay: relay_str,
        lists_count: lists.len(),
    }))
}

async fn get_lists(State(state): State<AppState>) -> Result<Json<Vec<ListSummaryDto>>, AppError> {
    let ids = state
        .node
        .list_ids()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let mut summaries = Vec::new();
    for id in ids {
        if let Ok(Some(doc)) = state.node.open_list(id).await {
            let meta = state.node.get_list_meta(&doc).await.ok().flatten();
            let lid = ns_to_list_id(&id);
            let proj = load_projection(&state.node, &doc, lid).await?;
            summaries.push(ListSummaryDto {
                id: id.to_string(),
                title: meta
                    .as_ref()
                    .map(|m| m.title.clone())
                    .unwrap_or_else(|| "Untitled List".to_string()),
                created_by: meta
                    .as_ref()
                    .map(|m| m.created_by.clone())
                    .unwrap_or_default(),
                created_at: meta.as_ref().map(|m| m.created_at).unwrap_or(0),
                item_count: proj.item_count(),
                checked_count: proj.checked_count(),
            });
        }
    }
    Ok(Json(summaries))
}

async fn get_list(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<ListDetailDto>, AppError> {
    let ns_id = NamespaceId::from_str(&id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let doc = state
        .node
        .open_list(ns_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("List '{id_str}' not found")))?;
    let lid = ns_to_list_id(&ns_id);
    let proj = load_projection(&state.node, &doc, lid).await?;
    let meta = state.node.get_list_meta(&doc).await.ok().flatten();

    let items = proj
        .sorted_items()
        .into_iter()
        .map(|item| ItemDto {
            id: item.id.to_string(),
            name: item.name().to_string(),
            quantity: item.data.quantity,
            unit: item.data.unit.clone(),
            category: item.data.category.clone(),
            note: item.data.note.clone(),
            checked: item.is_checked(),
            position: item.position.to_string(),
            photo_hash: item
                .data
                .photo_hash
                .map(|h| h.iter().map(|b| format!("{:02x}", b)).collect()),
        })
        .collect();

    Ok(Json(ListDetailDto {
        id: id_str,
        title: meta
            .as_ref()
            .map(|m| m.title.clone())
            .unwrap_or_else(|| "Untitled List".to_string()),
        created_by: meta
            .as_ref()
            .map(|m| m.created_by.clone())
            .unwrap_or_default(),
        created_at: meta.as_ref().map(|m| m.created_at).unwrap_or(0),
        items,
    }))
}

async fn add_item(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Json(req): Json<AddItemRequest>,
) -> Result<(StatusCode, Json<ItemDto>), AppError> {
    let ns_id = NamespaceId::from_str(&id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let doc = state
        .node
        .open_list(ns_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("List '{id_str}' not found")))?;
    let lid = ns_to_list_id(&ns_id);
    let proj = load_projection(&state.node, &doc, lid).await?;

    let parsed = groshe_core::parser::parse_quick_add(&req.raw_input);
    let mut item_data =
        ItemData::new(&parsed.name).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if let Some(q) = parsed.quantity {
        item_data = item_data
            .with_quantity(q, parsed.unit)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
    }
    if let Some(c) = req.category.or(parsed.category) {
        item_data = item_data.with_category(c);
    }

    if let Some(ref b64) = req.photo_base64 {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| AppError::BadRequest(format!("Invalid base64 photo data: {e}")))?;
        let hash = state
            .node
            .add_photo_blob(&bytes)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        item_data = item_data.with_photo_hash(hash);
    }

    let last_pos = proj.sorted_items().last().map(|i| &i.position);
    let next_pos = if let Some(last) = last_pos {
        ItemPosition::after(last).map_err(|e| AppError::Internal(e.to_string()))?
    } else {
        ItemPosition::initial()
    };

    let item_id = ItemId::new_v7();
    let check = ItemCheck::unchecked();

    state
        .node
        .set_entry(
            &doc,
            DocKey::item_data(item_id).to_key_bytes(),
            to_cbor(&item_data).map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    state
        .node
        .set_entry(
            &doc,
            DocKey::item_check(item_id).to_key_bytes(),
            to_cbor(&check).map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    state
        .node
        .set_entry(
            &doc,
            DocKey::item_pos(item_id).to_key_bytes(),
            to_cbor(&next_pos).map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let created_dto = ItemDto {
        id: item_id.to_string(),
        name: item_data.name,
        quantity: item_data.quantity,
        unit: item_data.unit,
        category: item_data.category,
        note: item_data.note,
        checked: false,
        position: next_pos.to_string(),
        photo_hash: item_data
            .photo_hash
            .map(|h| h.iter().map(|b| format!("{:02x}", b)).collect()),
    };

    Ok((StatusCode::CREATED, Json(created_dto)))
}

async fn update_item(
    State(state): State<AppState>,
    Path((id_str, item_id_str)): Path<(String, String)>,
    Json(req): Json<UpdateItemRequest>,
) -> Result<Json<ItemDto>, AppError> {
    let ns_id = NamespaceId::from_str(&id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let item_id =
        ItemId::from_str(&item_id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let doc = state
        .node
        .open_list(ns_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("List '{id_str}' not found")))?;

    let data_key = DocKey::item_data(item_id).to_key_bytes();
    let current_bytes = state
        .node
        .get_entry(&doc, &data_key)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Item '{item_id_str}' not found")))?;

    let mut current_data: ItemData =
        from_cbor(&current_bytes).map_err(|e| AppError::Internal(e.to_string()))?;

    if let Some(ref raw) = req.raw_input {
        let parsed = groshe_core::parser::parse_quick_add(raw);
        if !parsed.name.trim().is_empty() {
            current_data.name = parsed.name;
        }
        if let Some(q) = parsed.quantity {
            current_data.quantity = Some(q);
            current_data.unit = parsed.unit;
        }
        if let Some(c) = parsed.category {
            current_data.category = Some(c);
        }
    }

    if let Some(n) = req.name {
        if !n.trim().is_empty() {
            current_data.name = n.trim().to_string();
        }
    }
    if let Some(q) = req.quantity {
        current_data.quantity = Some(q);
    }
    if let Some(u) = req.unit {
        current_data.unit = Some(u);
    }
    if let Some(c) = req.category {
        current_data.category = Some(c);
    }
    if let Some(nt) = req.note {
        current_data.note = Some(nt);
    }

    if let Some(ref b64) = req.photo_base64 {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| AppError::BadRequest(format!("Invalid base64 photo data: {e}")))?;
        let hash = state
            .node
            .add_photo_blob(&bytes)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        current_data = current_data.with_photo_hash(hash);
    }

    state
        .node
        .set_entry(
            &doc,
            &data_key,
            to_cbor(&current_data).map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let lid = ns_to_list_id(&ns_id);
    let proj = load_projection(&state.node, &doc, lid).await?;
    let updated_item = proj.get_item(&item_id).ok_or_else(|| {
        AppError::NotFound(format!("Item '{item_id_str}' not found in projection"))
    })?;

    Ok(Json(ItemDto {
        id: item_id.to_string(),
        name: updated_item.data.name.clone(),
        quantity: updated_item.data.quantity,
        unit: updated_item.data.unit.clone(),
        category: updated_item.data.category.clone(),
        note: updated_item.data.note.clone(),
        checked: updated_item.is_checked(),
        position: updated_item.position.to_string(),
        photo_hash: updated_item
            .data
            .photo_hash
            .map(|h| h.iter().map(|b| format!("{:02x}", b)).collect()),
    }))
}

async fn toggle_check(
    State(state): State<AppState>,
    Path((id_str, item_id_str)): Path<(String, String)>,
    Json(req): Json<ToggleCheckRequest>,
) -> Result<StatusCode, AppError> {
    let ns_id = NamespaceId::from_str(&id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let item_id =
        ItemId::from_str(&item_id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let doc = state
        .node
        .open_list(ns_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("List '{id_str}' not found")))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let check = if req.checked {
        ItemCheck::checked(now, state.node.author_id().to_string())
    } else {
        ItemCheck::unchecked()
    };

    state
        .node
        .set_entry(
            &doc,
            DocKey::item_check(item_id).to_key_bytes(),
            to_cbor(&check).map_err(|e| AppError::Internal(e.to_string()))?,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::OK)
}

async fn delete_item(
    State(state): State<AppState>,
    Path((id_str, item_id_str)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let ns_id = NamespaceId::from_str(&id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let item_id =
        ItemId::from_str(&item_id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let doc = state
        .node
        .open_list(ns_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("List '{id_str}' not found")))?;

    state
        .node
        .delete_entry(&doc, format!("item/{item_id}/"))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_photo(
    State(state): State<AppState>,
    Path(hash_str): Path<String>,
) -> Result<Response, AppError> {
    let trimmed = hash_str.trim();
    if trimmed.len() != 64 {
        return Err(AppError::BadRequest(
            "Invalid 64-character hex photo hash".into(),
        ));
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        let hi = (trimmed.as_bytes()[i * 2] as char)
            .to_digit(16)
            .ok_or_else(|| AppError::BadRequest("Invalid hex".into()))? as u8;
        let lo = (trimmed.as_bytes()[i * 2 + 1] as char)
            .to_digit(16)
            .ok_or_else(|| AppError::BadRequest("Invalid hex".into()))? as u8;
        *byte = (hi << 4) | lo;
    }

    let blob = state
        .node
        .get_photo_blob(&bytes)
        .await
        .map_err(|e| AppError::NotFound(format!("Photo not found: {e}")))?;

    Ok(([(axum::http::header::CONTENT_TYPE, "image/jpeg")], blob).into_response())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Response, AppError> {
    let ns_id = NamespaceId::from_str(&id_str).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let doc = state
        .node
        .open_list(ns_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("List '{id_str}' not found")))?;

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state.node, doc, ns_id)))
}

async fn handle_socket(socket: WebSocket, node: Arc<GrosheSyncNode>, doc: Doc, ns_id: NamespaceId) {
    let (mut sender, mut receiver) = socket.split();

    // 1. Send initial projection snapshot
    let lid = ns_to_list_id(&ns_id);
    if let Ok(proj) = load_projection(&node, &doc, lid).await {
        let items: Vec<ItemDto> = proj
            .sorted_items()
            .into_iter()
            .map(|item| ItemDto {
                id: item.id.to_string(),
                name: item.name().to_string(),
                quantity: item.data.quantity,
                unit: item.data.unit.clone(),
                category: item.data.category.clone(),
                note: item.data.note.clone(),
                checked: item.is_checked(),
                position: item.position.to_string(),
                photo_hash: item
                    .data
                    .photo_hash
                    .map(|h| h.iter().map(|b| format!("{:02x}", b)).collect()),
            })
            .collect();

        let initial_msg = serde_json::json!({
            "type": "initial",
            "list_id": ns_id.to_string(),
            "title": proj.title(),
            "items": items,
        });
        let _ = sender.send(Message::Text(initial_msg.to_string())).await;
    }

    // 2. Subscribe to live events on document
    let mut events = match node.subscribe_doc(&doc).await {
        Ok(s) => s,
        Err(_) => return,
    };

    loop {
        tokio::select! {
            event = events.next() => {
                match event {
                    Some(Ok(_)) => {
                        // Refresh projection and notify client
                        if let Ok(proj) = load_projection(&node, &doc, lid).await {
                            let items: Vec<ItemDto> = proj.sorted_items().into_iter().map(|item| ItemDto {
                                id: item.id.to_string(),
                                name: item.name().to_string(),
                                quantity: item.data.quantity,
                                unit: item.data.unit.clone(),
                                category: item.data.category.clone(),
                                note: item.data.note.clone(),
                                checked: item.is_checked(),
                                position: item.position.to_string(),
                                photo_hash: item.data.photo_hash.map(|h| h.iter().map(|b| format!("{:02x}", b)).collect()),
                            }).collect();
                            let update_msg = serde_json::json!({
                                "type": "update",
                                "items": items,
                            });
                            if sender.send(Message::Text(update_msg.to_string())).await.is_err() {
                                break;
                            }
                        }
                    }
                    _ => break,
                }
            }
            client_msg = receiver.next() => {
                match client_msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Text(text))) => {
                        // Inbound client command handling (e.g. check / ping)
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                            if val.get("type").and_then(|t| t.as_str()) == Some("ping") {
                                let _ = sender.send(Message::Text(r#"{"type":"pong"}"#.into())).await;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn serve_index() -> impl IntoResponse {
    axum::response::Html(crate::frontend::INDEX_HTML)
}
