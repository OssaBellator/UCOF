/// One level of a read-only experimental persistent-deletion repair path.
///
/// Levels are returned bottom-up. Level zero is the target leaf. Higher levels
/// appear only when a lower merge removes one child and therefore changes the
/// occupancy of the ancestor node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentalDeleteRepairLevel {
    /// Tree level of the node being changed (`0` is a leaf).
    pub level: u8,
    /// Whether this event is the root leaf rather than a non-root repair frontier.
    pub is_root: bool,
    /// Whether the local occupancy change was caused by a lower-level child merge.
    pub triggered_by_child_removal: bool,
    /// Occupancy before deleting the object or removing the merged child.
    pub target_occupancy_before: usize,
    /// Occupancy immediately after the local deletion/child removal and before repair.
    pub target_occupancy_after_local_change: usize,
    /// Occupancy of the immediate left sibling at the same level, when present.
    pub left_occupancy: Option<usize>,
    /// Occupancy of the immediate right sibling at the same level, when present.
    pub right_occupancy: Option<usize>,
    /// Whether the post-change occupancy is below the non-root minimum.
    pub would_underflow: bool,
    /// Borrow direction selected by the requested experimental policy, when borrowing applies.
    pub selected_donor_direction: Option<ExperimentalDeleteBorrowDirection>,
    /// Pre-repair occupancy of the selected donor, when borrowing applies.
    pub selected_donor_occupancy: Option<usize>,
    /// Whether the selected donor is exactly `minimum + 1` and would become minimum.
    pub donor_cliff: bool,
    /// Whether a donor-cliff selection had another eligible sibling that was strictly fuller.
    pub strictly_fuller_eligible_alternative: bool,
    /// Whether no eligible donor exists and this node would merge into a sibling.
    pub would_merge: bool,
}

/// Read-only bottom-up repair path for one experimental persistent deletion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentalDeleteRepairPath {
    /// Object that would be deleted.
    pub object_id: u64,
    /// Active root level before deletion.
    pub root_level: u8,
    /// Repair/change levels reached, ordered from the target leaf upward.
    pub levels: Vec<ExperimentalDeleteRepairLevel>,
    /// Whether the highest non-root repair merges and therefore removes a child from the root.
    pub root_child_removed: bool,
    /// Whether that root-child removal would collapse a two-child root to its sole child.
    pub root_would_collapse: bool,
}

#[derive(Clone, Debug)]
struct ExperimentalDeletePathFrame {
    level: u8,
    occupancy: usize,
    left_occupancy: Option<usize>,
    right_occupancy: Option<usize>,
}

fn experimental_deletion_node_occupancy(
    data: &[u8],
    reference: &PageRef,
    limits: ImmutableLimits,
) -> Result<usize, ImmutableError> {
    Ok(load_deletion_node(data, reference, limits)?.occupancy())
}

fn experimental_classify_repair_level(
    frame: &ExperimentalDeletePathFrame,
    occupancy_after_local_change: usize,
    triggered_by_child_removal: bool,
    borrow_policy: ExperimentalDeleteBorrowPolicy,
) -> ExperimentalDeleteRepairLevel {
    let minimum = deletion_minimum(frame.level);
    let would_underflow = occupancy_after_local_change < minimum;
    let side = if would_underflow {
        choose_deletion_borrow_side(
            borrow_policy,
            frame.left_occupancy,
            frame.right_occupancy,
            minimum,
        )
    } else {
        None
    };
    let (selected_donor_direction, selected_donor_occupancy, other_occupancy) = match side {
        Some(DeletionBorrowSide::Left) => (
            Some(ExperimentalDeleteBorrowDirection::Left),
            frame.left_occupancy,
            frame.right_occupancy,
        ),
        Some(DeletionBorrowSide::Right) => (
            Some(ExperimentalDeleteBorrowDirection::Right),
            frame.right_occupancy,
            frame.left_occupancy,
        ),
        None => (None, None, None),
    };
    let donor_cliff = selected_donor_occupancy == Some(minimum + 1);
    let strictly_fuller_eligible_alternative = donor_cliff
        && other_occupancy.is_some_and(|occupancy| {
            occupancy > minimum
                && occupancy > selected_donor_occupancy.expect("selected donor occupancy")
        });

    ExperimentalDeleteRepairLevel {
        level: frame.level,
        is_root: false,
        triggered_by_child_removal,
        target_occupancy_before: frame.occupancy,
        target_occupancy_after_local_change: occupancy_after_local_change,
        left_occupancy: frame.left_occupancy,
        right_occupancy: frame.right_occupancy,
        would_underflow,
        selected_donor_direction,
        selected_donor_occupancy,
        donor_cliff,
        strictly_fuller_eligible_alternative,
        would_merge: would_underflow && selected_donor_occupancy.is_none(),
    }
}

