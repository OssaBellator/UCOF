/// Borrow direction selected by the non-normative deletion-frontier inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExperimentalDeleteBorrowDirection {
    /// The left sibling would lend.
    Left,
    /// The right sibling would lend.
    Right,
}

/// Read-only leaf-frontier state for one experimental persistent deletion.
///
/// This structure is research instrumentation only. It exposes the occupancy state
/// already consumed by the persistent deletion experiment so workload traces can
/// connect borrower choice to later immutable write amplification. It has no wire
/// compatibility or normative-policy meaning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentalDeleteLeafFrontier {
    /// Object that would be deleted.
    pub object_id: u64,
    /// Active root level before deletion.
    pub root_level: u8,
    /// Occupancy of the target leaf before deleting `object_id`.
    pub target_occupancy: usize,
    /// Occupancy of the immediate left leaf sibling, when present.
    pub left_occupancy: Option<usize>,
    /// Occupancy of the immediate right leaf sibling, when present.
    pub right_occupancy: Option<usize>,
    /// Whether deleting from the target leaf reaches the non-root underflow frontier.
    pub would_underflow: bool,
    /// Borrow direction selected by the requested experimental policy, when borrowing applies.
    pub selected_donor_direction: Option<ExperimentalDeleteBorrowDirection>,
    /// Pre-deletion occupancy of the selected donor, when borrowing applies.
    pub selected_donor_occupancy: Option<usize>,
    /// Whether the selected donor is barely eligible (`minimum + 1`) and would become minimum.
    pub donor_cliff: bool,
    /// Whether a donor-cliff selection had another eligible sibling that was strictly fuller.
    pub strictly_fuller_eligible_alternative: bool,
    /// Whether the underflow has no eligible donor and would therefore merge at the leaf level.
    pub would_merge: bool,
    /// Number of active leaves before deletion.
    pub leaf_count: usize,
    /// Number of active leaves exactly at `LEAF_MIN_OCCUPANCY` before deletion.
    pub minimum_leaf_count: usize,
}

fn experimental_active_leaf_counts(
    data: &[u8],
    root: &PageRef,
    limits: ImmutableLimits,
) -> Result<(usize, usize), ImmutableError> {
    let mut stack = vec![root.clone()];
    let mut visited = 0_usize;
    let mut leaf_count = 0_usize;
    let mut minimum_leaf_count = 0_usize;

    while let Some(reference) = stack.pop() {
        visited = visited
            .checked_add(1)
            .ok_or(ImmutableError::Limit("page count"))?;
        if visited > limits.max_pages {
            return Err(ImmutableError::Limit("page count"));
        }

        match load_deletion_node(data, &reference, limits)? {
            PendingDeletionNode::Leaf(entries) => {
                leaf_count = leaf_count
                    .checked_add(1)
                    .ok_or(ImmutableError::Limit("page count"))?;
                if entries.len() == LEAF_MIN_OCCUPANCY {
                    minimum_leaf_count = minimum_leaf_count
                        .checked_add(1)
                        .ok_or(ImmutableError::Limit("page count"))?;
                }
            }
            PendingDeletionNode::Internal { children, .. } => {
                let required = stack
                    .len()
                    .checked_add(children.len())
                    .ok_or(ImmutableError::Limit("page count"))?;
                allocation_check::<PageRef>(required, limits)?;
                stack.extend(children);
            }
        }
    }

    Ok((leaf_count, minimum_leaf_count))
}

fn experimental_leaf_occupancy(
    data: &[u8],
    reference: &PageRef,
    limits: ImmutableLimits,
) -> Result<usize, ImmutableError> {
    match load_deletion_node(data, reference, limits)? {
        PendingDeletionNode::Leaf(entries) => Ok(entries.len()),
        PendingDeletionNode::Internal { .. } => Err(ImmutableError::Invalid(
            "deletion frontier leaf sibling",
        )),
    }
}

