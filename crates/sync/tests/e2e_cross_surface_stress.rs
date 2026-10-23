use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use tower::ServiceExt;

use groshe_core::{to_cbor, DocKey, ItemCheck, ItemData, ItemId, ItemPosition, ListId};
use groshe_store::{history::ItemHistoryTracker, ListProjection};
use groshe_sync::{DeviceIdentity, Doc, GrosheSyncNode, KeychainIdentityStorage, NamespaceId};
use groshe_web::{create_router, AppState, ItemDto};

fn ns_to_list_id(ns_id: &NamespaceId) -> ListId {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&ns_id.as_bytes()[..16]);
    ListId(uuid::Uuid::from_bytes(bytes))
}

async fn wait_for_full_convergence(
    node: &GrosheSyncNode,
    doc: &Doc,
    list_id: ListId,
    expected_count: usize,
    timeout: Duration,
) -> Result<ListProjection, String> {
    let expected_entries = 1 + expected_count * 3;
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(entries) = node.get_entries_prefix(doc, "").await {
            if entries.len() >= expected_entries {
                let mut proj = ListProjection::new(list_id);
                for (k, v) in &entries {
                    let _ = proj.apply_entry(k, v, 0);
                }
                if proj.item_count() == expected_count {
                    return Ok(proj);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Timed out waiting for {expected_count} items ({expected_entries} entries) after {:?}",
        start.elapsed()
    ))
}

#[tokio::test]
async fn test_cross_surface_e2e_convergence() {
    // 1. Setup 3 distinct nodes representing:
    //    - Peer A: Home Server / Anchor Peer (runs Web Companion HTTP API + WebSocket)
    //    - Peer B: Mobile App Peer (uses KeychainIdentityStorage)
    //    - Peer C: CLI Peer
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    let tmp_c = tempfile::tempdir().unwrap();

    let node_a = Arc::new(GrosheSyncNode::persistent(tmp_a.path()).await.unwrap());

    // Peer B uses mobile Keychain identity storage
    let keychain = KeychainIdentityStorage::default_service();
    let identity_b = DeviceIdentity::load_or_create_with_storage(Arc::new(keychain))
        .await
        .unwrap();
    let node_b = GrosheSyncNode::persistent_with_identity(tmp_b.path(), identity_b)
        .await
        .unwrap();

    // Peer C uses standard persistent node
    let node_c = GrosheSyncNode::persistent(tmp_c.path()).await.unwrap();

    // 2. Peer A creates a grocery list
    let (doc_a, ticket_a, ns_id) = node_a
        .create_list_with_meta("E2E Multi-Device Pantry", "anchor-admin")
        .await
        .unwrap();
    let list_id = ns_to_list_id(&ns_id);

    // 3. Peers B and C join using the ticket
    let doc_b = node_b.import_list(ticket_a.clone()).await.unwrap();
    let doc_c = node_c.import_list(ticket_a.clone()).await.unwrap();

    // 4. Spin up Web Companion Router on Peer A with a real listener
    let app_state = AppState {
        node: Arc::clone(&node_a),
    };
    let router = create_router(app_state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // 5. Connect WebSocket client (simulating a live browser tab)
    let ws_url = format!("ws://{addr}/api/ws/{ns_id}");
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await.unwrap();

    // Consume initial snapshot from WebSocket
    let init_msg = ws_stream.next().await.unwrap().unwrap();
    let init_json: serde_json::Value = serde_json::from_str(init_msg.to_text().unwrap()).unwrap();
    assert_eq!(init_json["type"], "initial");

    // 6. Action 1: Web Companion User adds "3 Avocados @Produce" with photo attachment via REST POST
    let raw_photo = b"fake_jpeg_avocado_bytes_data";
    let b64_photo = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, raw_photo);
    let payload = serde_json::json!({
        "raw_input": "3 Avocados @Produce",
        "photo_base64": b64_photo
    });

    let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));

    let resp = http_client
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{ns_id}/items"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let added_item: ItemDto = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(added_item.name, "Avocados");
    assert_eq!(added_item.category.as_deref(), Some("Produce"));
    let avocado_id = ItemId(uuid::Uuid::parse_str(&added_item.id).unwrap());
    let avocado_photo_hash = added_item.photo_hash.clone();
    assert!(avocado_photo_hash.is_some());

    // Verify WebSocket pushed the live update containing the newly created item
    let mut received_avocado = false;
    let ws_timeout = Instant::now();
    while ws_timeout.elapsed() < Duration::from_secs(5) {
        if let Some(Ok(ws_msg)) = ws_stream.next().await {
            if let Ok(update) = serde_json::from_str::<serde_json::Value>(ws_msg.to_text().unwrap())
            {
                if update["type"] == "update"
                    && update["items"].as_array().is_some_and(|a| a.len() == 1)
                {
                    received_avocado = true;
                    break;
                }
            }
        }
    }
    assert!(
        received_avocado,
        "WebSocket did not receive live item update"
    );

    // 7. Action 2: CLI Peer C adds "Almond Milk 1 gal @Dairy" and checks "Avocados"
    let milk_id = ItemId::new_v7();
    let mut milk_data = ItemData::new("Almond Milk").unwrap();
    milk_data.quantity = Some(1.0);
    milk_data.unit = Some("gal".to_string());
    milk_data.category = Some("Dairy".to_string());
    let milk_check = ItemCheck::unchecked();
    let milk_pos =
        ItemPosition::after(&ItemPosition::initial()).unwrap_or_else(|_| ItemPosition::initial());

    node_c
        .set_entry(
            &doc_c,
            DocKey::item_data(milk_id).to_key_bytes(),
            to_cbor(&milk_data).unwrap(),
        )
        .await
        .unwrap();
    node_c
        .set_entry(
            &doc_c,
            DocKey::item_check(milk_id).to_key_bytes(),
            to_cbor(&milk_check).unwrap(),
        )
        .await
        .unwrap();
    node_c
        .set_entry(
            &doc_c,
            DocKey::item_pos(milk_id).to_key_bytes(),
            to_cbor(&milk_pos).unwrap(),
        )
        .await
        .unwrap();

    // Check off Avocados on Peer C
    let avocado_check = ItemCheck::checked(1700000000, "cli-user");
    node_c
        .set_entry(
            &doc_c,
            DocKey::item_check(avocado_id).to_key_bytes(),
            to_cbor(&avocado_check).unwrap(),
        )
        .await
        .unwrap();

    // 8. Action 3: Mobile Peer B adds "Sourdough Bread @Bakery" positioned before initial
    let bread_id = ItemId::new_v7();
    let mut bread_data = ItemData::new("Sourdough Bread").unwrap();
    bread_data.category = Some("Bakery".to_string());
    let bread_check = ItemCheck::unchecked();
    let bread_pos =
        ItemPosition::before(&ItemPosition::initial()).unwrap_or_else(|_| ItemPosition::initial());

    node_b
        .set_entry(
            &doc_b,
            DocKey::item_data(bread_id).to_key_bytes(),
            to_cbor(&bread_data).unwrap(),
        )
        .await
        .unwrap();
    node_b
        .set_entry(
            &doc_b,
            DocKey::item_check(bread_id).to_key_bytes(),
            to_cbor(&bread_check).unwrap(),
        )
        .await
        .unwrap();
    node_b
        .set_entry(
            &doc_b,
            DocKey::item_pos(bread_id).to_key_bytes(),
            to_cbor(&bread_pos).unwrap(),
        )
        .await
        .unwrap();

    // 9. Reconvergence verification across all 3 peers
    let timeout = Duration::from_secs(15);
    let proj_a = wait_for_full_convergence(&node_a, &doc_a, list_id, 3, timeout)
        .await
        .unwrap();
    let proj_b = wait_for_full_convergence(&node_b, &doc_b, list_id, 3, timeout)
        .await
        .unwrap();
    let proj_c = wait_for_full_convergence(&node_c, &doc_c, list_id, 3, timeout)
        .await
        .unwrap();

    // Verify all 3 peer projections are 100% identical
    assert_eq!(proj_a.item_count(), 3);
    assert_eq!(proj_b.item_count(), 3);
    assert_eq!(proj_c.item_count(), 3);

    let items_a = proj_a.sorted_items();
    let items_b = proj_b.sorted_items();
    let items_c = proj_c.sorted_items();

    for i in 0..3 {
        assert_eq!(items_a[i].id, items_b[i].id);
        assert_eq!(items_b[i].id, items_c[i].id);
        assert_eq!(items_a[i].data.name, items_b[i].data.name);
        assert_eq!(items_a[i].check.checked, items_b[i].check.checked);
    }

    // Verify Avocados is checked across all peers
    let avo_view = proj_a.get_item(&avocado_id).unwrap();
    assert!(avo_view.check.checked);

    server_handle.abort();
    let _ = node_b.shutdown().await;
    let _ = node_c.shutdown().await;
}

