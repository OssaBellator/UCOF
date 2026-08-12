/// Non-normative borrower-selection variants for the persistent deletion experiment.
///
/// `LeftFirst` preserves the current byte-significant behavior exactly. The alternate
/// variant is exposed only so review can measure a fuller-eligible-sibling rule before
/// EXP-0003 mutation bytes are frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExperimentalDeleteBorrowPolicy {
    LeftFirst,
    FullerSiblingLeftTie,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeletionBorrowSide {
    Left,
    Right,
}

#[derive(Clone, Debug)]
enum PendingDeletionNode {
    Leaf(Vec<Locator>),
    Internal { level: u8, children: Vec<PageRef> },
}

impl PendingDeletionNode {
    fn level(&self) -> u8 {
        match self {
            Self::Leaf(_) => 0,
            Self::Internal { level, .. } => *level,
        }
    }

    fn occupancy(&self) -> usize {
        match self {
            Self::Leaf(entries) => entries.len(),
            Self::Internal { children, .. } => children.len(),
        }
    }
}

fn deletion_minimum(level: u8) -> usize {
    if level == 0 {
        LEAF_MIN_OCCUPANCY
    } else {
        INTERNAL_MIN_OCCUPANCY
    }
}

fn choose_deletion_borrow_side(
    policy: ExperimentalDeleteBorrowPolicy,
    left_occupancy: Option<usize>,
    right_occupancy: Option<usize>,
    minimum: usize,
) -> Option<DeletionBorrowSide> {
    let left_can_lend = left_occupancy.is_some_and(|occupancy| occupancy > minimum);
    let right_can_lend = right_occupancy.is_some_and(|occupancy| occupancy > minimum);

    match policy {
        ExperimentalDeleteBorrowPolicy::LeftFirst => {
            if left_can_lend {
                Some(DeletionBorrowSide::Left)
            } else if right_can_lend {
                Some(DeletionBorrowSide::Right)
            } else {
                None
            }
        }
        ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie => match (
            left_can_lend,
            right_can_lend,
        ) {
            (true, true) => {
                if left_occupancy.expect("eligible left occupancy")
                    >= right_occupancy.expect("eligible right occupancy")
                {
                    Some(DeletionBorrowSide::Left)
                } else {
                    Some(DeletionBorrowSide::Right)
                }
            }
            (true, false) => Some(DeletionBorrowSide::Left),
            (false, true) => Some(DeletionBorrowSide::Right),
            (false, false) => None,
        },
    }
}

fn load_deletion_node(
    data: &[u8],
    reference: &PageRef,
    limits: ImmutableLimits,
) -> Result<PendingDeletionNode, ImmutableError> {
    let page = checked_persistent_page(data, reference)?;
    if reference.level == 0 {
        Ok(PendingDeletionNode::Leaf(decode_persistent_leaf(
            page, reference, limits,
        )?))
    } else {
        Ok(PendingDeletionNode::Internal {
            level: reference.level,
            children: decode_persistent_children(page, reference, limits)?,
        })
    }
}

fn materialize_deletion_node(
    output: &mut Vec<u8>,
    node: &PendingDeletionNode,
    limits: ImmutableLimits,
    pages_written: &mut usize,
) -> Result<PageRef, ImmutableError> {
    match node {
        PendingDeletionNode::Leaf(entries) => append_cow_page(
            output,
            &encode_leaf(entries)?,
            limits,
            pages_written,
        ),
        PendingDeletionNode::Internal { level, children } => append_cow_page(
            output,
            &encode_internal(children, *level)?,
            limits,
            pages_written,
        ),
    }
}