/// Inspects the exact leaf-level occupancy frontier that an experimental persistent deletion
/// would consume, without producing successor bytes.
///
/// The file is strictly validated with canonical occupancy first. The returned donor choice uses
/// the same `choose_deletion_borrow_side` function as the writer. Root-leaf deletions are reported
/// as non-underflowing because the root is exempt from the non-root minimum occupancy rule.
pub fn inspect_persistent_delete_leaf_frontier_experimental(
    data: &[u8],
    object_id: u64,
    limits: ImmutableLimits,
    borrow_policy: ExperimentalDeleteBorrowPolicy,
) -> Result<ExperimentalDeleteLeafFrontier, ImmutableError> {
    if data.len() > limits.max_output_bytes {
        return Err(ImmutableError::Limit("output"));
    }
    if object_id == 0 {
        return Err(ImmutableError::Invalid("batch object id"));
    }

    let previous = validate_canonical_internal(data, limits)?;
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
    let root_level = root.level;
    let (leaf_count, minimum_leaf_count) =
        experimental_active_leaf_counts(data, &root, limits)?;

    if root.level == 0 {
        let PendingDeletionNode::Leaf(entries) = load_deletion_node(data, &root, limits)? else {
            return Err(ImmutableError::Invalid("deletion frontier root leaf"));
        };
        entries
            .binary_search_by_key(&object_id, |entry| entry.object_id)
            .map_err(|_| ImmutableError::MissingObject(object_id))?;
        return Ok(ExperimentalDeleteLeafFrontier {
            object_id,
            root_level,
            target_occupancy: entries.len(),
            left_occupancy: None,
            right_occupancy: None,
            would_underflow: false,
            selected_donor_direction: None,
            selected_donor_occupancy: None,
            donor_cliff: false,
            strictly_fuller_eligible_alternative: false,
            would_merge: false,
            leaf_count,
            minimum_leaf_count,
        });
    }

    let mut current = root;
    loop {
        let PendingDeletionNode::Internal { children, .. } =
            load_deletion_node(data, &current, limits)?
        else {
            return Err(ImmutableError::Invalid("deletion frontier path"));
        };
        let child_index = children
            .iter()
            .position(|child| child.minimum <= object_id && object_id <= child.maximum)
            .ok_or(ImmutableError::MissingObject(object_id))?;
        let child = children[child_index].clone();

        if child.level != 0 {
            current = child;
            continue;
        }

        let PendingDeletionNode::Leaf(entries) = load_deletion_node(data, &child, limits)? else {
            return Err(ImmutableError::Invalid("deletion frontier target leaf"));
        };
        entries
            .binary_search_by_key(&object_id, |entry| entry.object_id)
            .map_err(|_| ImmutableError::MissingObject(object_id))?;
        let target_occupancy = entries.len();
        let left_occupancy = if child_index > 0 {
            Some(experimental_leaf_occupancy(
                data,
                &children[child_index - 1],
                limits,
            )?)
        } else {
            None
        };
        let right_occupancy = if child_index + 1 < children.len() {
            Some(experimental_leaf_occupancy(
                data,
                &children[child_index + 1],
                limits,
            )?)
        } else {
            None
        };
        let would_underflow = target_occupancy == LEAF_MIN_OCCUPANCY;
        let side = if would_underflow {
            choose_deletion_borrow_side(
                borrow_policy,
                left_occupancy,
                right_occupancy,
                LEAF_MIN_OCCUPANCY,
            )
        } else {
            None
        };
        let (selected_donor_direction, selected_donor_occupancy, other_occupancy) = match side {
            Some(DeletionBorrowSide::Left) => (
                Some(ExperimentalDeleteBorrowDirection::Left),
                left_occupancy,
                right_occupancy,
            ),
            Some(DeletionBorrowSide::Right) => (
                Some(ExperimentalDeleteBorrowDirection::Right),
                right_occupancy,
                left_occupancy,
            ),
            None => (None, None, None),
        };
        let donor_cliff = selected_donor_occupancy == Some(LEAF_MIN_OCCUPANCY + 1);
        let strictly_fuller_eligible_alternative = donor_cliff
            && other_occupancy.is_some_and(|occupancy| {
                occupancy > LEAF_MIN_OCCUPANCY
                    && occupancy > selected_donor_occupancy.expect("selected donor occupancy")
            });
        let would_merge = would_underflow && selected_donor_occupancy.is_none();

        return Ok(ExperimentalDeleteLeafFrontier {
            object_id,
            root_level,
            target_occupancy,
            left_occupancy,
            right_occupancy,
            would_underflow,
            selected_donor_direction,
            selected_donor_occupancy,
            donor_cliff,
            strictly_fuller_eligible_alternative,
            would_merge,
            leaf_count,
            minimum_leaf_count,
        });
    }
}
