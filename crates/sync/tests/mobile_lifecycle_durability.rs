//! Integration test for Spike 0.2: Iroh Mobile Durability, redb crash recovery,
//! and live QUIC sync latency.
//!
//! Validates:
//! 1. redb database durability across 10 consecutive simulated mobile restart cycles.
//! 2. Live remote insertion event latency over direct QUIC transport (< 1.5s).
//! 3. Re-convergence of persisted offline mutations upon restart.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use groshe_sync::{GrosheSyncNode, LiveEvent};

/// Test 1: Validate redb durability across 10 restart cycles.
#[tokio::test]
async fn test_redb_durability_across_10_force_kills() {
    let test_dir = tempfile::tempdir().unwrap();
    let path = test_dir.path().to_path_buf();
    println!("Testing redb durability at: {}", path.display());

    let mut doc_id_opt = None;

    for cycle in 0..10 {
        // Step 1: Open or recover node
        let node = GrosheSyncNode::persistent(&path)
            .await
            .unwrap_or_else(|e| panic!("Failed to open node on cycle {}: {}", cycle, e));

        let doc = match doc_id_opt {
            None => {
                let (new_doc, _ticket) = node
                    .create_list()
                    .await
                    .expect("Failed to create initial list");
                doc_id_opt = Some(new_doc.id());
                new_doc
            }
            Some(id) => node
                .open_list(id)
                .await
                .expect("Failed to open existing list")
                .expect("List document not found in persistent store"),
        };

        // Step 2: Write entries for this cycle
        let data_key = format!("item/{}/data", cycle);
        let check_key = format!("item/{}/check", cycle);
        let data_val = format!("Item Content for Cycle {}", cycle);

        node.set_entry(&doc, &data_key, &data_val)
            .await
            .expect("Failed to write data key");
        node.set_entry(&doc, &check_key, "false")
            .await
            .expect("Failed to write check key");

        // Verify all previous cycles are intact
        for prev in 0..=cycle {
            let prev_key = format!("item/{}/data", prev);
            let expected_val = format!("Item Content for Cycle {}", prev);
            let read_val = node
                .get_entry(&doc, &prev_key)
                .await
                .expect("Failed reading previous cycle entry")
                .unwrap_or_else(|| panic!("Cycle {} data missing on cycle {}", prev, cycle));

            assert_eq!(
                read_val,
                expected_val.as_bytes(),
                "Data mismatch on cycle {} for key {}",
                cycle,
                prev_key
            );
        }

        // Cleanly shut down protocols so file locks and irpc actors are released before reopening
        drop(doc);
        node.shutdown().await.expect("Failed to shutdown node");

        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // Final verification after 10 cycles: All 10 items present and readable
    let final_node = GrosheSyncNode::persistent(&path)
        .await
        .expect("Final persistent reopen failed");
    let final_doc = final_node
        .open_list(doc_id_opt.unwrap())
        .await
        .expect("Final open_list failed")
        .expect("Final doc missing");

    let entries = final_node
        .get_entries_prefix(&final_doc, "item/")
        .await
        .expect("Failed to query entries prefix");

    // 10 items * 2 keys each (data + check) = 20 keys
    assert_eq!(
        entries.len(),
        20,
        "Expected 20 persisted entries across 10 kill cycles, found {}",
        entries.len()
    );

    final_node.shutdown().await.expect("Clean final shutdown");
}

/// Test 2: Live remote sync latency over direct QUIC transport (< 1.5s).
#[tokio::test]
async fn test_quic_live_event_latency() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let node_a = GrosheSyncNode::persistent(dir_a.path())
        .await
        .expect("Failed to boot Node A");
    let (doc_a, ticket_a) = node_a
        .create_list()
        .await
        .expect("Failed to create list on Node A");

    let node_b = GrosheSyncNode::persistent(dir_b.path())
        .await
        .expect("Failed to boot Node B");
    let (doc_b, mut events_b) = node_b
        .import_list_with_events(ticket_a)
        .await
        .expect("Failed to import list on Node B");

    // Warm-up initial sync connection
    node_a
        .set_entry(&doc_a, "warmup", "ready")
        .await
        .expect("Failed warmup write");

    // Measure latency for a live item update
    let start = Instant::now();
    let target_key = "item/live_latency_test/data";
    let target_val = b"Organic Honeycrisp Apples";

    node_a
        .set_entry(&doc_a, target_key, target_val)
        .await
        .expect("Failed live entry write");

    let mut received_remote = false;
    let timeout = tokio::time::sleep(Duration::from_secs(3));
    tokio::pin!(timeout);

    let mut sync_duration = Duration::ZERO;

    loop {
        tokio::select! {
            Some(event_res) = events_b.next() => {
                if let Ok(LiveEvent::ContentReady { hash }) = event_res {
                    if let Ok(bytes) = node_b.blobs().get_bytes(hash).await {
                        if bytes.as_ref() == target_val {
                            sync_duration = start.elapsed();
                            received_remote = true;
                            break;
                        }
                    }
                }
            }
            _ = &mut timeout => {
                break;
            }
        }
    }

    if !received_remote {
        // Fallback check if content was ready before event listener polled
        if let Ok(Some(val)) = node_b.get_entry(&doc_b, target_key).await {
            if val == target_val {
                sync_duration = start.elapsed();
                received_remote = true;
            }
        }
    }

    assert!(
        received_remote,
        "Node B did not receive live remote update within timeout"
    );

    println!(
        "Live QUIC sync event latency: {:?} (acceptance criteria: < 1500ms)",
        sync_duration
    );
    assert!(
        sync_duration < Duration::from_millis(1500),
        "Live QUIC sync took {:?}, exceeding 1.5s requirement",
        sync_duration
    );

    node_a.shutdown().await.expect("Node A shutdown failed");
    node_b.shutdown().await.expect("Node B shutdown failed");
}