fn borrow_deletion_from_left(
    left: &mut PendingDeletionNode,
    target: &mut PendingDeletionNode,
    limits: ImmutableLimits,
) -> Result<(), ImmutableError> {
    match (left, target) {
        (PendingDeletionNode::Leaf(left), PendingDeletionNode::Leaf(target)) => {
            allocation_check::<Locator>(target.len() + 1, limits)?;
            let entry = left
                .pop()
                .ok_or(ImmutableError::Invalid("deletion left borrow"))?;
            target.insert(0, entry);
        }
        (
            PendingDeletionNode::Internal {
                level: left_level,
                children: left,
            },
            PendingDeletionNode::Internal {
                level: target_level,
                children: target,
            },
        ) if left_level == target_level => {
            allocation_check::<PageRef>(target.len() + 1, limits)?;
            let child = left
                .pop()
                .ok_or(ImmutableError::Invalid("deletion left borrow"))?;
            target.insert(0, child);
        }
        _ => return Err(ImmutableError::Invalid("deletion sibling level")),
    }
    Ok(())
}

fn borrow_deletion_from_right(
    target: &mut PendingDeletionNode,
    right: &mut PendingDeletionNode,
    limits: ImmutableLimits,
) -> Result<(), ImmutableError> {
    match (target, right) {
        (PendingDeletionNode::Leaf(target), PendingDeletionNode::Leaf(right)) => {
            allocation_check::<Locator>(target.len() + 1, limits)?;
            if right.is_empty() {
                return Err(ImmutableError::Invalid("deletion right borrow"));
            }
            target.push(right.remove(0));
        }
        (
            PendingDeletionNode::Internal {
                level: target_level,
                children: target,
            },
            PendingDeletionNode::Internal {
                level: right_level,
                children: right,
            },
        ) if target_level == right_level => {
            allocation_check::<PageRef>(target.len() + 1, limits)?;
            if right.is_empty() {
                return Err(ImmutableError::Invalid("deletion right borrow"));
            }
            target.push(right.remove(0));
        }
        _ => return Err(ImmutableError::Invalid("deletion sibling level")),
    }
    Ok(())
}

fn merge_deletion_nodes(
    mut left: PendingDeletionNode,
    right: PendingDeletionNode,
    limits: ImmutableLimits,
) -> Result<PendingDeletionNode, ImmutableError> {
    match (&mut left, right) {
        (PendingDeletionNode::Leaf(left_entries), PendingDeletionNode::Leaf(right_entries)) => {
            let merged = left_entries
                .len()
                .checked_add(right_entries.len())
                .ok_or(ImmutableError::Limit("page count"))?;
            if merged > LEAF_CAPACITY {
                return Err(ImmutableError::Invalid("deletion leaf merge"));
            }
            allocation_check::<Locator>(merged, limits)?;
            left_entries.extend(right_entries);
        }
        (
            PendingDeletionNode::Internal {
                level: left_level,
                children: left_children,
            },
            PendingDeletionNode::Internal {
                level: right_level,
                children: right_children,
            },
        ) if *left_level == right_level => {
            let merged = left_children
                .len()
                .checked_add(right_children.len())
                .ok_or(ImmutableError::Limit("page count"))?;
            if merged > INTERNAL_FANOUT {
                return Err(ImmutableError::Invalid("deletion internal merge"));
            }
            allocation_check::<PageRef>(merged, limits)?;
            left_children.extend(right_children);
        }
        _ => return Err(ImmutableError::Invalid("deletion sibling level")),
    }
    Ok(left)
}

fn increment_touched(
    touched_original: &mut usize,
    limits: ImmutableLimits,
) -> Result<(), ImmutableError> {
    *touched_original = touched_original
        .checked_add(1)
        .ok_or(ImmutableError::Limit("page count"))?;
    if *touched_original > limits.max_pages {
        return Err(ImmutableError::Limit("page count"));
    }
    Ok(())
}

