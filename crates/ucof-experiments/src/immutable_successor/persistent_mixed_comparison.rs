use crate::{
    plan_mixed_tree_updates, MixedLeafPlanLimits, MixedPlanOperation, MixedRootTransition,
    MixedTreePlanError, MixedTreePlanLimits,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentMixedRewriteComparisonError {
    Format(ImmutableError),
    Planner(MixedTreePlanError),
}

impl std::fmt::Display for PersistentMixedRewriteComparisonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "persistent mixed comparison failed: {error}"),
            Self::Planner(error) => write!(formatter, "persistent mixed planner failed: {error}"),
        }
    }
}

impl std::error::Error for PersistentMixedRewriteComparisonError {}

impl From<ImmutableError> for PersistentMixedRewriteComparisonError {
    fn from(error: ImmutableError) -> Self {
        Self::Format(error)
    }
}

impl From<MixedTreePlanError> for PersistentMixedRewriteComparisonError {
    fn from(error: MixedTreePlanError) -> Self {
        Self::Planner(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersistentMixedRewriteRelation {
    Equal,
    CanonicalWritesMore(usize),
    CanonicalWritesFewer(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistentMixedRewriteComparison {
    pub root_transition: MixedRootTransition,
    pub original_leaf_sizes: Vec<usize>,
    pub planner_final_leaf_sizes: Vec<usize>,
    pub canonical_final_leaf_sizes: Vec<usize>,
    pub leaf_partition_equal: bool,
    pub first_differing_leaf: Option<usize>,
    pub planner_touched_original_leaves: usize,
    pub planner_conservative_touched_original_internal_pages: usize,
    pub planner_estimated_pages_written: usize,
    pub canonical_pages_written: usize,
    pub canonical_pages_reused: usize,
    pub planner_exact_leaf_pages_written: usize,
    pub planner_exact_leaf_pages_reused: usize,
    pub canonical_exact_leaf_pages_written: usize,
    pub canonical_exact_leaf_pages_reused: usize,
    pub extra_canonical_leaf_writes: usize,
    /// Present only when the path-local and canonical writers choose identical final leaf bodies.
    pub comparable_relation: Option<PersistentMixedRewriteRelation>,
}

fn mixed_leaf_locator_pages(
    data: &[u8],
    limits: ImmutableLimits,
) -> Result<Vec<Vec<Locator>>, ImmutableError> {
    let report = validate_canonical_internal(data, limits)?;
    let footer = parse_footer(data, report.footer_offset)?;
    let snapshot_offset = usize_from_u64(footer.snapshot_offset, "snapshot range")?;
    let snapshot = checked_range(data, snapshot_offset, SNAPSHOT_LEN, "snapshot")?;
    let root = root_reference(data, snapshot, limits)?;
    let mut levels = vec![Vec::new(); usize::from(root.level) + 1];
    let mut visited = 0_usize;
    collect_original_mixed_pages(data, &root, &mut levels, limits, &mut visited)?;
    if visited != report.public.page_count {
        return Err(ImmutableError::Invalid("mixed comparison page inventory"));
    }

    let leaves = levels
        .first_mut()
        .ok_or(ImmutableError::Invalid("mixed comparison leaves"))?;
    leaves.sort_unstable_by_key(|page| page.reference.minimum);
    allocation_check::<Vec<Locator>>(leaves.len(), limits)?;
    let mut result = Vec::with_capacity(leaves.len());
    for page in leaves {
        let entries = match &page.body {
            OriginalMixedPageBody::Leaf(entries) => entries,
            OriginalMixedPageBody::Internal(_) => {
                return Err(ImmutableError::Invalid("mixed comparison leaf body"));
            }
        };
        result.push(entries.clone());
    }
    Ok(result)
}

fn mixed_leaf_id_pages(locator_pages: &[Vec<Locator>]) -> Vec<Vec<u64>> {
    locator_pages
        .iter()
        .map(|entries| entries.iter().map(|locator| locator.object_id).collect())
        .collect()
}

fn comparison_planner_limits(limits: ImmutableLimits) -> MixedTreePlanLimits {
    MixedTreePlanLimits {
        leaf: MixedLeafPlanLimits {
            capacity: LEAF_CAPACITY,
            minimum: LEAF_MIN_OCCUPANCY,
            max_objects: limits.max_objects,
            max_pages: limits.max_pages,
            max_actions: limits.max_objects.saturating_add(limits.max_pages),
        },
        internal_fanout: INTERNAL_FANOUT,
        internal_minimum: INTERNAL_MIN_OCCUPANCY,
        max_depth: usize::from(limits.max_depth),
        max_internal_pages: limits.max_pages,
    }
}

fn comparison_operations(operations: &[ImmutableBatchOperation]) -> Vec<MixedPlanOperation> {
    operations
        .iter()
        .map(|operation| match operation {
            ImmutableBatchOperation::Put(input) => MixedPlanOperation::Put(input.object_id),
            ImmutableBatchOperation::Delete(object_id) => MixedPlanOperation::Delete(*object_id),
        })
        .collect()
}

fn planner_estimated_writes(
    original_leaf_count: usize,
    final_leaf_count: usize,
    touched_original_leaves: usize,
    original_level_counts: &[usize],
    final_level_counts: &[usize],
    touched_original_internal: &[Vec<usize>],
) -> Result<usize, ImmutableError> {
    let reusable_leaves = original_leaf_count
        .saturating_sub(touched_original_leaves)
        .min(final_leaf_count);
    let mut writes = final_leaf_count
        .checked_sub(reusable_leaves)
        .ok_or(ImmutableError::Invalid("mixed comparison leaf count"))?;

    for (final_level, final_count) in final_level_counts.iter().copied().enumerate().skip(1) {
        let original_count = original_level_counts
            .get(final_level)
            .copied()
            .unwrap_or(0);
        let touched = touched_original_internal
            .get(final_level - 1)
            .map_or(original_count, Vec::len)
            .min(original_count);
        let reusable = original_count.saturating_sub(touched).min(final_count);
        writes = writes
            .checked_add(final_count.saturating_sub(reusable))
            .ok_or(ImmutableError::Limit("page count"))?;
    }
    Ok(writes)
}

fn rewrite_relation(canonical: usize, planner: usize) -> PersistentMixedRewriteRelation {
    match canonical.cmp(&planner) {
        std::cmp::Ordering::Equal => PersistentMixedRewriteRelation::Equal,
        std::cmp::Ordering::Greater => {
            PersistentMixedRewriteRelation::CanonicalWritesMore(canonical - planner)
        }
        std::cmp::Ordering::Less => {
            PersistentMixedRewriteRelation::CanonicalWritesFewer(planner - canonical)
        }
    }
}

fn final_locator_pages_for_plan(
    final_locator_pages: &[Vec<Locator>],
    planned_id_pages: &[Vec<u64>],
) -> Result<Vec<Vec<Locator>>, ImmutableError> {
    let final_locators: Vec<&Locator> = final_locator_pages.iter().flatten().collect();
    planned_id_pages
        .iter()
        .map(|page| {
            page.iter()
                .map(|object_id| {
                    final_locators
                        .binary_search_by_key(object_id, |locator| locator.object_id)
                        .map(|index| final_locators[index].clone())
                        .map_err(|_| ImmutableError::MissingObject(*object_id))
                })
                .collect()
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

/// Compares authenticated canonical mixed-page writes with the path-local repair planner.
///
/// Complete-tree rewrite estimates are directly comparable only when both paths choose the same
/// final leaf bodies. Exact locator-body leaf reuse is also measured under divergent legal layouts,
/// allowing the cost of the current global canonical grouping rule to be quantified without treating
/// path-local output as canonical bytes. A positive `extra_canonical_leaf_writes` value is therefore
/// policy-cost evidence, not by itself a correctness defect.
pub fn compare_persistent_mixed_rewrites(
    data: &[u8],
    operations: &[ImmutableBatchOperation],
    limits: ImmutableLimits,
) -> Result<PersistentMixedRewriteComparison, PersistentMixedRewriteComparisonError> {
    let original_locator_pages = mixed_leaf_locator_pages(data, limits)?;
    let original_leaves = mixed_leaf_id_pages(&original_locator_pages);
    let plan = plan_mixed_tree_updates(
        &original_leaves,
        &comparison_operations(operations),
        comparison_planner_limits(limits),
    )?;
    let written = append_persistent_mixed_batch(data, operations, limits)?;
    let canonical_final_locator_pages = mixed_leaf_locator_pages(&written.bytes, limits)?;
    let canonical_final_leaves = mixed_leaf_id_pages(&canonical_final_locator_pages);
    let planner_final_locator_pages =
        final_locator_pages_for_plan(&canonical_final_locator_pages, &plan.leaf.final_pages)?;
    let leaf_partition_equal = plan.leaf.final_pages == canonical_final_leaves;
    let first_differing_leaf = first_leaf_difference(&plan.leaf.final_pages, &canonical_final_leaves);
    let planner_estimated_pages_written = planner_estimated_writes(
        original_leaves.len(),
        plan.leaf.final_pages.len(),
        plan.leaf.touched_original_pages.len(),
        &plan.original.level_page_counts,
        &plan.final_shape.level_page_counts,
        &plan.conservative_touched_original_internal_pages,
    )?;
    let comparable_relation = leaf_partition_equal
        .then(|| rewrite_relation(written.pages_written, planner_estimated_pages_written));
    let (planner_exact_leaf_pages_written, planner_exact_leaf_pages_reused) =
        exact_leaf_reuse_counts(&original_locator_pages, &planner_final_locator_pages);
    let (canonical_exact_leaf_pages_written, canonical_exact_leaf_pages_reused) =
        exact_leaf_reuse_counts(&original_locator_pages, &canonical_final_locator_pages);

    Ok(PersistentMixedRewriteComparison {
        root_transition: plan.root_transition,
        original_leaf_sizes: original_leaves.iter().map(Vec::len).collect(),
        planner_final_leaf_sizes: plan.leaf.final_pages.iter().map(Vec::len).collect(),
        canonical_final_leaf_sizes: canonical_final_leaves.iter().map(Vec::len).collect(),
        leaf_partition_equal,
        first_differing_leaf,
        planner_touched_original_leaves: plan.leaf.touched_original_pages.len(),
        planner_conservative_touched_original_internal_pages: plan
            .conservative_touched_original_internal_pages
            .iter()
            .map(Vec::len)
            .sum(),
        planner_estimated_pages_written,
        canonical_pages_written: written.pages_written,
        canonical_pages_reused: written.pages_reused,
        planner_exact_leaf_pages_written,
        planner_exact_leaf_pages_reused,
        canonical_exact_leaf_pages_written,
        canonical_exact_leaf_pages_reused,
        extra_canonical_leaf_writes: canonical_exact_leaf_pages_written
            .saturating_sub(planner_exact_leaf_pages_written),
        comparable_relation,
    })
}

#[cfg(test)]
mod persistent_mixed_comparison_tests {
    use super::*;

    fn object(object_id: u64, seed: u8, payload: &[u8]) -> ImmutableObjectInput {
        ImmutableObjectInput::new(object_id, 1 + u16::from(seed % 31), payload.to_vec())
    }

    fn strided_objects(count: usize) -> Vec<ImmutableObjectInput> {
        (1..=count)
            .map(|index| {
                let object_id = u64::try_from(index * 2).expect("small object id");
                object(
                    object_id,
                    u8::try_from(index % 251).expect("seed"),
                    &[u8::try_from(index % 251).expect("payload")],
                )
            })
            .collect()
    }

    fn compare(
        count: usize,
        operations: Vec<ImmutableBatchOperation>,
    ) -> PersistentMixedRewriteComparison {
        let limits = ImmutableLimits {
            max_file_bytes: 64 * 1024 * 1024,
            max_output_bytes: 64 * 1024 * 1024,
            ..ImmutableLimits::default()
        };
        let base = build_genesis(&strided_objects(count), limits).expect("base");
        compare_persistent_mixed_rewrites(&base, &operations, limits).expect("comparison")
    }

    #[test]
    fn stable_transition_matches_path_local_rewrite_count() {
        let comparison = compare(
            400,
            vec![
                ImmutableBatchOperation::Delete(700),
                ImmutableBatchOperation::Put(object(701, 78, b"inserted-701")),
                ImmutableBatchOperation::Put(object(702, 77, b"replacement-702")),
            ],
        );
        assert_eq!(comparison.root_transition, MixedRootTransition::Stable);
        assert!(comparison.leaf_partition_equal);
        assert_eq!(comparison.first_differing_leaf, None);
        assert_eq!(comparison.original_leaf_sizes, vec![185, 122, 93]);
        assert_eq!(comparison.canonical_final_leaf_sizes, vec![185, 122, 93]);
        assert_eq!(comparison.canonical_pages_written, 2);
        assert_eq!(comparison.canonical_pages_reused, 2);
        assert_eq!(comparison.planner_estimated_pages_written, 2);
        assert_eq!(comparison.extra_canonical_leaf_writes, 0);
        assert_eq!(
            comparison.comparable_relation,
            Some(PersistentMixedRewriteRelation::Equal)
        );
    }

    #[test]
    fn root_collapse_matches_path_local_rewrite_count() {
        let comparison = compare(
            186,
            vec![
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(4, 91, b"replacement-four")),
            ],
        );
        assert_eq!(comparison.root_transition, MixedRootTransition::Collapsed);
        assert!(comparison.leaf_partition_equal);
        assert_eq!(comparison.canonical_final_leaf_sizes, vec![185]);
        assert_eq!(comparison.canonical_pages_written, 1);
        assert_eq!(comparison.canonical_pages_reused, 0);
        assert_eq!(comparison.planner_estimated_pages_written, 1);
        assert_eq!(comparison.extra_canonical_leaf_writes, 0);
        assert_eq!(
            comparison.comparable_relation,
            Some(PersistentMixedRewriteRelation::Equal)
        );
    }

    #[test]
    fn root_growth_matches_path_local_rewrite_count() {
        let comparison = compare(
            185,
            vec![
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(1, 11, b"inserted-one")),
                ImmutableBatchOperation::Put(object(371, 12, b"inserted-371")),
            ],
        );
        assert_eq!(comparison.root_transition, MixedRootTransition::Grew);
        assert!(comparison.leaf_partition_equal);
        assert_eq!(comparison.canonical_final_leaf_sizes, vec![93, 93]);
        assert_eq!(comparison.canonical_pages_written, 3);
        assert_eq!(comparison.canonical_pages_reused, 0);
        assert_eq!(comparison.planner_estimated_pages_written, 3);
        assert_eq!(comparison.extra_canonical_leaf_writes, 0);
        assert_eq!(
            comparison.comparable_relation,
            Some(PersistentMixedRewriteRelation::Equal)
        );
    }

    #[test]
    fn divergent_partition_quantifies_one_extra_canonical_leaf_write() {
        let comparison = compare(
            400,
            vec![
                ImmutableBatchOperation::Delete(2),
                ImmutableBatchOperation::Put(object(800, 99, b"replacement-eight-hundred")),
            ],
        );
        assert_eq!(comparison.root_transition, MixedRootTransition::Stable);
        assert!(!comparison.leaf_partition_equal);
        assert_eq!(comparison.first_differing_leaf, Some(0));
        assert_eq!(comparison.original_leaf_sizes, vec![185, 122, 93]);
        assert_eq!(comparison.planner_final_leaf_sizes, vec![184, 122, 93]);
        assert_eq!(comparison.canonical_final_leaf_sizes, vec![185, 121, 93]);
        assert_eq!(comparison.planner_exact_leaf_pages_written, 2);
        assert_eq!(comparison.planner_exact_leaf_pages_reused, 1);
        assert_eq!(comparison.canonical_exact_leaf_pages_written, 3);
        assert_eq!(comparison.canonical_exact_leaf_pages_reused, 0);
        assert_eq!(comparison.extra_canonical_leaf_writes, 1);
        assert_eq!(comparison.comparable_relation, None);
    }
}
