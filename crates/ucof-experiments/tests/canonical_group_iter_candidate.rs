#[path = "../src/canonical_group_iter_candidate.rs"]
mod canonical_group_iter_candidate;

use canonical_group_iter_candidate::{CanonicalGroupIterError, CanonicalGroupSizesIter};

fn reference(total: usize, capacity: usize, minimum: usize) -> Vec<usize> {
    let groups = total.div_ceil(capacity);
    if groups == 1 {
        return vec![total];
    }
    let full_groups = total / capacity;
    let remainder = total % capacity;
    if remainder == 0 {
        return vec![capacity; full_groups];
    }
    if remainder >= minimum {
        let mut sizes = vec![capacity; full_groups];
        sizes.push(remainder);
        return sizes;
    }
    let mut sizes = vec![capacity; full_groups - 1];
    let transfer = minimum - remainder;
    sizes.push(capacity - transfer);
    sizes.push(minimum);
    sizes
}

fn assert_matches(total: usize, capacity: usize, minimum: usize) {
    let actual: Vec<_> = CanonicalGroupSizesIter::new(total, capacity, minimum)
        .expect("valid canonical partition")
        .collect();
    let expected = reference(total, capacity, minimum);
    assert_eq!(actual, expected);
    assert_eq!(actual.iter().sum::<usize>(), total);
    assert!(actual.iter().all(|size| *size <= capacity));
    if actual.len() > 1 {
        assert!(actual.iter().all(|size| *size >= minimum));
    }
}

#[test]
fn generic_small_geometries_match_reference_exhaustively() {
    for capacity in 1usize..=32 {
        for minimum in 1usize..=capacity {
            for total in 1usize..=4 * capacity + 3 {
                assert_matches(total, capacity, minimum);
            }
        }
    }
}

#[test]
fn immutable_format_geometries_match_reference_across_boundaries() {
    for (capacity, minimum) in [(185usize, 93usize), (255usize, 128usize)] {
        for total in 1usize..=4 * capacity + 3 {
            assert_matches(total, capacity, minimum);
        }
    }
}

#[test]
fn iterator_reports_exact_remaining_group_count() {
    let mut sizes = CanonicalGroupSizesIter::new(400, 185, 93).expect("partition");
    assert_eq!(sizes.len(), 3);
    assert_eq!(sizes.next(), Some(185));
    assert_eq!(sizes.len(), 2);
    assert_eq!(sizes.next(), Some(122));
    assert_eq!(sizes.next(), Some(93));
    assert_eq!(sizes.len(), 0);
    assert_eq!(sizes.next(), None);
}

#[test]
fn invalid_geometry_is_rejected_without_allocation() {
    assert_eq!(
        CanonicalGroupSizesIter::new(0, 185, 93).expect_err("zero total"),
        CanonicalGroupIterError::Invalid
    );
    assert_eq!(
        CanonicalGroupSizesIter::new(1, 0, 1).expect_err("zero capacity"),
        CanonicalGroupIterError::Invalid
    );
    assert_eq!(
        CanonicalGroupSizesIter::new(1, 1, 0).expect_err("zero minimum"),
        CanonicalGroupIterError::Invalid
    );
    assert_eq!(
        CanonicalGroupSizesIter::new(1, 1, 2).expect_err("minimum above capacity"),
        CanonicalGroupIterError::Invalid
    );
}
