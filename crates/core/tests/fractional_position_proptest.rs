use groshe_core::ItemPosition;
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_arbitrary_between_invariants(
        insert_positions in prop::collection::vec(0usize..100, 1..50)
    ) {
        let mut list: Vec<ItemPosition> = vec![ItemPosition::initial()];

        for raw_idx in insert_positions {
            let idx = raw_idx % (list.len() + 1);
            let prev = if idx > 0 { Some(&list[idx - 1]) } else { None };
            let next = if idx < list.len() { Some(&list[idx]) } else { None };

            let new_pos = ItemPosition::between(prev, next).expect("between must succeed for valid bounds");

            if let Some(p) = prev {
                prop_assert!(p < &new_pos, "Expected prev {:?} < new_pos {:?}", p, new_pos);
            }
            if let Some(n) = next {
                prop_assert!(&new_pos < n, "Expected new_pos {:?} < next {:?}", new_pos, n);
            }

            list.insert(idx, new_pos);
        }

        // Verify the entire array remains strictly sorted
        for i in 0..(list.len() - 1) {
            prop_assert!(list[i] < list[i + 1], "List monotonicity broken at index {}: {:?} vs {:?}", i, list[i], list[i+1]);
        }
    }
}