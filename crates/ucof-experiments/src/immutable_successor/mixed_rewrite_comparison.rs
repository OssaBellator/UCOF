use crate::{
    plan_mixed_leaf_updates, MixedLeafPlanError, MixedLeafPlanLimits, MixedPlanOperation,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixedRewriteComparisonError {
    Format(ImmutableError),
    Plan(MixedLeafPlanError),
}

impl std::fmt::Display for MixedRewriteComparisonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "mixed rewrite comparison failed: {error}"),
            Self::Plan(error) => write!(formatter, "mixed path-local plan failed: {error}"),
        }
    }
}

impl std::error::Error for MixedRewriteComparisonError {}

impl From<ImmutableError> for MixedRewriteComparisonError {
    fn from(error: ImmutableError) -> Self {
        Self::Format(error)
    }
}

impl From<MixedLeafPlanError> for MixedRewriteComparisonError {
    fn from(error: MixedLeafPlanError) -> Self {
        Self::Plan(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedRewriteComparisonReport {
    pub original_leaf_pages: usize,
    pub final_leaf_pages: usize,
    pub insertions: usize,
    pub replacements: usize,
    pub deletions: usize,
    pub path_local_touched_original_leaf_pages: usize,
    pub path_local_leaf_pages_written: usize,
    pub path_local_leaf_pages_reused: usize,
    pub canonical_leaf_pages_written: usize,
    pub canonical_leaf_pages_reused: usize,
    pub extra_canonical_leaf_writes: usize,
    pub exact_leaf_layout_match: bool,
    pub first_differing_leaf: Option<usize>,
    pub path_local_final_leaf_counts: Vec<usize>,
    pub canonical_final_leaf_counts: Vec<usize>,
}

fn original_mixed_leaf_bodies(
    originals: &[Vec<OriginalMixedPage>],
) -> Result<Vec<Vec<Locator>>, ImmutableError> {
    let mut leaves: Vec<(u64, Vec<Locator>)> = originals
        .first()
        .ok_or(ImmutableError::Invalid("mixed comparison leaves"))?
        .iter()
        .map(|page| match &page.body {
            OriginalMixedPageBody::Leaf(entries) => Ok((page.reference.minimum, entries.clone())),
            OriginalMixedPageBody::Internal(_) => {
                Err(ImmutableError::Invalid("mixed comparison leaf body"))
            }
        })
        .collect::<Result<_, _>>()?;
    leaves.sort_unstable_by_key(|(minimum, _)| *minimum);
    Ok(leaves.into_iter().map(|(_, entries)| entries).collect())
}

fn final_locator_page(
    final_locators: &[Locator],
    object_ids: &[u64],
) -> Result<Vec<Locator>, ImmutableError> {
    object_ids
        .iter()
        .map(|object_id| {
            final_locators
                .binary_search_by_key(object_id, |locator| locator.object_id)
                .map(|index| final_locators[index].clone())
                .map_err(|_| ImmutableError::MissingObject(*object_id))
        })
        .collect()
}

fn exact_leaf_reuse_counts(
    original: &[Vec<Locator>],
    final_pages: &[Vec<Locator>],
) -> (usize, usize) {
    let reused = final_pages
        .iter()
        .filter(|page| original.iter().any(|candidate| candidate == *page))
        .count();
    (final_pages.len() - reused, reused)
}

fn first_leaf_difference(left: &[Vec<u64>], right: &[Vec<u64>]) -> Option<usize> {
    let common = left.len().min(right.len());
    left.iter()
        .zip(right)
        .position(|(left_page, right_page)| left_page != right_page)
        .or_else(|| (left.len() != right.len()).then_some(common))
}

/// Compares the authenticated canonical mixed writer's global leaf regrouping with the valid
/// path-local repair model for the same complete mixed operation set.
///
/// This function does not treat path-local output as canonical bytes. It measures leaf-body reuse
/// under both legal occupancy layouts using the same final authenticated locators. A positive
/// `extra_canonical_leaf_writes` value therefore identifies the rewrite cost of the current global
/// canonical grouping rule, not a correctness defect in the canonical writer.
pub fn compare_persistent_mixed_leaf_rewrites(
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
) -> Result<MixedRewriteComparisonReport, MixedRewriteComparisonError> {
    let previous = validate_canonical_internal(data, limits)?;
    let order = canonical_mixed_operation_order(operations, &previous, limits)?;
    let footer = parse_footer(data, previous.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut originals = vec![Vec::new(); usize::from(root.level) + 1];
    let mut visited = 0_usize;
    collect_original_mixed_pages(data, &root, &mut originals, limits, &mut visited)?;
    if visited != previous.public.page_count {
        return Err(ImmutableError::Invalid("mixed comparison page inventory").into());
    }
    let original_leaves = original_mixed_leaf_bodies(&originals)?;
    let original_ids: Vec<Vec<u64>> = original_leaves
        .iter()
        .map(|page| page.iter().map(|locator| locator.object_id).collect())
        .collect();

    let plan_operations: Vec<MixedPlanOperation> = operations
        .iter()
        .map(|operation| match operation {
            ImmutableBatchOperation::Put(input) => MixedPlanOperation::Put(input.object_id),
            ImmutableBatchOperation::Delete(object_id) => MixedPlanOperation::Delete(*object_id),
        })
        .collect();
    let path_local = plan_mixed_leaf_updates(
        &original_ids,
        &plan_operations,
        MixedLeafPlanLimits {
            capacity: LEAF_CAPACITY,
            minimum: LEAF_MIN_OCCUPANCY,
            max_objects: limits.max_objects,
            max_pages: limits.max_pages,
            max_actions: limits.max_pages.saturating_mul(8).max(operations.len()),
        },
    )?;

    let mut scratch = data.to_vec();
    let final_locators = apply_canonical_mixed_operations(
        &mut scratch,
        operations,
        &order,
        &previous,
        limits,
    )?;
    let canonical_sizes = canonical_group_sizes(
        final_locators.len(),
        LEAF_CAPACITY,
        LEAF_MIN_OCCUPANCY,
        limits,
    )?;
    let mut canonical_locator_pages = Vec::with_capacity(canonical_sizes.len());
    let mut canonical_id_pages = Vec::with_capacity(canonical_sizes.len());
    let mut start = 0_usize;
    for size in &canonical_sizes {
        let end = start
            .checked_add(*size)
            .ok_or(ImmutableError::Limit("object count"))?;
        let page = final_locators[start..end].to_vec();
        canonical_id_pages.push(page.iter().map(|locator| locator.object_id).collect());
        canonical_locator_pages.push(page);
        start = end;
    }
    let path_local_locator_pages: Vec<Vec<Locator>> = path_local
        .final_pages
        .iter()
        .map(|page| final_locator_page(&final_locators, page))
        .collect::<Result<_, _>>()?;

    let (path_local_leaf_pages_written, path_local_leaf_pages_reused) =
        exact_leaf_reuse_counts(&original_leaves, &path_local_locator_pages);
    let (canonical_leaf_pages_written, canonical_leaf_pages_reused) =
        exact_leaf_reuse_counts(&original_leaves, &canonical_locator_pages);
    let first_differing_leaf = first_leaf_difference(&path_local.final_pages, &canonical_id_pages);

    Ok(MixedRewriteComparisonReport {
        original_leaf_pages: original_leaves.len(),
        final_leaf_pages: canonical_locator_pages.len(),
        insertions: path_local.insertions,
        replacements: path_local.replacements,
        deletions: path_local.deletions,
        path_local_touched_original_leaf_pages: path_local.touched_original_pages.len(),
        path_local_leaf_pages_written,
        path_local_leaf_pages_reused,
        canonical_leaf_pages_written,
        canonical_leaf_pages_reused,
        extra_canonical_leaf_writes: canonical_leaf_pages_written
            .saturating_sub(path_local_leaf_pages_written),
        exact_leaf_layout_match: first_differing_leaf.is_none(),
        first_differing_leaf,
        path_local_final_leaf_counts: path_local.final_pages.iter().map(Vec::len).collect(),
        canonical_final_leaf_counts: canonical_sizes,
    })
}

#[cfg(test)]
mod mixed_rewrite_comparison_tests {
    use super::*;

    fn object(object_id: u64) -> ImmutableObjectInput {
        ImmutableObjectInput::new(
            object_id,
            u16::try_from(1 + object_id % 31).expect("kind"),
            vec![u8::try_from(object_id % 251).expect("payload"); 8],
        )
    }

    fn even_objects(count: usize) -> Vec<ImmutableObjectInput> {
        (1..=count)
            .map(|index| object(u64::try_from(index * 2).expect("object id")))
            .collect()
    }

    #[test]
    fn balanced_delete_insert_matches_canonical_layout() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(400), limits).expect("base");
        let report = compare_persistent_mixed_leaf_rewrites(
            &base,
            &[
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(1)),
            ],
            limits,
        )
        .expect("comparison");

        assert_eq!(report.original_leaf_pages, 3);
        assert_eq!(report.path_local_final_leaf_counts, vec![185, 122, 93]);
        assert_eq!(report.canonical_final_leaf_counts, vec![185, 122, 93]);
        assert!(report.exact_leaf_layout_match);
        assert_eq!(report.first_differing_leaf, None);
        assert_eq!(report.path_local_leaf_pages_written, 1);
        assert_eq!(report.canonical_leaf_pages_written, 1);
        assert_eq!(report.extra_canonical_leaf_writes, 0);
    }

    #[test]
    fn early_delete_exposes_one_extra_global_regrouping_write() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(400), limits).expect("base");
        let report = compare_persistent_mixed_leaf_rewrites(
            &base,
            &[
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(400)),
            ],
            limits,
        )
        .expect("comparison");

        assert_eq!(report.insertions, 0);
        assert_eq!(report.replacements, 1);
        assert_eq!(report.deletions, 1);
        assert_eq!(report.path_local_final_leaf_counts, vec![184, 122, 93]);
        assert_eq!(report.canonical_final_leaf_counts, vec![185, 121, 93]);
        assert!(!report.exact_leaf_layout_match);
        assert_eq!(report.first_differing_leaf, Some(0));
        assert_eq!(report.path_local_touched_original_leaf_pages, 2);
        assert_eq!(report.path_local_leaf_pages_written, 2);
        assert_eq!(report.canonical_leaf_pages_written, 2);
        assert_eq!(report.extra_canonical_leaf_writes, 0);
    }

    #[test]
    fn early_delete_with_untouched_replacement_quantifies_shift_cost() {
        let limits = ImmutableLimits::default();
        let base = build_genesis(&even_objects(400), limits).expect("base");
        let report = compare_persistent_mixed_leaf_rewrites(
            &base,
            &[
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(800)),
            ],
            limits,
        )
        .expect("comparison");

        assert_eq!(report.path_local_final_leaf_counts, vec![184, 122, 93]);
        assert_eq!(report.canonical_final_leaf_counts, vec![185, 121, 93]);
        assert_eq!(report.path_local_leaf_pages_written, 2);
        assert_eq!(report.canonical_leaf_pages_written, 3);
        assert_eq!(report.extra_canonical_leaf_writes, 1);
    }
}
