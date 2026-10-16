use groshe_core::{to_cbor, DocKey, ItemCheck, ItemData, ItemId, ItemPosition, ListId};
use groshe_store::ListProjection;
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum TestAction {
    AddOrUpdateData {
        item_idx: usize,
        name: String,
        qty: Option<f64>,
        unit: Option<String>,
        cat: Option<String>,
    },
    SetCheck {
        item_idx: usize,
        checked: bool,
    },
    SetPosition {
        item_idx: usize,
        pos_str: String,
    },
    Delete {
        item_idx: usize,
    },
}

#[derive(Debug, Clone)]
struct GeneratedOp {
    key: Vec<u8>,
    value: Vec<u8>,
    timestamp: u64,
}

fn arbitrary_actions() -> impl Strategy<Value = Vec<TestAction>> {
    let names = prop_oneof![
        Just("Apples".to_string()),
        Just("Bananas".to_string()),
        Just("Milk".to_string()),
        Just("Bread".to_string()),
        Just("Eggs".to_string()),
        Just("Coffee".to_string()),
        Just("Butter".to_string()),
        Just("Rice".to_string()),
    ];

    let units = prop_oneof![
        Just(None),
        Just(Some("lbs".to_string())),
        Just(Some("gal".to_string())),
        Just(Some("carton".to_string())),
        Just(Some("oz".to_string())),
    ];

    let categories = prop_oneof![
        Just(None),
        Just(Some("Produce".to_string())),
        Just(Some("Dairy".to_string())),
        Just(Some("Bakery".to_string())),
    ];

    let pos_strings = prop_oneof![
        Just("a".to_string()),
        Just("am".to_string()),
        Just("c".to_string()),
        Just("k".to_string()),
        Just("n".to_string()),
        Just("p".to_string()),
        Just("t".to_string()),
        Just("z".to_string()),
    ];

    let action_strategy = prop_oneof![
        (
            0..6usize,
            names,
            proptest::option::of(1.0..10.0f64),
            units,
            categories
        )
            .prop_map(
                |(item_idx, name, qty, unit, cat)| TestAction::AddOrUpdateData {
                    item_idx,
                    name,
                    qty: qty.map(|q| (q * 10.0).round() / 10.0),
                    unit,
                    cat,
                }
            ),
        (0..6usize, any::<bool>())
            .prop_map(|(item_idx, checked)| TestAction::SetCheck { item_idx, checked }),
        (0..6usize, pos_strings)
            .prop_map(|(item_idx, pos_str)| TestAction::SetPosition { item_idx, pos_str }),
        (0..6usize).prop_map(|item_idx| TestAction::Delete { item_idx }),
    ];

    prop::collection::vec(action_strategy, 1..60)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn test_crdt_three_replica_convergence(actions in arbitrary_actions()) {
        let list_id = ListId::new_v7();

        let item_ids: Vec<ItemId> = (0..6)
            .map(|i| {
                let mut bytes = [0u8; 16];
                bytes[0] = 0x01;
                bytes[15] = i as u8 + 1;
                ItemId(uuid::Uuid::from_bytes(bytes))
            })
            .collect();

        let mut ops = Vec::new();
        let mut ts: u64 = 100;

        for action in actions {
            ts += 10;
            match action {
                TestAction::AddOrUpdateData { item_idx, name, qty, unit, cat } => {
                    let id = item_ids[item_idx % item_ids.len()];
                    let mut data = ItemData::new(name).unwrap();
                    if let Some(q) = qty {
                        data = data.with_quantity(q, unit).unwrap();
                    }
                    if let Some(c) = cat {
                        data = data.with_category(c);
                    }
                    ops.push(GeneratedOp {
                        key: DocKey::item_data(id).to_key_bytes(),
                        value: to_cbor(&data).unwrap(),
                        timestamp: ts,
                    });
                }
                TestAction::SetCheck { item_idx, checked } => {
                    let id = item_ids[item_idx % item_ids.len()];
                    let check = if checked {
                        ItemCheck::checked(ts, "author-1")
                    } else {
                        ItemCheck::unchecked()
                    };
                    ops.push(GeneratedOp {
                        key: DocKey::item_check(id).to_key_bytes(),
                        value: to_cbor(&check).unwrap(),
                        timestamp: ts,
                    });
                }
                TestAction::SetPosition { item_idx, pos_str } => {
                    let id = item_ids[item_idx % item_ids.len()];
                    let pos = ItemPosition::new(pos_str).unwrap();
                    ops.push(GeneratedOp {
                        key: DocKey::item_pos(id).to_key_bytes(),
                        value: to_cbor(&pos).unwrap(),
                        timestamp: ts,
                    });
                }
                TestAction::Delete { item_idx } => {
                    let id = item_ids[item_idx % item_ids.len()];
                    ops.push(GeneratedOp {
                        key: DocKey::item_data(id).to_key_bytes(),
                        value: vec![],
                        timestamp: ts,
                    });
                }
            }
        }

        // Replica 1: Applies ops in original generation order
        let mut r1 = ListProjection::new(list_id);
        for op in &ops {
            let _ = r1.apply_entry(&op.key, &op.value, op.timestamp);
        }

        // Replica 2: Replays ops with even indices first, then odd indices (interleaved reorder)
        let mut r2_ops = Vec::new();
        for (i, op) in ops.iter().enumerate() {
            if i % 2 == 0 {
                r2_ops.push(op.clone());
            }
        }
        for (i, op) in ops.iter().enumerate() {
            if i % 2 != 0 {
                r2_ops.push(op.clone());
            }
        }

        let mut r2 = ListProjection::new(list_id);
        for op in &r2_ops {
            let _ = r2.apply_entry(&op.key, &op.value, op.timestamp);
        }

        // Replica 3: Reversed order with duplicate retransmissions (idempotency check)
        let mut r3_ops: Vec<GeneratedOp> = ops.iter().cloned().rev().collect();
        for i in 0..r3_ops.len().min(3) {
            r3_ops.push(r3_ops[i].clone());
        }

        let mut r3 = ListProjection::new(list_id);
        for op in &r3_ops {
            let _ = r3.apply_entry(&op.key, &op.value, op.timestamp);
        }

        // ---------------------------------------------------------------------
        // Strong Eventual Consistency Assertions: All 3 replicas must converge
        // ---------------------------------------------------------------------
        prop_assert_eq!(r1.item_count(), r2.item_count());
        prop_assert_eq!(r1.item_count(), r3.item_count());
        prop_assert_eq!(r1.checked_count(), r2.checked_count());
        prop_assert_eq!(r1.checked_count(), r3.checked_count());

        let items1 = r1.sorted_items();
        let items2 = r2.sorted_items();
        let items3 = r3.sorted_items();

        prop_assert_eq!(items1.len(), items2.len());
        prop_assert_eq!(items1.len(), items3.len());

        for i in 0..items1.len() {
            let it1 = items1[i];
            let it2 = items2[i];
            let it3 = items3[i];

            prop_assert_eq!(it1.id, it2.id);
            prop_assert_eq!(it1.id, it3.id);

            prop_assert_eq!(it1.name(), it2.name());
            prop_assert_eq!(it1.name(), it3.name());

            prop_assert_eq!(it1.data.quantity, it2.data.quantity);
            prop_assert_eq!(it1.data.quantity, it3.data.quantity);

            prop_assert_eq!(&it1.data.unit, &it2.data.unit);
            prop_assert_eq!(&it1.data.unit, &it3.data.unit);

            prop_assert_eq!(&it1.data.category, &it2.data.category);
            prop_assert_eq!(&it1.data.category, &it3.data.category);

            prop_assert_eq!(it1.is_checked(), it2.is_checked());
            prop_assert_eq!(it1.is_checked(), it3.is_checked());

            prop_assert_eq!(&it1.position, &it2.position);
            prop_assert_eq!(&it1.position, &it3.position);
        }
    }
}
