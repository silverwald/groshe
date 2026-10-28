use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use futures_util::{SinkExt, StreamExt};
use groshe_sync::GrosheSyncNode;
use groshe_web::{create_router, AppState, ItemDto, ListDetailDto, ListSummaryDto, NodeStatusDto};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn test_web_status_and_lists_endpoints() {
    let node = Arc::new(GrosheSyncNode::memory().await.unwrap());
    let (_doc, _ticket, id) = node.create_list_with_meta("Farmers Market", "alice").await.unwrap();

    let app = create_router(AppState { node });

    // 1. GET /api/status
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let status_dto: NodeStatusDto = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(status_dto.lists_count, 1);
    assert!(!status_dto.node_id.is_empty());

    // 2. GET /api/lists
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/lists")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let lists: Vec<ListSummaryDto> = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].id, id.to_string());
    assert_eq!(lists[0].title, "Farmers Market");
    assert_eq!(lists[0].created_by, "alice");
    assert_eq!(lists[0].item_count, 0);
}

#[tokio::test]
async fn test_web_crud_item_flow() {
    let node = Arc::new(GrosheSyncNode::memory().await.unwrap());
    let (_doc, _ticket, id) = node.create_list_with_meta("Weekly Groceries", "bob").await.unwrap();
    let app = create_router(AppState { node });

    // 1. POST /api/lists/:id/items (quick-add parsing)
    let payload = serde_json::json!({
        "raw_input": "2 gal organic milk"
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{id}/items"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let created: ItemDto = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(created.name, "organic milk");
    assert_eq!(created.quantity, Some(2.0));
    assert_eq!(created.unit, Some("gal".to_string()));
    assert_eq!(created.category, Some("Dairy & Eggs".to_string()));
    assert!(!created.checked);

    let item_id = created.id;

    // 2. GET /api/lists/:id
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/lists/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let detail: ListDetailDto = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(detail.items.len(), 1);
    assert_eq!(detail.items[0].id, item_id);

    // 3. POST /api/lists/:id/check/:item_id (toggle checked)
    let check_payload = serde_json::json!({ "checked": true });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{id}/check/{item_id}"))
                .header("content-type", "application/json")
                .body(Body::from(check_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify checked state
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/lists/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let detail: ListDetailDto = serde_json::from_slice(&body_bytes).unwrap();
    assert!(detail.items[0].checked);

    // 4. DELETE /api/lists/:id/items/:item_id
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/lists/{id}/items/{item_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify list is now empty
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/lists/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let detail: ListDetailDto = serde_json::from_slice(&body_bytes).unwrap();
    assert!(detail.items.is_empty());
}

#[tokio::test]
async fn test_web_photo_blob_api() {
    let node = Arc::new(GrosheSyncNode::memory().await.unwrap());
    let (_doc, _ticket, id) = node.create_list_with_meta("Photo List", "charlie").await.unwrap();

    // Store a thumbnail photo blob
    let sample_image = vec![0xCA, 0xFE, 0xBA, 0xBE];
    let hash = node.add_photo_blob(&sample_image).await.unwrap();
    let hex_hash: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

    let app = create_router(AppState { node });

    // 1. GET /api/photos/:hash
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/photos/{hex_hash}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), sample_image.as_slice());

    // 2. Add item with base64 photo
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&sample_image);
    let payload = serde_json::json!({
        "raw_input": "Heirloom Tomatoes",
        "photo_base64": b64
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{id}/items"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let item: ItemDto = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(item.name, "Heirloom Tomatoes");
    assert_eq!(item.photo_hash, Some(hex_hash));
}

#[tokio::test]
async fn test_web_socket_streaming_and_mutation() {
    let node = Arc::new(GrosheSyncNode::memory().await.unwrap());
    let (doc, _ticket, id) = node.create_list_with_meta("Live Market", "david").await.unwrap();

    let app = create_router(AppState { node: node.clone() });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let ws_url = format!("ws://{addr}/api/ws/{id}");
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    // 1. Initial snapshot message
    let msg = ws_stream.next().await.unwrap().unwrap();
    let initial: serde_json::Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert_eq!(initial["type"], "initial");
    assert_eq!(initial["title"], "Live Market");

    // 2. Add an item through sync node directly (simulating peer synchronization)
    let item_id = groshe_core::ItemId::new_v7();
    let item_data = groshe_core::ItemData::new("Gala Apples").unwrap();
    let check = groshe_core::ItemCheck::unchecked();
    let pos = groshe_core::ItemPosition::initial();
    node.set_entry(&doc, groshe_core::DocKey::item_data(item_id).to_key_bytes(), groshe_core::to_cbor(&item_data).unwrap()).await.unwrap();
    node.set_entry(&doc, groshe_core::DocKey::item_check(item_id).to_key_bytes(), groshe_core::to_cbor(&check).unwrap()).await.unwrap();
    node.set_entry(&doc, groshe_core::DocKey::item_pos(item_id).to_key_bytes(), groshe_core::to_cbor(&pos).unwrap()).await.unwrap();

    // 3. Client receives update over WebSocket
    let msg = ws_stream.next().await.unwrap().unwrap();
    let update: serde_json::Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert_eq!(update["type"], "update");
    assert_eq!(update["items"].as_array().unwrap().len(), 1);
    assert_eq!(update["items"][0]["name"], "Gala Apples");

    // 4. Send ping from client, assert pong
    ws_stream.send(tokio_tungstenite::tungstenite::Message::Text(r#"{"type":"ping"}"#.into())).await.unwrap();
    let mut got_pong = false;
    while let Some(Ok(m)) = ws_stream.next().await {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(m.to_text().unwrap()) {
            if val["type"] == "pong" {
                got_pong = true;
                break;
            }
        }
    }
    assert!(got_pong, "Expected pong response from server");
}

#[tokio::test]
async fn test_web_reorder_item_endpoints() {
    let node = Arc::new(GrosheSyncNode::memory().await.unwrap());
    let (_doc, _ticket, id) = node.create_list_with_meta("Reorder List", "dan").await.unwrap();
    let app = create_router(AppState { node: Arc::clone(&node) });

    // 1. Add 3 items in sequence: A, B, C
    let mut item_ids = Vec::new();
    for name in ["Apples", "Bananas", "Carrots"] {
        let payload = serde_json::json!({ "raw_input": name });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/lists/{id}/items"))
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let item: ItemDto = serde_json::from_slice(&bytes).unwrap();
        item_ids.push(item.id);
    }

    // 2. Initial order should be: Apples (0), Bananas (1), Carrots (2)
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/lists/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: ListDetailDto = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(detail.items.len(), 3);
    assert_eq!(detail.items[0].name, "Apples");
    assert_eq!(detail.items[1].name, "Bananas");
    assert_eq!(detail.items[2].name, "Carrots");

    // 3. Move Carrots up: should become Apples, Carrots, Bananas
    let carrots_id = &item_ids[2];
    let reorder_payload = serde_json::json!({ "direction": "up" });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{id}/pos/{carrots_id}"))
                .header("content-type", "application/json")
                .body(Body::from(reorder_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/lists/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: ListDetailDto = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(detail.items[0].name, "Apples");
    assert_eq!(detail.items[1].name, "Carrots");
    assert_eq!(detail.items[2].name, "Bananas");

    // 4. Move Bananas to top: should become Bananas, Apples, Carrots
    let bananas_id = &item_ids[1];
    let reorder_top_payload = serde_json::json!({ "direction": "top" });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{id}/pos/{bananas_id}"))
                .header("content-type", "application/json")
                .body(Body::from(reorder_top_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/lists/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: ListDetailDto = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(detail.items[0].name, "Bananas");
    assert_eq!(detail.items[1].name, "Apples");
    assert_eq!(detail.items[2].name, "Carrots");

    // 5. Move Apples after Carrots using after_id: should become Bananas, Carrots, Apples
    let apples_id = &item_ids[0];
    let reorder_after_payload = serde_json::json!({ "after_id": carrots_id });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{id}/pos/{apples_id}"))
                .header("content-type", "application/json")
                .body(Body::from(reorder_after_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = create_router(AppState { node })
        .oneshot(
            Request::builder()
                .uri(format!("/api/lists/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let detail: ListDetailDto = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(detail.items[0].name, "Bananas");
    assert_eq!(detail.items[1].name, "Carrots");
    assert_eq!(detail.items[2].name, "Apples");
}