fn delete_persistent_node(
    data: &[u8],
    output: &mut Vec<u8>,
    reference: &PageRef,
    object_id: u64,
    limits: ImmutableLimits,
    pages_written: &mut usize,
    touched_original: &mut usize,
    borrow_policy: ExperimentalDeleteBorrowPolicy,
) -> Result<PendingDeletionNode, ImmutableError> {
    increment_touched(touched_original, limits)?;
    let mut node = load_deletion_node(data, reference, limits)?;
    if let PendingDeletionNode::Leaf(entries) = &mut node {
        let position = entries
            .binary_search_by_key(&object_id, |entry| entry.object_id)
            .map_err(|_| ImmutableError::MissingObject(object_id))?;
        entries.remove(position);
        return Ok(node);
    }

    let PendingDeletionNode::Internal { level, children } = &mut node else {
        return Err(ImmutableError::Invalid("deletion node"));
    };
    let child_index = children
        .iter()
        .position(|child| child.minimum <= object_id && object_id <= child.maximum)
        .ok_or(ImmutableError::MissingObject(object_id))?;
    let mut target = delete_persistent_node(
        data,
        output,
        &children[child_index],
        object_id,
        limits,
        pages_written,
        touched_original,
        borrow_policy,
    )?;
    let minimum = deletion_minimum(target.level());

    if target.occupancy() >= minimum {
        children[child_index] =
            materialize_deletion_node(output, &target, limits, pages_written)?;
        return Ok(node);
    }

    let mut left = if child_index > 0 {
        Some(load_deletion_node(data, &children[child_index - 1], limits)?)
    } else {
        None
    };
    let mut right = if child_index + 1 < children.len() {
        Some(load_deletion_node(data, &children[child_index + 1], limits)?)
    } else {
        None
    };

    match choose_deletion_borrow_side(
        borrow_policy,
        left.as_ref().map(PendingDeletionNode::occupancy),
        right.as_ref().map(PendingDeletionNode::occupancy),
        minimum,
    ) {
        Some(DeletionBorrowSide::Left) => {
            let left_node = left
                .as_mut()
                .ok_or(ImmutableError::Invalid("deletion left borrow"))?;
            increment_touched(touched_original, limits)?;
            borrow_deletion_from_left(left_node, &mut target, limits)?;
            children[child_index - 1] =
                materialize_deletion_node(output, left_node, limits, pages_written)?;
            children[child_index] =
                materialize_deletion_node(output, &target, limits, pages_written)?;
            return Ok(node);
        }
        Some(DeletionBorrowSide::Right) => {
            let right_node = right
                .as_mut()
                .ok_or(ImmutableError::Invalid("deletion right borrow"))?;
            increment_touched(touched_original, limits)?;
            borrow_deletion_from_right(&mut target, right_node, limits)?;
            children[child_index] =
                materialize_deletion_node(output, &target, limits, pages_written)?;
            children[child_index + 1] =
                materialize_deletion_node(output, right_node, limits, pages_written)?;
            return Ok(node);
        }
        None => {}
    }

    if let Some(left_node) = left {
        increment_touched(touched_original, limits)?;
        let merged = merge_deletion_nodes(left_node, target, limits)?;
        children[child_index - 1] =
            materialize_deletion_node(output, &merged, limits, pages_written)?;
        children.remove(child_index);
    } else if let Some(right_node) = right {
        increment_touched(touched_original, limits)?;
        let merged = merge_deletion_nodes(target, right_node, limits)?;
        children[child_index] =
            materialize_deletion_node(output, &merged, limits, pages_written)?;
        children.remove(child_index + 1);
    } else {
        return Err(ImmutableError::Invalid("deletion sibling"));
    }

    if *level == 0 {
        return Err(ImmutableError::Invalid("deletion internal level"));
    }
    Ok(node)
}