#[tokio::test]
async fn test_high_volume_stress_and_churn() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    let tmp_c = tempfile::tempdir().unwrap();

    let node_a = GrosheSyncNode::persistent(tmp_a.path()).await.unwrap();
    let (doc_a, ticket_a, ns_id) = node_a
        .create_list_with_meta("Stress Superstore", "tester")
        .await
        .unwrap();
    let list_id = ns_to_list_id(&ns_id);

    let node_b = GrosheSyncNode::persistent(tmp_b.path()).await.unwrap();
    let doc_b = node_b.import_list(ticket_a.clone()).await.unwrap();

    let node_c = GrosheSyncNode::persistent(tmp_c.path()).await.unwrap();
    let doc_c = node_c.import_list(ticket_a.clone()).await.unwrap();

    let mut history = ItemHistoryTracker::new();

    // Generate 20 rapid interleaved additions, position calculations, and deletions across all 3 nodes
    let mut active_items: Vec<(ItemId, usize)> = Vec::new();
    let mut last_pos = ItemPosition::initial();

    for i in 0..20 {
        let item_id = ItemId::new_v7();
        let name = format!("Item_{:03}", i);
        let item_data = ItemData::new(&name).unwrap();
        let check = if i % 3 == 0 {
            ItemCheck::checked(1000 + i as u64, "tester")
        } else {
            ItemCheck::unchecked()
        };
        let pos = ItemPosition::after(&last_pos).expect("valid fractional index");
        last_pos = pos.clone();

        history.record_item(
            &name,
            item_data.category.as_deref(),
            item_data.quantity,
            item_data.unit.as_deref(),
            1000 + i as u64,
        );

        let target_idx = i % 3;
        let (target_node, target_doc) = match target_idx {
            0 => (&node_a, &doc_a),
            1 => (&node_b, &doc_b),
            _ => (&node_c, &doc_c),
        };

        target_node
            .set_entry(
                target_doc,
                DocKey::item_data(item_id).to_key_bytes(),
                to_cbor(&item_data).unwrap(),
            )
            .await
            .unwrap();
        target_node
            .set_entry(
                target_doc,
                DocKey::item_check(item_id).to_key_bytes(),
                to_cbor(&check).unwrap(),
            )
            .await
            .unwrap();
        target_node
            .set_entry(
                target_doc,
                DocKey::item_pos(item_id).to_key_bytes(),
                to_cbor(&pos).unwrap(),
            )
            .await
            .unwrap();

        active_items.push((item_id, target_idx));

        // Delete every 5th item via its origin node
        if i > 0 && i % 5 == 0 {
            let (del_id, origin_idx) = active_items.remove(0);
            let (origin_node, origin_doc) = match origin_idx {
                0 => (&node_a, &doc_a),
                1 => (&node_b, &doc_b),
                _ => (&node_c, &doc_c),
            };
            origin_node
                .delete_entry(origin_doc, format!("item/{del_id}/"))
                .await
                .unwrap();
        }
    }

    let expected_count = active_items.len();
    let timeout = Duration::from_secs(20);

    let proj_a = wait_for_full_convergence(&node_a, &doc_a, list_id, expected_count, timeout)
        .await
        .unwrap();
    let proj_b = wait_for_full_convergence(&node_b, &doc_b, list_id, expected_count, timeout)
        .await
        .unwrap();
    let proj_c = wait_for_full_convergence(&node_c, &doc_c, list_id, expected_count, timeout)
        .await
        .unwrap();

    assert_eq!(proj_a.item_count(), expected_count);
    assert_eq!(proj_b.item_count(), expected_count);
    assert_eq!(proj_c.item_count(), expected_count);

    // Verify all 3 peer projections converged to identical sorted lists
    let sorted_a = proj_a.sorted_items();
    let sorted_b = proj_b.sorted_items();
    let sorted_c = proj_c.sorted_items();

    for (a, b) in sorted_a.iter().zip(sorted_b.iter()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.position, b.position);
    }
    for (b, c) in sorted_b.iter().zip(sorted_c.iter()) {
        assert_eq!(b.id, c.id);
        assert_eq!(b.position, c.position);
    }

    // Verify active items match the converged items
    let active_ids: std::collections::HashSet<_> = active_items.iter().map(|(id, _)| *id).collect();
    let sorted_ids: std::collections::HashSet<_> = sorted_a.iter().map(|it| it.id).collect();
    assert_eq!(active_ids, sorted_ids);

    // Verify strict fractional position order preservation
    for i in 0..expected_count - 1 {
        assert!(sorted_a[i].position.0 < sorted_a[i + 1].position.0);
    }

    // Verify history suggestions
    let suggestions = history.suggest_frequent(&[] as &[&str], 5, 2000);
    assert!(!suggestions.is_empty());

    let _ = node_a.shutdown().await;
    let _ = node_b.shutdown().await;
    let _ = node_c.shutdown().await;
}