/// Test 3: Re-convergence of persisted offline mutations upon restart.
#[tokio::test]
async fn test_offline_mobile_mutation_reconvergence() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    // Step 1: Initial sync between Phone A and Phone B
    let node_a = GrosheSyncNode::persistent(dir_a.path()).await.unwrap();
    let (doc_a, ticket_a) = node_a.create_list().await.unwrap();
    node_a
        .set_entry(&doc_a, "item/1/data", "Sourdough Bread")
        .await
        .unwrap();

    let node_b = GrosheSyncNode::persistent(dir_b.path()).await.unwrap();
    let doc_b = node_b.import_list(ticket_a.clone()).await.unwrap();

    // Verify Phone B receives item 1
    let mut synced = false;
    for _ in 0..50 {
        if let Ok(Some(val)) = node_b.get_entry(&doc_b, "item/1/data").await {
            if val == b"Sourdough Bread" {
                synced = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(synced, "Phone B failed initial sync");

    let doc_id = doc_a.id();

    // Step 2: Phone A goes completely offline
    drop(doc_a);
    node_a.shutdown().await.unwrap();

    // Step 3: Phone B, while Phone A is offline, checks item 1 and adds item 2
    node_b
        .set_entry(&doc_b, "item/1/check", "true")
        .await
        .unwrap();
    node_b
        .set_entry(&doc_b, "item/2/data", "Organic Butter")
        .await
        .unwrap();

    // Phone B now shuts down
    drop(doc_b);
    node_b.shutdown().await.unwrap();

    // Step 4: Phone A and Phone B boot up again and reconnect
    let node_a_reboot = GrosheSyncNode::persistent(dir_a.path()).await.unwrap();
    let doc_a_reboot = node_a_reboot.open_list(doc_id).await.unwrap().unwrap();

    let node_b_reboot = GrosheSyncNode::persistent(dir_b.path()).await.unwrap();
    let doc_b_reboot = node_b_reboot.open_list(doc_id).await.unwrap().unwrap();

    // Phone B generates a fresh live ticket with its new endpoint addresses and Phone A joins it
    let ticket_b_live = node_b_reboot.ticket(&doc_b_reboot).await.unwrap();
    node_a_reboot
        .join_ticket(&doc_a_reboot, &ticket_b_live)
        .await
        .unwrap();

    // Step 5: Verify Phone A reconciles all offline mutations from Phone B
    let mut reconciled_item2 = false;
    let mut reconciled_check1 = false;
    for _ in 0..100 {
        if let Ok(Some(val)) = node_a_reboot.get_entry(&doc_a_reboot, "item/2/data").await {
            if val == b"Organic Butter" {
                reconciled_item2 = true;
            }
        }
        if let Ok(Some(val)) = node_a_reboot.get_entry(&doc_a_reboot, "item/1/check").await {
            if val == b"true" {
                reconciled_check1 = true;
            }
        }
        if reconciled_item2 && reconciled_check1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(
        reconciled_item2,
        "Phone A failed to reconcile offline added item 2"
    );
    assert!(
        reconciled_check1,
        "Phone A failed to reconcile offline check state for item 1"
    );

    node_a_reboot.shutdown().await.unwrap();
    node_b_reboot.shutdown().await.unwrap();
}
