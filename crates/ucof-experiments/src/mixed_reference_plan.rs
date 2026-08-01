use std::error::Error;
use std::fmt;

use crate::{
    plan_mixed_tree_updates, MixedPlanOperation, MixedTreePlan, MixedTreePlanError,
    MixedTreePlanLimits,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlannedPageIdentity {
    Original { level: usize, index: usize },
    New { level: usize, index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedReferencePlan {
    pub tree: MixedTreePlan,
    /// Final page identities from leaves at index zero through the root at the final index.
    pub final_level_identities: Vec<Vec<PlannedPageIdentity>>,
    /// Exactly reusable original page indexes at each original level, leaves first.
    pub reused_original_pages: Vec<Vec<usize>>,
    /// Newly emitted page counts at each final level, leaves first.
    pub new_pages_by_level: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixedReferencePlanError {
    Tree(MixedTreePlanError),
    InvalidShape,
    Limit(&'static str),
}

impl fmt::Display for MixedReferencePlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tree(error) => write!(formatter, "{error}"),
            Self::InvalidShape => write!(formatter, "invalid mixed reference plan shape"),
            Self::Limit(label) => write!(formatter, "mixed reference plan {label} limit exceeded"),
        }
    }
}

impl Error for MixedReferencePlanError {}

impl From<MixedTreePlanError> for MixedReferencePlanError {
    fn from(error: MixedTreePlanError) -> Self {
        Self::Tree(error)
    }
}

fn original_group_ranges(groups: &[usize]) -> Result<Vec<(usize, usize)>, MixedReferencePlanError> {
    let mut ranges = Vec::with_capacity(groups.len());
    let mut start = 0_usize;
    for count in groups {
        let end = start
            .checked_add(*count)
            .ok_or(MixedReferencePlanError::Limit("page count"))?;
        ranges.push((start, end));
        start = end;
    }
    Ok(ranges)
}

fn matching_original_group(
    child_identities: &[PlannedPageIdentity],
    original_groups: &[usize],
    child_level: usize,
) -> Result<Option<usize>, MixedReferencePlanError> {
    for (group_index, (start, end)) in original_group_ranges(original_groups)?
        .into_iter()
        .enumerate()
    {
        if child_identities.len() != end - start {
            continue;
        }
        let matches = child_identities.iter().enumerate().all(|(offset, identity)| {
            *identity
                == PlannedPageIdentity::Original {
                    level: child_level,
                    index: start + offset,
                }
        });
        if matches {
            return Ok(Some(group_index));
        }
    }
    Ok(None)
}

fn final_leaf_identities(
    original_leaves: &[Vec<u64>],
    tree: &MixedTreePlan,
) -> Vec<PlannedPageIdentity> {
    tree.leaf
        .final_pages
        .iter()
        .enumerate()
        .map(|(final_index, page)| {
            let original = original_leaves.iter().enumerate().find_map(|(index, candidate)| {
                if candidate == page && !tree.leaf.touched_original_pages.contains(&index) {
                    Some(index)
                } else {
                    None
                }
            });
            original.map_or(
                PlannedPageIdentity::New {
                    level: 0,
                    index: final_index,
                },
                |index| PlannedPageIdentity::Original { level: 0, index },
            )
        })
        .collect()
}

/// Derives exact original-page reuse from a simultaneous mixed tree plan.
///
/// A leaf is reusable only when its identifier sequence is byte-shape-equivalent to one untouched
/// original leaf. An internal page is reusable only when its complete ordered child identity
/// sequence exactly matches one original page at the same level. Any changed locator, split, merge,
/// shifted group boundary, or new child therefore forces a new page identity without relying on a
/// conservative whole-level approximation.
pub fn plan_mixed_page_references(
    original_leaves: &[Vec<u64>],
    operations: &[MixedPlanOperation],
    limits: MixedTreePlanLimits,
) -> Result<MixedReferencePlan, MixedReferencePlanError> {
    let tree = plan_mixed_tree_updates(original_leaves, operations, limits)?;
    let mut final_level_identities = vec![final_leaf_identities(original_leaves, &tree)];

    for level in 1..=tree.final_shape.root_level {
        let child_identities = final_level_identities
            .get(level - 1)
            .ok_or(MixedReferencePlanError::InvalidShape)?;
        let final_groups = tree
            .final_shape
            .internal_group_sizes
            .get(level - 1)
            .ok_or(MixedReferencePlanError::InvalidShape)?;
        let original_groups = tree.original.internal_group_sizes.get(level - 1);
        let ranges = original_group_ranges(final_groups)?;
        if ranges.last().map(|range| range.1) != Some(child_identities.len()) {
            return Err(MixedReferencePlanError::InvalidShape);
        }
        let mut identities = Vec::with_capacity(final_groups.len());
        for (final_index, (start, end)) in ranges.into_iter().enumerate() {
            let children = &child_identities[start..end];
            let original = match original_groups {
                Some(groups) => matching_original_group(children, groups, level - 1)?,
                None => None,
            };
            identities.push(original.map_or(
                PlannedPageIdentity::New {
                    level,
                    index: final_index,
                },
                |index| PlannedPageIdentity::Original { level, index },
            ));
        }
        final_level_identities.push(identities);
    }

    let mut reused_original_pages = vec![Vec::new(); tree.original.root_level + 1];
    let mut new_pages_by_level = Vec::with_capacity(final_level_identities.len());
    for (level, identities) in final_level_identities.iter().enumerate() {
        let mut new_pages = 0_usize;
        for identity in identities {
            match *identity {
                PlannedPageIdentity::Original {
                    level: original_level,
                    index,
                } => {
                    if original_level != level || level >= reused_original_pages.len() {
                        return Err(MixedReferencePlanError::InvalidShape);
                    }
                    reused_original_pages[level].push(index);
                }
                PlannedPageIdentity::New { level: new_level, .. } => {
                    if new_level != level {
                        return Err(MixedReferencePlanError::InvalidShape);
                    }
                    new_pages = new_pages
                        .checked_add(1)
                        .ok_or(MixedReferencePlanError::Limit("page count"))?;
                }
            }
        }
        reused_original_pages
            .get_mut(level)
            .into_iter()
            .for_each(|pages| {
                pages.sort_unstable();
                pages.dedup();
            });
        new_pages_by_level.push(new_pages);
    }

    Ok(MixedReferencePlan {
        tree,
        final_level_identities,
        reused_original_pages,
        new_pages_by_level,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MixedLeafPlanLimits;

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
    fn replacement_marks_one_leaf_and_ancestor_path_new() {
        let pages = even_pages(9, 2);
        let plan = plan_mixed_page_references(
            &pages,
            &[MixedPlanOperation::Put(2)],
            limits(),
        )
        .expect("reference plan");
        assert_eq!(plan.new_pages_by_level, vec![1, 1, 1]);
        assert_eq!(plan.reused_original_pages[0], (1..9).collect::<Vec<_>>());
        assert_eq!(plan.reused_original_pages[1], vec![1, 2]);
        assert!(plan.reused_original_pages[2].is_empty());
    }

    #[test]
    fn insertion_without_split_reuses_unaffected_internal_groups() {
        let pages = even_pages(9, 2);
        let plan = plan_mixed_page_references(
            &pages,
            &[MixedPlanOperation::Put(37)],
            limits(),
        )
        .expect("reference plan");
        assert_eq!(plan.new_pages_by_level, vec![1, 1, 1]);
        assert_eq!(plan.reused_original_pages[0], (0..8).collect::<Vec<_>>());
        assert_eq!(plan.reused_original_pages[1], vec![0, 1]);
        assert!(plan.reused_original_pages[2].is_empty());
    }

    #[test]
    fn split_that_shifts_group_boundaries_reuses_only_untouched_leaves() {
        let pages = even_pages(9, 3);
        let plan = plan_mixed_page_references(
            &pages,
            &[MixedPlanOperation::Put(1)],
            limits(),
        )
        .expect("reference plan");
        assert_eq!(plan.tree.final_shape.root_level, 3);
        assert_eq!(plan.reused_original_pages[0], (1..9).collect::<Vec<_>>());
        assert!(plan.reused_original_pages[1].is_empty());
        assert!(plan.reused_original_pages[2].is_empty());
        assert_eq!(plan.new_pages_by_level, vec![2, 4, 2, 1]);
    }

    #[test]
    fn merge_collapse_emits_a_new_root_when_the_child_sequence_changes() {
        let pages = even_pages(4, 2);
        let plan = plan_mixed_page_references(
            &pages,
            &[MixedPlanOperation::Delete(8)],
            limits(),
        )
        .expect("reference plan");
        assert_eq!(plan.tree.final_shape.root_level, 1);
        assert_eq!(plan.final_level_identities[1].len(), 1);
        assert!(matches!(
            plan.final_level_identities[1][0],
            PlannedPageIdentity::New { level: 1, .. }
        ));
        assert_eq!(plan.new_pages_by_level, vec![1, 1]);
    }

    #[test]
    fn caller_order_does_not_change_exact_reference_reuse() {
        let pages = even_pages(9, 2);
        let forward = [
            MixedPlanOperation::Delete(4),
            MixedPlanOperation::Put(3),
            MixedPlanOperation::Put(18),
        ];
        let mut reverse = forward;
        reverse.reverse();
        assert_eq!(
            plan_mixed_page_references(&pages, &forward, limits()).expect("forward"),
            plan_mixed_page_references(&pages, &reverse, limits()).expect("reverse")
        );
    }
}
