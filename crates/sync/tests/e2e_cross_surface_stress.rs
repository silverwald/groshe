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
use groshe_web::{create_router, AppState, ItemDto, ListDetailDto};

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
    let timeout = Duration::from_secs(35);

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


#[tokio::test]
async fn test_cross_surface_item_edit_convergence() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();

    let node_a = Arc::new(GrosheSyncNode::persistent(tmp_a.path()).await.unwrap());
    let node_b = GrosheSyncNode::persistent(tmp_b.path()).await.unwrap();

    let (_doc_a, ticket_a, ns_id) = node_a
        .create_list_with_meta("Edit Sync Test", "admin")
        .await
        .unwrap();
    let list_id = ns_to_list_id(&ns_id);

    let doc_b = node_b.import_list(ticket_a).await.unwrap();

    let app_state = AppState {
        node: Arc::clone(&node_a),
    };

    let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
    let add_payload = serde_json::json!({
        "raw_input": "2 Honeycrisp Apples @Produce"
    });
    let resp = http_client
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{ns_id}/items"))
                .header("content-type", "application/json")
                .body(Body::from(add_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let created_item: ItemDto = serde_json::from_slice(&body_bytes).unwrap();
    let item_id = ItemId(uuid::Uuid::parse_str(&created_item.id).unwrap());
    assert_eq!(created_item.name, "Honeycrisp Apples");
    assert_eq!(created_item.quantity, Some(2.0));

    let timeout = Duration::from_secs(25);
    let proj_b = wait_for_full_convergence(&node_b, &doc_b, list_id, 1, timeout)
        .await
        .unwrap();
    assert_eq!(proj_b.get_item(&item_id).unwrap().name(), "Honeycrisp Apples");

    let edit_payload = serde_json::json!({
        "raw_input": "5 Organic Honeycrisp Apples @Produce",
        "note": "crispy ones only"
    });
    let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
    let resp = http_client
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/lists/{ns_id}/items/{item_id}"))
                .header("content-type", "application/json")
                .body(Body::from(edit_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let updated_item: ItemDto = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(updated_item.name, "Organic Honeycrisp Apples");
    assert_eq!(updated_item.quantity, Some(5.0));
    assert_eq!(updated_item.note.as_deref(), Some("crispy ones only"));

    let start = Instant::now();
    let mut converged = false;
    while start.elapsed() < Duration::from_secs(25) {
        if let Ok(entries) = node_b.get_entries_prefix(&doc_b, "").await {
            let mut proj = ListProjection::new(list_id);
            for (k, v) in &entries {
                let _ = proj.apply_entry(k, v, 0);
            }
            if let Some(it) = proj.get_item(&item_id) {
                if it.name() == "Organic Honeycrisp Apples"
                    && it.data.quantity == Some(5.0)
                    && it.data.note.as_deref() == Some("crispy ones only")
                {
                    converged = true;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(converged, "Peer B timed out waiting for edited item convergence");

    let key = DocKey::item_data(item_id).to_key_bytes();
    let mut mobile_data = ItemData::new("Jumbo Honeycrisp Apples").unwrap();
    mobile_data.quantity = Some(6.0);
    mobile_data.unit = Some("lbs".to_string());
    mobile_data.category = Some("Produce".to_string());
    mobile_data.note = Some("crispy ones only".to_string());
    node_b
        .set_entry(&doc_b, key, to_cbor(&mobile_data).unwrap())
        .await
        .unwrap();

    let start = Instant::now();
    let mut web_converged = false;
    while start.elapsed() < Duration::from_secs(25) {
        let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
        let resp = http_client
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/lists/{ns_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == StatusCode::OK {
            let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
            if let Ok(detail) = serde_json::from_slice::<ListDetailDto>(&body_bytes) {
                if let Some(it) = detail.items.iter().find(|i| i.id == item_id.to_string()) {
                    if it.name == "Jumbo Honeycrisp Apples"
                        && it.quantity == Some(6.0)
                        && it.unit.as_deref() == Some("lbs")
                    {
                        web_converged = true;
                        break;
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(web_converged, "Peer A timed out waiting for Mobile edit convergence");

    node_a.endpoint().close().await;
    let _ = node_b.shutdown().await;
}
#[tokio::test]
async fn test_cross_surface_item_reorder_convergence() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();

    let node_a = Arc::new(GrosheSyncNode::persistent(tmp_a.path()).await.unwrap());
    let node_b = GrosheSyncNode::persistent(tmp_b.path()).await.unwrap();

    let (_doc_a, ticket_a, ns_id) = node_a
        .create_list_with_meta("Reorder Sync Test", "admin")
        .await
        .unwrap();
    let list_id = ns_to_list_id(&ns_id);

    let doc_b = node_b.import_list(ticket_a.clone()).await.unwrap();



    let app_state = AppState {
        node: Arc::clone(&node_a),
    };

    // 1. Peer A adds 3 items: Apples, Bananas, Carrots via Web Companion REST API
    let mut item_ids = Vec::new();
    for name in ["Apples", "Bananas", "Carrots"] {
        let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
        let payload = serde_json::json!({ "raw_input": name });
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
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let created: ItemDto = serde_json::from_slice(&bytes).unwrap();
        item_ids.push(ItemId(uuid::Uuid::parse_str(&created.id).unwrap()));
    }

    // 2. Wait for full sync to Peer B (all 3 items including data, check, and pos entries)
    let timeout = Duration::from_secs(25);
    let start = Instant::now();
    let mut proj_b = None;
    while start.elapsed() < timeout {
        if let Ok(entries) = node_b.get_entries_prefix(&doc_b, "").await {
            let mut proj = ListProjection::new(list_id);
            for (k, v) in &entries {
                let _ = proj.apply_entry(k, v, 0);
            }
            // 1 meta + 3 items * 3 entries (data, check, pos) = 10 entries
            if entries.len() >= 10 && proj.item_count() == 3 {
                proj_b = Some(proj);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let proj_b = proj_b.expect("Peer B failed to converge on 3 items with all entries");
    let sorted_b = proj_b.sorted_items();
    assert_eq!(sorted_b[0].id, item_ids[0]);
    assert_eq!(sorted_b[1].id, item_ids[1]);
    assert_eq!(sorted_b[2].id, item_ids[2]);

    // 3. Peer A reorders Carrots to top via Web Companion REST POST /pos/:id
    let carrots_id = item_ids[2];
    let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
    let reorder_payload = serde_json::json!({ "direction": "top" });
    let resp = http_client
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{ns_id}/pos/{carrots_id}"))
                .header("content-type", "application/json")
                .body(Body::from(reorder_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Verify Peer B converges on [Carrots, Apples, Bananas]
    let start = Instant::now();
    let mut b_converged = false;
    while start.elapsed() < Duration::from_secs(25) {
        if let Ok(entries) = node_b.get_entries_prefix(&doc_b, "").await {
            let mut proj = ListProjection::new(list_id);
            for (k, v) in &entries {
                let _ = proj.apply_entry(k, v, 0);
            }
            let sorted = proj.sorted_items();
            if sorted.len() == 3
                && sorted[0].id == carrots_id
                && sorted[1].id == item_ids[0]
                && sorted[2].id == item_ids[1]
            {
                b_converged = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(b_converged, "Peer B timed out waiting for Carrots reorder to top");

    // 5. Concurrently, Peer B moves Bananas between Carrots and Apples
    // while Peer A checks off Apples
    let apples_id = item_ids[0];
    let bananas_id = item_ids[1];

    // Peer B computes position between Carrots and Apples
    let carrots_pos = {
        let entries = node_b.get_entries_prefix(&doc_b, "").await.unwrap();
        let mut proj = ListProjection::new(list_id);
        for (k, v) in &entries {
            let _ = proj.apply_entry(k, v, 0);
        }
        proj.get_item(&carrots_id).unwrap().position.clone()
    };
    let apples_pos = {
        let entries = node_b.get_entries_prefix(&doc_b, "").await.unwrap();
        let mut proj = ListProjection::new(list_id);
        for (k, v) in &entries {
            let _ = proj.apply_entry(k, v, 0);
        }
        proj.get_item(&apples_id).unwrap().position.clone()
    };
    let bananas_new_pos = ItemPosition::between(Some(&carrots_pos), Some(&apples_pos)).unwrap();

    node_b
        .set_entry(
            &doc_b,
            DocKey::item_pos(bananas_id).to_key_bytes(),
            to_cbor(&bananas_new_pos).unwrap(),
        )
        .await
        .unwrap();

    // Peer A checks off Apples via REST POST
    let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
    let check_payload = serde_json::json!({ "checked": true });
    let resp = http_client
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/lists/{ns_id}/check/{apples_id}"))
                .header("content-type", "application/json")
                .body(Body::from(check_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 6. Verify convergence on both peers:
    // Unchecked items: Carrots, Bananas
    // Checked items: Apples
    let start = Instant::now();
    let mut web_converged = false;
    while start.elapsed() < Duration::from_secs(25) {
        let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
        let resp = http_client
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/lists/{ns_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == StatusCode::OK {
            let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
            if let Ok(detail) = serde_json::from_slice::<ListDetailDto>(&body_bytes) {
                if detail.items.len() == 3
                    && detail.items[0].id == carrots_id.to_string()
                    && !detail.items[0].checked
                    && detail.items[1].id == bananas_id.to_string()
                    && !detail.items[1].checked
                    && detail.items[2].id == apples_id.to_string()
                    && detail.items[2].checked
                {
                    web_converged = true;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(web_converged, "Peer A (Web Companion) timed out waiting for concurrent reorder & check convergence");

    node_a.endpoint().close().await;
    let _ = node_b.shutdown().await;
}

#[tokio::test]
async fn test_cross_surface_household_and_live_presence_convergence() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();

    let node_a = Arc::new(GrosheSyncNode::persistent(tmp_a.path()).await.unwrap());
    let node_b = GrosheSyncNode::persistent(tmp_b.path()).await.unwrap();

    let app_state = AppState {
        node: Arc::clone(&node_a),
    };

    // 1. Peer A creates a Household via Web Companion POST /api/households
    let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
    let create_hh_payload = serde_json::json!({ "name": "Groshe Family" });
    let resp = http_client
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/households")
                .header("content-type", "application/json")
                .body(Body::from(create_hh_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let hh_dto: groshe_web::HouseholdDto = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(hh_dto.name, "Groshe Family");

    // 2. Peer A creates a shared list inside the Household via POST /api/households/:id/lists
    let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
    let add_list_payload = serde_json::json!({ "title": "Saturday Farmers Market" });
    let resp = http_client
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/households/{}/lists", hh_dto.id))
                .header("content-type", "application/json")
                .body(Body::from(add_list_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let hh_dto: groshe_web::HouseholdDto = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(hh_dto.lists.len(), 1);
    let market_list_id_str = hh_dto.lists[0].list_id.clone();

    // 3. Peer B (Mobile/CLI) joins the Household using ONLY the Household ticket and auto-discovers the list
    let (hh_doc_b, _, _) = node_b
        .join_household(&hh_dto.ticket, "Bob (Mobile)")
        .await
        .unwrap();

    let timeout = Duration::from_secs(25);
    let start = Instant::now();
    let mut discovered_ns = None;
    while start.elapsed() < timeout {
        let _ = node_b.reconcile_household_lists(&hh_doc_b).await;
        if let Ok(ids) = node_b.list_ids().await {
            if let Some(found) = ids.into_iter().find(|id| id.to_string() == market_list_id_str) {
                discovered_ns = Some(found);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let market_ns = discovered_ns.expect("Peer B failed to auto-discover household list");

    // 4. Peer B enters In-Store shopping mode on the discovered list and announces presence
    node_b
        .announce_presence(&market_ns, "Bob (Mobile)", true)
        .await
        .unwrap();

    // 5. Verify Peer A's Web Companion GET /api/lists/:id converges on Bob's live in-store presence
    let start = Instant::now();
    let mut presence_converged = false;
    while start.elapsed() < timeout {
        let http_client = tower::ServiceBuilder::new().service(create_router(app_state.clone()));
        let resp = http_client
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/lists/{market_list_id_str}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == StatusCode::OK {
            let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
            if let Ok(detail) = serde_json::from_slice::<ListDetailDto>(&body_bytes) {
                if detail
                    .active_peers
                    .iter()
                    .any(|p| p.author_name == "Bob (Mobile)" && p.in_store)
                {
                    presence_converged = true;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        presence_converged,
        "Peer A Web Companion failed to converge on Peer B live in-store presence"
    );

    node_a.endpoint().close().await;
    let _ = node_b.shutdown().await;
}
