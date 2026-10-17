use std::time::{Duration, Instant};

use groshe_core::{to_cbor, DocKey, ItemCheck, ItemData, ItemId, ItemPosition, ListId};
use groshe_store::ListProjection;
use groshe_sync::{Doc, GrosheSyncNode, NamespaceId};

fn ns_to_list_id(ns_id: &NamespaceId) -> ListId {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&ns_id.as_bytes()[..16]);
    ListId(uuid::Uuid::from_bytes(bytes))
}

async fn wait_for_item_count(
    node: &GrosheSyncNode,
    doc: &Doc,
    list_id: ListId,
    expected_count: usize,
    timeout: Duration,
) -> Result<ListProjection, String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(entries) = node.get_entries_prefix(doc, "").await {
            let mut proj = ListProjection::new(list_id);
            for (k, v) in entries {
                let _ = proj.apply_entry(&k, &v, 0);
            }
            if proj.item_count() == expected_count {
                return Ok(proj);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let entries = node.get_entries_prefix(doc, "").await.unwrap_or_default();
    let mut proj = ListProjection::new(list_id);
    for (k, v) in entries {
        let _ = proj.apply_entry(&k, &v, 0);
    }
    Err(format!(
        "Timed out waiting for item count {}, current count is {}",
        expected_count,
        proj.item_count()
    ))
}

async fn wait_for_item_order(
    node: &GrosheSyncNode,
    doc: &Doc,
    list_id: ListId,
    expected_order: &[ItemId],
    timeout: Duration,
) -> Result<ListProjection, String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(entries) = node.get_entries_prefix(doc, "").await {
            let mut proj = ListProjection::new(list_id);
            for (k, v) in entries {
                let _ = proj.apply_entry(&k, &v, 0);
            }
            if proj.item_count() == expected_order.len() {
                let ids: Vec<ItemId> = proj.sorted_items().iter().map(|i| i.id).collect();
                if ids == expected_order {
                    return Ok(proj);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let entries = node.get_entries_prefix(doc, "").await.unwrap_or_default();
    let mut proj = ListProjection::new(list_id);
    for (k, v) in entries {
        let _ = proj.apply_entry(&k, &v, 0);
    }
    let ids: Vec<ItemId> = proj.sorted_items().iter().map(|i| i.id).collect();
    Err(format!(
        "Timed out waiting for item order {:?}, current order is {:?}",
        expected_order, ids
    ))
}

async fn wait_for_checked_count(
    node: &GrosheSyncNode,
    doc: &Doc,
    list_id: ListId,
    expected_checked: usize,
    timeout: Duration,
) -> Result<ListProjection, String> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(entries) = node.get_entries_prefix(doc, "").await {
            let mut proj = ListProjection::new(list_id);
            for (k, v) in entries {
                let _ = proj.apply_entry(&k, &v, 0);
            }
            if proj.checked_count() == expected_checked {
                return Ok(proj);
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let entries = node.get_entries_prefix(doc, "").await.unwrap_or_default();
    let mut proj = ListProjection::new(list_id);
    for (k, v) in entries {
        let _ = proj.apply_entry(&k, &v, 0);
    }
    Err(format!(
        "Timed out waiting for checked count {}, current count is {}",
        expected_checked,
        proj.checked_count()
    ))
}

#[tokio::test]
async fn test_three_peer_cli_convergence_and_reconciliation() {
    let timeout = Duration::from_secs(10);

    let dir1 = tempfile::tempdir().unwrap();
    let dir2 = tempfile::tempdir().unwrap();
    let dir3 = tempfile::tempdir().unwrap();

    // 1. Peer 2 (Always-on Headless Daemon / Sync Router) boots first
    let node_daemon = GrosheSyncNode::persistent(dir2.path())
        .await
        .expect("Failed to initialize daemon node");

    // 2. Peer 1 (Alice CLI) creates list
    let node_alice = GrosheSyncNode::persistent(dir1.path())
        .await
        .expect("Failed to initialize Alice node");

    let (doc_alice, ticket_alice, ns_id) = node_alice
        .create_list_with_meta("Weekend BBQ", "Alice")
        .await
        .expect("Failed to create list with meta");
    let list_id = ns_to_list_id(&ns_id);

    // 3. Daemon and Charlie join the list via Alice's ticket
    let doc_daemon = node_daemon
        .import_list(ticket_alice.clone())
        .await
        .expect("Daemon failed to import list");

    let node_charlie = GrosheSyncNode::persistent(dir3.path())
        .await
        .expect("Failed to initialize Charlie node");

    let doc_charlie = node_charlie
        .import_list(ticket_alice)
        .await
        .expect("Charlie failed to import list");

    // 4. Alice adds two items: "Burger Buns" and "Ribeye Steaks"
    let id_buns = ItemId::new_v7();
    let id_steaks = ItemId::new_v7();

    let buns_data = ItemData::new("Burger Buns")
        .unwrap()
        .with_quantity(2.0, Some("packs".to_string()))
        .unwrap();
    let steaks_data = ItemData::new("Ribeye Steaks")
        .unwrap()
        .with_quantity(4.0, Some("steaks".to_string()))
        .unwrap();

    let pos_buns = ItemPosition::initial();
    let pos_steaks = ItemPosition::after(&pos_buns).unwrap();

    node_alice
        .set_entry(
            &doc_alice,
            DocKey::item_data(id_buns).to_key_bytes(),
            to_cbor(&buns_data).unwrap(),
        )
        .await
        .unwrap();
    node_alice
        .set_entry(
            &doc_alice,
            DocKey::item_pos(id_buns).to_key_bytes(),
            to_cbor(&pos_buns).unwrap(),
        )
        .await
        .unwrap();

    node_alice
        .set_entry(
            &doc_alice,
            DocKey::item_data(id_steaks).to_key_bytes(),
            to_cbor(&steaks_data).unwrap(),
        )
        .await
        .unwrap();
    node_alice
        .set_entry(
            &doc_alice,
            DocKey::item_pos(id_steaks).to_key_bytes(),
            to_cbor(&pos_steaks).unwrap(),
        )
        .await
        .unwrap();

    // 5. Charlie concurrently adds "Charcoal"
    let id_charcoal = ItemId::new_v7();
    let charcoal_data = ItemData::new("Charcoal")
        .unwrap()
        .with_quantity(1.0, Some("bag".to_string()))
        .unwrap();
    let pos_charcoal = ItemPosition::before(&pos_buns).unwrap();

    node_charlie
        .set_entry(
            &doc_charlie,
            DocKey::item_data(id_charcoal).to_key_bytes(),
            to_cbor(&charcoal_data).unwrap(),
        )
        .await
        .unwrap();
    node_charlie
        .set_entry(
            &doc_charlie,
            DocKey::item_pos(id_charcoal).to_key_bytes(),
            to_cbor(&pos_charcoal).unwrap(),
        )
        .await
        .unwrap();

    // 6. Verify all 3 peers converge to 3 items in the exact expected position order
    let expected_order = vec![id_charcoal, id_buns, id_steaks];
    let proj_alice =
        wait_for_item_order(&node_alice, &doc_alice, list_id, &expected_order, timeout)
            .await
            .unwrap();
    let proj_daemon =
        wait_for_item_order(&node_daemon, &doc_daemon, list_id, &expected_order, timeout)
            .await
            .unwrap();
    let proj_charlie = wait_for_item_order(
        &node_charlie,
        &doc_charlie,
        list_id,
        &expected_order,
        timeout,
    )
    .await
    .unwrap();

    assert_eq!(proj_alice.item_count(), 3);
    assert_eq!(proj_daemon.item_count(), 3);
    assert_eq!(proj_charlie.item_count(), 3);

    // Assert identical position sorting across all 3 peers
    let ids_alice: Vec<ItemId> = proj_alice.sorted_items().iter().map(|i| i.id).collect();
    let ids_daemon: Vec<ItemId> = proj_daemon.sorted_items().iter().map(|i| i.id).collect();
    let ids_charlie: Vec<ItemId> = proj_charlie.sorted_items().iter().map(|i| i.id).collect();

    assert_eq!(ids_alice, expected_order);
    assert_eq!(ids_daemon, ids_alice);
    assert_eq!(ids_charlie, ids_alice);

    // 7. Concurrent mutation test:
    // Charlie checks off "Burger Buns"
    let check_buns = ItemCheck::checked(1000, "charlie");
    node_charlie
        .set_entry(
            &doc_charlie,
            DocKey::item_check(id_buns).to_key_bytes(),
            to_cbor(&check_buns).unwrap(),
        )
        .await
        .unwrap();

    // Alice deletes "Ribeye Steaks"
    node_alice
        .delete_entry(&doc_alice, format!("item/{id_steaks}/"))
        .await
        .unwrap();

    // Verify all 3 converge to 2 active items with 1 checked item
    let _final_alice = wait_for_item_count(&node_alice, &doc_alice, list_id, 2, timeout)
        .await
        .unwrap();
    let _final_daemon = wait_for_item_count(&node_daemon, &doc_daemon, list_id, 2, timeout)
        .await
        .unwrap();
    let _final_charlie = wait_for_item_count(&node_charlie, &doc_charlie, list_id, 2, timeout)
        .await
        .unwrap();

    let check_alice = wait_for_checked_count(&node_alice, &doc_alice, list_id, 1, timeout)
        .await
        .unwrap();
    assert_eq!(check_alice.checked_count(), 1);

    let check_daemon = wait_for_checked_count(&node_daemon, &doc_daemon, list_id, 1, timeout)
        .await
        .unwrap();
    assert_eq!(check_daemon.checked_count(), 1);

    let check_charlie = wait_for_checked_count(&node_charlie, &doc_charlie, list_id, 1, timeout)
        .await
        .unwrap();
    assert_eq!(check_charlie.checked_count(), 1);

    assert_eq!(check_alice.sorted_items()[0].id, id_charcoal);
    assert_eq!(check_alice.sorted_items()[1].id, id_buns);
    assert!(check_alice.sorted_items()[1].is_checked());

    assert_eq!(check_daemon.sorted_items(), check_alice.sorted_items());
    assert_eq!(check_charlie.sorted_items(), check_alice.sorted_items());

    // 8. List Rotation Test (Milestone 1.5.3)
    // Alice performs list rotation: migrates active items to brand-new doc, drops old doc
    let (rotated_doc_alice, rotated_ticket_alice, rotated_ns_id) = node_alice
        .rotate_list(ns_id, "Alice-Rotated")
        .await
        .expect("Alice failed to rotate list");
    let rotated_list_id = ns_to_list_id(&rotated_ns_id);

    // Daemon joins the new rotated list via the new ticket
    let rotated_doc_daemon = node_daemon
        .import_list(rotated_ticket_alice.clone())
        .await
        .expect("Daemon failed to join rotated list");

    // Charlie joins the new rotated list via the new ticket
    let rotated_doc_charlie = node_charlie
        .import_list(rotated_ticket_alice)
        .await
        .expect("Charlie failed to join rotated list");

    // Verify all active items migrated to the new list across all peers
    let rot_alice =
        wait_for_item_count(&node_alice, &rotated_doc_alice, rotated_list_id, 2, timeout)
            .await
            .unwrap();
    let rot_daemon = wait_for_item_count(
        &node_daemon,
        &rotated_doc_daemon,
        rotated_list_id,
        2,
        timeout,
    )
    .await
    .unwrap();
    let rot_charlie = wait_for_item_count(
        &node_charlie,
        &rotated_doc_charlie,
        rotated_list_id,
        2,
        timeout,
    )
    .await
    .unwrap();

    assert_eq!(rot_alice.item_count(), 2);
    assert_eq!(rot_daemon.item_count(), 2);
    assert_eq!(rot_charlie.item_count(), 2);

    // Verify deleted Ribeye Steaks was NOT migrated
    assert!(rot_alice.get_item(&id_steaks).is_none());
    assert!(rot_daemon.get_item(&id_steaks).is_none());
    assert!(rot_charlie.get_item(&id_steaks).is_none());

    // Clean shutdown
    node_alice.shutdown().await.unwrap();
    node_daemon.shutdown().await.unwrap();
    node_charlie.shutdown().await.unwrap();
}