fn append_persistent_delete_from_previous_with_policy(
    data: &[u8],
    object_id: u64,
    previous: InternalReport,
    limits: ImmutableLimits,
    borrow_policy: ExperimentalDeleteBorrowPolicy,
) -> Result<PersistentBatchResult, ImmutableError> {
    if object_id == 0 {
        return Err(ImmutableError::Invalid("batch object id"));
    }
    if previous.locators.len() <= 1 {
        return Err(ImmutableError::Invalid("batch result"));
    }
    if previous
        .locators
        .binary_search_by_key(&object_id, |locator| locator.object_id)
        .is_err()
    {
        return Err(ImmutableError::MissingObject(object_id));
    }

    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut output = data.to_vec();
    let mut pages_written = 0_usize;
    let mut touched_original = 0_usize;
    let pending = delete_persistent_node(
        data,
        &mut output,
        &root,
        object_id,
        limits,
        &mut pages_written,
        &mut touched_original,
        borrow_policy,
    )?;

    let next_root = match pending {
        PendingDeletionNode::Leaf(entries) => materialize_deletion_node(
            &mut output,
            &PendingDeletionNode::Leaf(entries),
            limits,
            &mut pages_written,
        )?,
        PendingDeletionNode::Internal { level: _, children } if children.len() == 1 => children
            .into_iter()
            .next()
            .ok_or(ImmutableError::Invalid("deletion root collapse"))?,
        PendingDeletionNode::Internal { level, children } => materialize_deletion_node(
            &mut output,
            &PendingDeletionNode::Internal { level, children },
            limits,
            &mut pages_written,
        )?,
    };

    publish(
        &mut output,
        previous
            .public
            .sequence
            .checked_add(1)
            .ok_or(ImmutableError::Limit("sequence"))?,
        &next_root,
        previous.public.snapshot_digest,
        u64_from_usize(previous.footer_offset)?,
        pages_written,
        limits,
    )?;
    let report = validate_canonical_occupancy(&output, limits)?;
    let pages_reused = previous
        .public
        .page_count
        .checked_sub(touched_original)
        .ok_or(ImmutableError::Invalid("persistent page accounting"))?;
    Ok(PersistentBatchResult {
        bytes: output,
        report,
        mode: PersistentBatchMode::CopyOnWriteDeletion,
        pages_written,
        pages_reused,
    })
}

fn append_persistent_delete_from_previous(
    data: &[u8],
    object_id: u64,
    previous: InternalReport,
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    append_persistent_delete_from_previous_with_policy(
        data,
        object_id,
        previous,
        limits,
        ExperimentalDeleteBorrowPolicy::LeftFirst,
    )
}

/// Deletes one active object through a persistent path with deterministic left-first borrowing,
/// merge fallback, recursive internal repair, and root collapse.
pub fn append_persistent_delete(
    data: &[u8],
    object_id: u64,
    limits: ImmutableLimits,
) -> Result<PersistentBatchResult, ImmutableError> {
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    let previous = validate_canonical_internal(data, limits)?;
    append_persistent_delete_from_previous(data, object_id, previous, limits)
}

/// Runs the persistent deletion experiment with an explicit non-normative borrower policy.
///
/// This API exists only to compare deterministic transition rules. `LeftFirst` is byte-identical
/// to [`append_persistent_delete`]. `FullerSiblingLeftTie` changes which eligible sibling lends
/// when both can lend; it does not change the occupancy floor or merge direction.
pub fn append_persistent_delete_experimental(
    data: &[u8],
    object_id: u64,
    limits: ImmutableLimits,
    borrow_policy: ExperimentalDeleteBorrowPolicy,
) -> Result<PersistentBatchResult, ImmutableError> {
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    let previous = validate_canonical_internal(data, limits)?;
    append_persistent_delete_from_previous_with_policy(
        data,
        object_id,
        previous,
        limits,
        borrow_policy,
    )
}

#[cfg(test)]
mod borrow_policy_tests {
    use super::*;

    #[test]
    fn fuller_policy_prefers_fuller_eligible_sibling_and_left_on_ties() {
        let minimum = 93;
        assert_eq!(
            choose_deletion_borrow_side(
                ExperimentalDeleteBorrowPolicy::LeftFirst,
                Some(94),
                Some(101),
                minimum,
            ),
            Some(DeletionBorrowSide::Left)
        );
        assert_eq!(
            choose_deletion_borrow_side(
                ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
                Some(94),
                Some(101),
                minimum,
            ),
            Some(DeletionBorrowSide::Right)
        );
        assert_eq!(
            choose_deletion_borrow_side(
                ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
                Some(101),
                Some(101),
                minimum,
            ),
            Some(DeletionBorrowSide::Left)
        );
        assert_eq!(
            choose_deletion_borrow_side(
                ExperimentalDeleteBorrowPolicy::FullerSiblingLeftTie,
                Some(minimum),
                Some(minimum),
                minimum,
            ),
            None
        );
    }
}
