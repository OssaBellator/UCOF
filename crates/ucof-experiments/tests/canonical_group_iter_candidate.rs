#[path = "../src/canonical_group_iter_candidate.rs"]
mod canonical_group_iter_candidate;

use canonical_group_iter_candidate::{CanonicalGroupIterError, CanonicalGroupSizesIter};

fn reference(
    total: usize,
    capacity: usize,
    minimum: usize,
) -> Result<Vec<usize>, CanonicalGroupIterError> {
    if total == 0 || capacity == 0 || minimum == 0 || minimum > capacity {
        return Err(CanonicalGroupIterError::Invalid);
    }
    let groups = total
        .checked_add(capacity - 1)
        .ok_or(CanonicalGroupIterError::Overflow)?
        / capacity;
    let mut sizes = Vec::with_capacity(groups);
    if groups == 1 {
        sizes.push(total);
        return Ok(sizes);
    }
    let full_groups = total / capacity;
    let remainder = total % capacity;
    if remainder == 0 {
        sizes.resize(full_groups, capacity);
    } else if remainder >= minimum {
        sizes.resize(full_groups, capacity);
        sizes.push(remainder);
    } else {
        sizes.resize(full_groups, capacity);
        let transfer = minimum - remainder;
        let last = sizes.last_mut().ok_or(CanonicalGroupIterError::Invalid)?;
        *last = last
            .checked_sub(transfer)
            .ok_or(CanonicalGroupIterError::Invalid)?;
        sizes.push(minimum);
    }
    if sizes.iter().any(|size| *size > capacity || *size < minimum) {
        return Err(CanonicalGroupIterError::Invalid);
    }
    Ok(sizes)
}

fn assert_matches(total: usize, capacity: usize, minimum: usize) {
    let expected = reference(total, capacity, minimum);
    let actual = CanonicalGroupSizesIter::new(total, capacity, minimum)
        .map(|sizes| sizes.collect::<Vec<_>>());
    assert_eq!(actual, expected);
    if let Ok(actual) = actual {
        assert_eq!(actual.iter().sum::<usize>(), total);
        assert!(actual.iter().all(|size| *size <= capacity));
        if actual.len() > 1 {
            assert!(actual.iter().all(|size| *size >= minimum));
        }
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
    assert_eq!(
        CanonicalGroupSizesIter::new(3, 2, 2).expect_err("infeasible redistribution"),
        CanonicalGroupIterError::Invalid
    );
}
