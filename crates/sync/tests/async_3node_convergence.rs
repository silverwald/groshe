use std::time::Duration;

use groshe_sync::{Doc, GrosheSyncNode};

/// Polls for a key to equal an expected value within a given timeout.
async fn wait_for_entry(
    node: &GrosheSyncNode,
    doc: &Doc,
    key: &str,
    expected_value: &[u8],
    timeout_duration: Duration,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout_duration {
        if let Ok(Some(val)) = node.get_entry(doc, key).await {
            if val == expected_value {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let actual = node.get_entry(doc, key).await;
    Err(format!(
        "Timed out waiting for key '{}' to equal '{:?}', actual: {:?}",
        key,
        String::from_utf8_lossy(expected_value),
        actual
    ))
}

#[tokio::test]
async fn test_async_3node_peer_convergence() {
    // -------------------------------------------------------------------------
    // Setup isolated persistent storage directories for all three peers
    // -------------------------------------------------------------------------
    let dir_node1 = tempfile::tempdir().unwrap(); // Phone A
    let dir_node2 = tempfile::tempdir().unwrap(); // Headless "grocery serve" daemon
    let dir_node3 = tempfile::tempdir().unwrap(); // Phone B

    println!("Node 1 (Phone A) dir: {}", dir_node1.path().display());
    println!("Node 2 (CLI serve) dir: {}", dir_node2.path().display());
    println!("Node 3 (Phone B) dir: {}", dir_node3.path().display());

    // -------------------------------------------------------------------------
    // Step 1: Boot Node 2 (Always-on CLI "grocery serve" daemon)
    // -------------------------------------------------------------------------
    let node2_serve = GrosheSyncNode::persistent(dir_node2.path())
        .await
        .expect("Failed to initialize Node 2 (serve daemon)");

    // -------------------------------------------------------------------------
    // Step 2: Phone A boots up, creates doc, adds Item 1, syncs to Node 2
    // -------------------------------------------------------------------------
    let node1_phone_a = GrosheSyncNode::persistent(dir_node1.path())
        .await
        .expect("Failed to initialize Node 1 (Phone A)");

    let (doc1, ticket_from_a) = node1_phone_a
        .create_list()
        .await
        .expect("Phone A failed to create list");
    let doc_id = ticket_from_a.capability.id();

    // Node 1 writes Item 1 ("Oat Milk")
    node1_phone_a
        .set_entry(&doc1, "item/1/data", "Oat Milk")
        .await
        .expect("Phone A failed to set item 1 data");

    // Node 2 (grocery serve) joins doc using Phone A's ticket
    let doc2 = node2_serve
        .import_list(ticket_from_a)
        .await
        .expect("Node 2 failed to import doc ticket");

    // Wait for Node 2 to receive Item 1 from Node 1
    wait_for_entry(
        &node2_serve,
        &doc2,
        "item/1/data",
        b"Oat Milk",
        Duration::from_secs(10),
    )
    .await
    .expect("Node 2 failed to converge on Item 1");

    // Node 2 (always-on peer) generates a DocTicket containing Node 2's address
    let ticket_from_serve = node2_serve
        .ticket(&doc2)
        .await
        .expect("Node 2 failed to generate DocTicket");

    // -------------------------------------------------------------------------
    // Step 3: Phone A shuts down completely (goes offline)
    // -------------------------------------------------------------------------
    node1_phone_a
        .shutdown()
        .await
        .expect("Phone A failed to shut down");
    println!(">> Phone A is now OFFLINE.");

    // -------------------------------------------------------------------------
    // Step 4: Phone B boots up with ticket from serve daemon, syncs from Node 2
    // (Phone A is strictly OFFLINE!)
    // -------------------------------------------------------------------------
    let node3_phone_b = GrosheSyncNode::persistent(dir_node3.path())
        .await
        .expect("Failed to initialize Node 3 (Phone B)");

    let doc3 = node3_phone_b
        .import_list(ticket_from_serve.clone())
        .await
        .expect("Phone B failed to import ticket from serve peer");

    // Phone B must sync Item 1 from Node 2 (serve daemon) without Phone A being online
    wait_for_entry(
        &node3_phone_b,
        &doc3,
        "item/1/data",
        b"Oat Milk",
        Duration::from_secs(10),
    )
    .await
    .expect("Phone B failed to sync Item 1 from Node 2");

    // Phone B checks off Item 1 and adds Item 2 ("Apples")
    node3_phone_b
        .set_entry(&doc3, "item/1/check", "true")
        .await
        .expect("Phone B failed to check Item 1");

    node3_phone_b
        .set_entry(&doc3, "item/2/data", "Apples")
        .await
        .expect("Phone B failed to add Item 2");

    // Wait for Node 2 (serve daemon) to receive Phone B's updates
    wait_for_entry(
        &node2_serve,
        &doc2,
        "item/1/check",
        b"true",
        Duration::from_secs(10),
    )
    .await
    .expect("Node 2 failed to sync Item 1 check state from Phone B");

    wait_for_entry(
        &node2_serve,
        &doc2,
        "item/2/data",
        b"Apples",
        Duration::from_secs(10),
    )
    .await
    .expect("Node 2 failed to sync Item 2 from Phone B");

    // -------------------------------------------------------------------------
    // Step 5: Phone B shuts down completely (goes offline)
    // -------------------------------------------------------------------------
    node3_phone_b
        .shutdown()
        .await
        .expect("Phone B failed to shut down");
    println!(">> Phone B is now OFFLINE.");

    // -------------------------------------------------------------------------
    // Step 6: Phone A boots back up, syncs from Node 2 (Phone B is offline)
    // -------------------------------------------------------------------------
    let node1_reboot = GrosheSyncNode::persistent(dir_node1.path())
        .await
        .expect("Phone A failed to reboot");

    let doc1_reboot = node1_reboot
        .open_list(doc_id)
        .await
        .expect("Phone A failed to open list")
        .expect("Doc not found in Phone A local store");

    // Re-connect Phone A to the swarm via ticket nodes (Node 2 serve daemon)
    node1_reboot
        .join_ticket(&doc1_reboot, &ticket_from_serve)
        .await
        .expect("Phone A failed to re-join sync swarm with Node 2");

    // Wait for Phone A to sync the check state and new item from Node 2
    wait_for_entry(
        &node1_reboot,
        &doc1_reboot,
        "item/1/check",
        b"true",
        Duration::from_secs(10),
    )
    .await
    .expect("Phone A failed to sync Item 1 checked state from Node 2");

    wait_for_entry(
        &node1_reboot,
        &doc1_reboot,
        "item/2/data",
        b"Apples",
        Duration::from_secs(10),
    )
    .await
    .expect("Phone A failed to sync Item 2 from Node 2");

    // -------------------------------------------------------------------------
    // Step 7: Final Convergence Verification
    // -------------------------------------------------------------------------
    let items_phone_a = node1_reboot
        .get_entries_prefix(&doc1_reboot, "item/")
        .await
        .unwrap();

    let items_map: std::collections::BTreeMap<String, String> = items_phone_a
        .into_iter()
        .map(|(k, v)| {
            (
                String::from_utf8_lossy(&k).to_string(),
                String::from_utf8_lossy(&v).to_string(),
            )
        })
        .collect();

    assert_eq!(
        items_map.get("item/1/data").map(|s| s.as_str()),
        Some("Oat Milk")
    );
    assert_eq!(
        items_map.get("item/1/check").map(|s| s.as_str()),
        Some("true")
    );
    assert_eq!(
        items_map.get("item/2/data").map(|s| s.as_str()),
        Some("Apples")
    );

    println!(">> 3-Node Async Convergence test passed with 100% data consistency!");

    // Clean up
    node1_reboot.shutdown().await.unwrap();
    node2_serve.shutdown().await.unwrap();
}
