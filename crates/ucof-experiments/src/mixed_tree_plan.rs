use std::error::Error;
use std::fmt;

use crate::{
    plan_mixed_leaf_updates, MixedLeafPlan, MixedLeafPlanError, MixedLeafPlanLimits,
    MixedPlanOperation,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixedTreePlanLimits {
    pub leaf: MixedLeafPlanLimits,
    pub internal_fanout: usize,
    pub internal_minimum: usize,
    pub max_depth: usize,
    pub max_internal_pages: usize,
}

impl Default for MixedTreePlanLimits {
    fn default() -> Self {
        Self {
            leaf: MixedLeafPlanLimits::default(),
            internal_fanout: 84,
            internal_minimum: 42,
            max_depth: 8,
            max_internal_pages: 100_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixedRootTransition {
    Stable,
    Grew,
    Collapsed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedTreeShape {
    /// Page counts from leaves at index zero through the root at the final index.
    pub level_page_counts: Vec<usize>,
    /// Child counts for every internal level, ordered from the leaf parent through the root.
    pub internal_group_sizes: Vec<Vec<usize>>,
    pub root_level: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedTreePlan {
    pub leaf: MixedLeafPlan,
    pub original: MixedTreeShape,
    pub final_shape: MixedTreeShape,
    pub root_transition: MixedRootTransition,
    /// Conservative original internal-page indexes that cannot be reused, one vector per original
    /// internal level. A structural grouping change marks that level and every ancestor completely.
    pub conservative_touched_original_internal_pages: Vec<Vec<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixedTreePlanError {
    Leaf(MixedLeafPlanError),
    InvalidLimits,
    Limit(&'static str),
}

impl fmt::Display for MixedTreePlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Leaf(error) => write!(formatter, "{error}"),
            Self::InvalidLimits => write!(formatter, "invalid mixed tree plan limits"),
            Self::Limit(label) => write!(formatter, "mixed tree plan {label} limit exceeded"),
        }
    }
}

impl Error for MixedTreePlanError {}

impl From<MixedLeafPlanError> for MixedTreePlanError {
    fn from(error: MixedLeafPlanError) -> Self {
        Self::Leaf(error)
    }
}

fn canonical_group_sizes(
    total: usize,
    capacity: usize,
    minimum: usize,
    max_pages: usize,
) -> Result<Vec<usize>, MixedTreePlanError> {
    let groups = total
        .checked_add(capacity - 1)
        .ok_or(MixedTreePlanError::Limit("page count"))?
        / capacity;
    if groups == 0 || groups > max_pages {
        return Err(MixedTreePlanError::Limit("page count"));
    }
    if groups == 1 {
        return Ok(vec![total]);
    }

    let full_groups = total / capacity;
    let remainder = total % capacity;
    let mut sizes = Vec::with_capacity(groups);
    if remainder == 0 {
        sizes.resize(full_groups, capacity);
    } else if remainder >= minimum {
        sizes.resize(full_groups, capacity);
        sizes.push(remainder);
    } else {
        let prefix = full_groups
            .checked_sub(1)
            .ok_or(MixedTreePlanError::InvalidLimits)?;
        sizes.resize(prefix, capacity);
        let transfer = minimum - remainder;
        sizes.push(capacity - transfer);
        sizes.push(minimum);
    }
    if sizes.len() != groups
        || sizes.iter().sum::<usize>() != total
        || sizes.iter().any(|size| *size < minimum || *size > capacity)
    {
        return Err(MixedTreePlanError::InvalidLimits);
    }
    Ok(sizes)
}

fn build_shape(
    leaf_count: usize,
    limits: MixedTreePlanLimits,
) -> Result<MixedTreeShape, MixedTreePlanError> {
    if leaf_count == 0 {
        return Err(MixedTreePlanError::InvalidLimits);
    }
    let mut level_page_counts = vec![leaf_count];
    let mut internal_group_sizes = Vec::new();
    let mut current = leaf_count;
    let mut internal_pages = 0_usize;

    while current > 1 {
        if internal_group_sizes.len() >= limits.max_depth {
            return Err(MixedTreePlanError::Limit("depth"));
        }
        let sizes = if current <= limits.internal_fanout {
            vec![current]
        } else {
            canonical_group_sizes(
                current,
                limits.internal_fanout,
                limits.internal_minimum,
                limits.max_internal_pages,
            )?
        };
        internal_pages = internal_pages
            .checked_add(sizes.len())
            .ok_or(MixedTreePlanError::Limit("page count"))?;
        if internal_pages > limits.max_internal_pages {
            return Err(MixedTreePlanError::Limit("page count"));
        }
        current = sizes.len();
        level_page_counts.push(current);
        internal_group_sizes.push(sizes);
    }

    Ok(MixedTreeShape {
        root_level: internal_group_sizes.len(),
        level_page_counts,
        internal_group_sizes,
    })
}

fn parent_index(child_index: usize, groups: &[usize]) -> Result<usize, MixedTreePlanError> {
    let mut start = 0_usize;
    for (parent, count) in groups.iter().enumerate() {
        let end = start
            .checked_add(*count)
            .ok_or(MixedTreePlanError::Limit("page count"))?;
        if child_index < end {
            return Ok(parent);
        }
        start = end;
    }
    Err(MixedTreePlanError::InvalidLimits)
}

fn conservative_touched_internal_pages(
    original: &MixedTreeShape,
    final_shape: &MixedTreeShape,
    touched_leaves: &[usize],
) -> Result<Vec<Vec<usize>>, MixedTreePlanError> {
    let mut touched_children = touched_leaves.to_vec();
    touched_children.sort_unstable();
    touched_children.dedup();
    let mut result = Vec::with_capacity(original.root_level);
    let mut structural_change = original.level_page_counts[0] != final_shape.level_page_counts[0];

    for level in 0..original.root_level {
        let original_groups = &original.internal_group_sizes[level];
        let final_groups = final_shape.internal_group_sizes.get(level);
        structural_change |= final_groups != Some(original_groups);
        let touched_parents = if structural_change {
            (0..original_groups.len()).collect()
        } else {
            let mut parents = Vec::with_capacity(touched_children.len());
            for child in &touched_children {
                parents.push(parent_index(*child, original_groups)?);
            }
            parents.sort_unstable();
            parents.dedup();
            parents
        };
        touched_children = touched_parents.clone();
        result.push(touched_parents);
    }
    Ok(result)
}

/// Plans canonical tree shape and conservative original-ancestor rewrites for a simultaneous mixed
/// batch.
///
/// The leaf planner remains authoritative for identifier routing and sibling repair. This layer
/// groups the resulting leaves into canonical internal levels, records root growth or collapse, and
/// identifies original internal pages that cannot safely be claimed as reusable. It emits no bytes.
pub fn plan_mixed_tree_updates(
    original_leaves: &[Vec<u64>],
    operations: &[MixedPlanOperation],
    limits: MixedTreePlanLimits,
) -> Result<MixedTreePlan, MixedTreePlanError> {
    if limits.internal_fanout < 2
        || limits.internal_minimum == 0
        || limits.internal_minimum > limits.internal_fanout
        || limits.max_internal_pages == 0
    {
        return Err(MixedTreePlanError::InvalidLimits);
    }
    let leaf = plan_mixed_leaf_updates(original_leaves, operations, limits.leaf)?;
    let original = build_shape(original_leaves.len(), limits)?;
    let final_shape = build_shape(leaf.final_pages.len(), limits)?;
    let root_transition = match final_shape.root_level.cmp(&original.root_level) {
        std::cmp::Ordering::Greater => MixedRootTransition::Grew,
        std::cmp::Ordering::Less => MixedRootTransition::Collapsed,
        std::cmp::Ordering::Equal => MixedRootTransition::Stable,
    };
    let conservative_touched_original_internal_pages =
        conservative_touched_internal_pages(&original, &final_shape, &leaf.touched_original_pages)?;
    Ok(MixedTreePlan {
        leaf,
        original,
        final_shape,
        root_transition,
        conservative_touched_original_internal_pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> MixedTreePlanLimits {
        MixedTreePlanLimits {
            leaf: MixedLeafPlanLimits {
                capacity: 3,
                minimum: 2,
                ..MixedLeafPlanLimits::default()
            },
            internal_fanout: 3,
            internal_minimum: 2,
            max_depth: 8,
            max_internal_pages: 100,
        }
    }

    fn even_pages(page_count: usize, entries: usize) -> Vec<Vec<u64>> {
        let mut next = 2_u64;
        (0..page_count)
            .map(|_| {
                (0..entries)
                    .map(|_| {
                        let value = next;
                        next += 2;
                        value
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn stable_shape_rewrites_only_the_original_ancestor_path() {
        let plan =
            plan_mixed_tree_updates(&even_pages(4, 2), &[MixedPlanOperation::Put(2)], limits())
                .expect("stable plan");
        assert_eq!(plan.root_transition, MixedRootTransition::Stable);
        assert_eq!(plan.original.root_level, 2);
        assert_eq!(plan.final_shape.root_level, 2);
        assert_eq!(
            plan.conservative_touched_original_internal_pages,
            vec![vec![0], vec![0]]
        );
    }

    #[test]
    fn simultaneous_leaf_split_can_grow_the_root() {
        let plan =
            plan_mixed_tree_updates(&even_pages(9, 3), &[MixedPlanOperation::Put(1)], limits())
                .expect("growing plan");
        assert_eq!(plan.root_transition, MixedRootTransition::Grew);
        assert_eq!(plan.original.root_level, 2);
        assert_eq!(plan.final_shape.root_level, 3);
        assert_eq!(plan.final_shape.level_page_counts, vec![10, 4, 2, 1]);
        assert_eq!(
            plan.final_shape.internal_group_sizes,
            vec![vec![3, 3, 2, 2], vec![2, 2], vec![2]]
        );
        assert_eq!(
            plan.conservative_touched_original_internal_pages,
            vec![vec![0, 1, 2], vec![0]]
        );
    }

    #[test]
    fn simultaneous_leaf_merge_can_collapse_the_root() {
        let plan = plan_mixed_tree_updates(
            &even_pages(4, 2),
            &[MixedPlanOperation::Delete(2)],
            limits(),
        )
        .expect("collapsing plan");
        assert_eq!(plan.root_transition, MixedRootTransition::Collapsed);
        assert_eq!(plan.original.root_level, 2);
        assert_eq!(plan.final_shape.root_level, 1);
        assert_eq!(plan.final_shape.level_page_counts, vec![3, 1]);
        assert_eq!(
            plan.conservative_touched_original_internal_pages,
            vec![vec![0, 1], vec![0]]
        );
    }

    #[test]
    fn operation_order_does_not_change_recursive_shape() {
        let pages = even_pages(4, 2);
        let forward = [
            MixedPlanOperation::Delete(2),
            MixedPlanOperation::Put(1),
            MixedPlanOperation::Put(10),
        ];
        let mut reverse = forward;
        reverse.reverse();
        assert_eq!(
            plan_mixed_tree_updates(&pages, &forward, limits()).expect("forward"),
            plan_mixed_tree_updates(&pages, &reverse, limits()).expect("reverse")
        );
    }

    #[test]
    fn depth_limit_fails_before_unbounded_grouping() {
        let mut constrained = limits();
        constrained.max_depth = 2;
        assert_eq!(
            plan_mixed_tree_updates(
                &even_pages(10, 2),
                &[MixedPlanOperation::Put(2)],
                constrained,
            ),
            Err(MixedTreePlanError::Limit("depth"))
        );
    }
}