/// Inspects the bottom-up repair path for an experimental persistent deletion without
/// producing successor bytes.
///
/// The path uses the same occupancy minima and `choose_deletion_borrow_side` helper as
/// the writer. Higher levels are reached only when the lower repair merges and removes
/// one child. Root occupancy is treated with the writer's root exception rather than the
/// non-root minimum.
pub fn inspect_persistent_delete_repair_path_experimental(
    data: &[u8],
    object_id: u64,
    limits: ImmutableLimits,
    borrow_policy: ExperimentalDeleteBorrowPolicy,
) -> Result<ExperimentalDeleteRepairPath, ImmutableError> {
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
    let root_node = load_deletion_node(data, &root, limits)?;
    let root_occupancy = root_node.occupancy();

    if root.level == 0 {
        let PendingDeletionNode::Leaf(entries) = root_node else {
            return Err(ImmutableError::Invalid("deletion repair root leaf"));
        };
        entries
            .binary_search_by_key(&object_id, |entry| entry.object_id)
            .map_err(|_| ImmutableError::MissingObject(object_id))?;
        let after = entries
            .len()
            .checked_sub(1)
            .ok_or(ImmutableError::Invalid("deletion repair root leaf"))?;
        return Ok(ExperimentalDeleteRepairPath {
            object_id,
            root_level,
            levels: vec![ExperimentalDeleteRepairLevel {
                level: 0,
                is_root: true,
                triggered_by_child_removal: false,
                target_occupancy_before: entries.len(),
                target_occupancy_after_local_change: after,
                left_occupancy: None,
                right_occupancy: None,
                would_underflow: false,
                selected_donor_direction: None,
                selected_donor_occupancy: None,
                donor_cliff: false,
                strictly_fuller_eligible_alternative: false,
                would_merge: false,
            }],
            root_child_removed: false,
            root_would_collapse: false,
        });
    }

    let mut frames = Vec::new();
    let mut current = root.clone();
    while current.level > 0 {
        if frames.len() >= usize::from(limits.max_depth) {
            return Err(ImmutableError::Limit("tree depth"));
        }
        let PendingDeletionNode::Internal { children, .. } =
            load_deletion_node(data, &current, limits)?
        else {
            return Err(ImmutableError::Invalid("deletion repair path"));
        };
        let child_index = children
            .iter()
            .position(|child| child.minimum <= object_id && object_id <= child.maximum)
            .ok_or(ImmutableError::MissingObject(object_id))?;
        let child = children[child_index].clone();
        let occupancy = experimental_deletion_node_occupancy(data, &child, limits)?;
        let left_occupancy = if child_index > 0 {
            Some(experimental_deletion_node_occupancy(
                data,
                &children[child_index - 1],
                limits,
            )?)
        } else {
            None
        };
        let right_occupancy = if child_index + 1 < children.len() {
            Some(experimental_deletion_node_occupancy(
                data,
                &children[child_index + 1],
                limits,
            )?)
        } else {
            None
        };
        frames.push(ExperimentalDeletePathFrame {
            level: child.level,
            occupancy,
            left_occupancy,
            right_occupancy,
        });
        current = child;
    }

    let PendingDeletionNode::Leaf(entries) = load_deletion_node(data, &current, limits)? else {
        return Err(ImmutableError::Invalid("deletion repair target leaf"));
    };
    entries
        .binary_search_by_key(&object_id, |entry| entry.object_id)
        .map_err(|_| ImmutableError::MissingObject(object_id))?;
    let leaf_frame = frames
        .last()
        .ok_or(ImmutableError::Invalid("deletion repair leaf frame"))?;
    if leaf_frame.level != 0 || leaf_frame.occupancy != entries.len() {
        return Err(ImmutableError::Invalid("deletion repair leaf frame"));
    }

    let leaf_after = leaf_frame
        .occupancy
        .checked_sub(1)
        .ok_or(ImmutableError::Invalid("deletion repair leaf occupancy"))?;
    let leaf_event = experimental_classify_repair_level(
        leaf_frame,
        leaf_after,
        false,
        borrow_policy,
    );
    let mut child_removed = leaf_event.would_merge;
    let mut levels = vec![leaf_event];

    if child_removed {
        for frame in frames[..frames.len() - 1].iter().rev() {
            let after = frame
                .occupancy
                .checked_sub(1)
                .ok_or(ImmutableError::Invalid("deletion repair internal occupancy"))?;
            let event = experimental_classify_repair_level(frame, after, true, borrow_policy);
            child_removed = event.would_merge;
            levels.push(event);
            if !child_removed {
                break;
            }
        }
    }

    let root_child_removed = child_removed;
    let root_would_collapse = root_child_removed && root_occupancy == 2;
    Ok(ExperimentalDeleteRepairPath {
        object_id,
        root_level,
        levels,
        root_child_removed,
        root_would_collapse,
    })
}
