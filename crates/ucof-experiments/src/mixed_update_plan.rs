use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

/// Identifier-only operation used to model a complete mixed persistent batch before byte emission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixedPlanOperation {
    /// Insert an absent identifier or replace an existing identifier without changing occupancy.
    Put(u64),
    /// Delete an existing identifier.
    Delete(u64),
}

impl MixedPlanOperation {
    fn object_id(self) -> u64 {
        match self {
            Self::Put(object_id) | Self::Delete(object_id) => object_id,
        }
    }
}

/// Limits for the identifier-level leaf repair model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixedLeafPlanLimits {
    pub capacity: usize,
    pub minimum: usize,
    pub max_objects: usize,
    pub max_pages: usize,
    pub max_actions: usize,
}

impl Default for MixedLeafPlanLimits {
    fn default() -> Self {
        Self {
            capacity: 185,
            minimum: 93,
            max_objects: 1_000_000,
            max_pages: 100_000,
            max_actions: 100_000,
        }
    }
}

/// Byte-significant repair decision after every batch operation has been applied simultaneously.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixedLeafRepairAction {
    Apply {
        original_page: usize,
        insertions: usize,
        replacements: usize,
        deletions: usize,
    },
    Split {
        position: usize,
        output_counts: Vec<usize>,
    },
    BorrowFromLeft {
        target_position: usize,
    },
    BorrowFromRight {
        target_position: usize,
    },
    MergeWithLeft {
        target_position: usize,
    },
    MergeWithRight {
        target_position: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedLeafPlan {
    /// Final ordered leaf contents after simultaneous application and deterministic repair.
    pub final_pages: Vec<Vec<u64>>,
    /// Original pages whose contents or references must be rewritten.
    pub touched_original_pages: Vec<usize>,
    pub actions: Vec<MixedLeafRepairAction>,
    pub insertions: usize,
    pub replacements: usize,
    pub deletions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixedLeafPlanError {
    InvalidLimits,
    InvalidPage,
    DuplicateObject(u64),
    DuplicateOperation(u64),
    MissingObject(u64),
    EmptyResult,
    Limit(&'static str),
}

impl fmt::Display for MixedLeafPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => write!(formatter, "invalid mixed leaf plan limits"),
            Self::InvalidPage => write!(formatter, "invalid mixed leaf page input"),
            Self::DuplicateObject(object_id) => {
                write!(formatter, "duplicate mixed leaf object {object_id}")
            }
            Self::DuplicateOperation(object_id) => {
                write!(formatter, "duplicate mixed leaf operation {object_id}")
            }
            Self::MissingObject(object_id) => {
                write!(formatter, "missing mixed leaf object {object_id}")
            }
            Self::EmptyResult => write!(formatter, "mixed leaf plan would delete every object"),
            Self::Limit(label) => write!(formatter, "mixed leaf plan {label} limit exceeded"),
        }
    }
}

impl Error for MixedLeafPlanError {}

#[derive(Clone, Debug)]
struct WorkingPage {
    object_ids: Vec<u64>,
    origins: BTreeSet<usize>,
}

fn checked_action(
    actions: &mut Vec<MixedLeafRepairAction>,
    action: MixedLeafRepairAction,
    limits: MixedLeafPlanLimits,
) -> Result<(), MixedLeafPlanError> {
    if actions.len() >= limits.max_actions {
        return Err(MixedLeafPlanError::Limit("action count"));
    }
    actions.push(action);
    Ok(())
}

fn canonical_group_sizes(
    total: usize,
    limits: MixedLeafPlanLimits,
) -> Result<Vec<usize>, MixedLeafPlanError> {
    if total == 0 {
        return Err(MixedLeafPlanError::InvalidPage);
    }
    let groups = total
        .checked_add(limits.capacity - 1)
        .ok_or(MixedLeafPlanError::Limit("page count"))?
        / limits.capacity;
    if groups == 1 {
        return Ok(vec![total]);
    }
    if groups > limits.max_pages {
        return Err(MixedLeafPlanError::Limit("page count"));
    }

    let full_groups = total / limits.capacity;
    let remainder = total % limits.capacity;
    let mut sizes = Vec::with_capacity(groups);
    if remainder == 0 {
        sizes.resize(full_groups, limits.capacity);
    } else if remainder >= limits.minimum {
        sizes.resize(full_groups, limits.capacity);
        sizes.push(remainder);
    } else {
        let prefix = full_groups
            .checked_sub(1)
            .ok_or(MixedLeafPlanError::InvalidPage)?;
        sizes.resize(prefix, limits.capacity);
        let transfer = limits.minimum - remainder;
        sizes.push(limits.capacity - transfer);
        sizes.push(limits.minimum);
    }
    if sizes.len() != groups
        || sizes.iter().sum::<usize>() != total
        || sizes
            .iter()
            .any(|size| *size < limits.minimum || *size > limits.capacity)
    {
        return Err(MixedLeafPlanError::InvalidPage);
    }
    Ok(sizes)
}

fn validate_input_pages(
    pages: &[Vec<u64>],
    limits: MixedLeafPlanLimits,
) -> Result<BTreeMap<u64, usize>, MixedLeafPlanError> {
    if limits.capacity == 0
        || limits.minimum == 0
        || limits.minimum > limits.capacity
        || limits.max_objects == 0
        || limits.max_pages == 0
        || limits.max_actions == 0
    {
        return Err(MixedLeafPlanError::InvalidLimits);
    }
    if pages.is_empty() || pages.len() > limits.max_pages {
        return Err(MixedLeafPlanError::InvalidPage);
    }

    let mut locations = BTreeMap::new();
    let mut total = 0_usize;
    let mut previous_max = None;
    for (page_index, page) in pages.iter().enumerate() {
        if page.is_empty()
            || page.len() > limits.capacity
            || (pages.len() > 1 && page.len() < limits.minimum)
            || page.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(MixedLeafPlanError::InvalidPage);
        }
        if previous_max.is_some_and(|maximum| maximum >= page[0]) {
            return Err(MixedLeafPlanError::InvalidPage);
        }
        previous_max = page.last().copied();
        total = total
            .checked_add(page.len())
            .ok_or(MixedLeafPlanError::Limit("object count"))?;
        if total > limits.max_objects {
            return Err(MixedLeafPlanError::Limit("object count"));
        }
        for object_id in page {
            if *object_id == 0 {
                return Err(MixedLeafPlanError::InvalidPage);
            }
            if locations.insert(*object_id, page_index).is_some() {
                return Err(MixedLeafPlanError::DuplicateObject(*object_id));
            }
        }
    }
    Ok(locations)
}

fn validate_final_pages(
    pages: &[WorkingPage],
    limits: MixedLeafPlanLimits,
) -> Result<(), MixedLeafPlanError> {
    if pages.is_empty() || pages.len() > limits.max_pages {
        return Err(MixedLeafPlanError::InvalidPage);
    }
    let mut total = 0_usize;
    let mut previous_max = None;
    for page in pages {
        if page.object_ids.is_empty()
            || page.object_ids.len() > limits.capacity
            || (pages.len() > 1 && page.object_ids.len() < limits.minimum)
            || page.object_ids.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(MixedLeafPlanError::InvalidPage);
        }
        if previous_max.is_some_and(|maximum| maximum >= page.object_ids[0]) {
            return Err(MixedLeafPlanError::InvalidPage);
        }
        previous_max = page.object_ids.last().copied();
        total = total
            .checked_add(page.object_ids.len())
            .ok_or(MixedLeafPlanError::Limit("object count"))?;
    }
    if total == 0 || total > limits.max_objects {
        return Err(MixedLeafPlanError::Limit("object count"));
    }
    Ok(())
}

/// Plans leaf-level occupancy repair for a complete mixed batch.
///
/// Every operation is routed against the original authenticated leaf ranges. All operations are
/// then applied simultaneously, overflows are split left-to-right using canonical grouping, and
/// underflows are repaired left-to-right using left borrow, right borrow, left merge, then right
/// merge. Caller order therefore cannot create intermediate-tree-dependent results.
pub fn plan_mixed_leaf_updates(
    pages: &[Vec<u64>],
    operations: &[MixedPlanOperation],
    limits: MixedLeafPlanLimits,
) -> Result<MixedLeafPlan, MixedLeafPlanError> {
    let locations = validate_input_pages(pages, limits)?;
    if operations.is_empty() {
        return Err(MixedLeafPlanError::InvalidPage);
    }
    if operations.len() > limits.max_objects {
        return Err(MixedLeafPlanError::Limit("operation count"));
    }

    let mut ordered = operations.to_vec();
    ordered.sort_unstable_by_key(|operation| operation.object_id());
    if let Some(pair) = ordered
        .windows(2)
        .find(|pair| pair[0].object_id() == pair[1].object_id())
    {
        return Err(MixedLeafPlanError::DuplicateOperation(
            pair[0].object_id(),
        ));
    }

    let original_maxima: Vec<u64> = pages
        .iter()
        .map(|page| *page.last().expect("validated non-empty page"))
        .collect();
    let mut by_page: Vec<Vec<MixedPlanOperation>> = vec![Vec::new(); pages.len()];
    let mut insertions = 0_usize;
    let mut replacements = 0_usize;
    let mut deletions = 0_usize;

    for operation in ordered {
        let object_id = operation.object_id();
        if object_id == 0 {
            return Err(MixedLeafPlanError::InvalidPage);
        }
        let page_index = if let Some(index) = locations.get(&object_id) {
            *index
        } else {
            match operation {
                MixedPlanOperation::Delete(_) => {
                    return Err(MixedLeafPlanError::MissingObject(object_id));
                }
                MixedPlanOperation::Put(_) => original_maxima
                    .iter()
                    .position(|maximum| object_id <= *maximum)
                    .unwrap_or(pages.len() - 1),
            }
        };
        match operation {
            MixedPlanOperation::Put(_) if locations.contains_key(&object_id) => {
                replacements += 1;
            }
            MixedPlanOperation::Put(_) => insertions += 1,
            MixedPlanOperation::Delete(_) => deletions += 1,
        }
        by_page[page_index].push(operation);
    }

    let projected_objects = locations
        .len()
        .checked_add(insertions)
        .and_then(|value| value.checked_sub(deletions))
        .ok_or(MixedLeafPlanError::EmptyResult)?;
    if projected_objects == 0 {
        return Err(MixedLeafPlanError::EmptyResult);
    }
    if projected_objects > limits.max_objects {
        return Err(MixedLeafPlanError::Limit("object count"));
    }

    let mut actions = Vec::new();
    let mut touched = BTreeSet::new();
    let mut working = Vec::with_capacity(pages.len());
    for (page_index, page) in pages.iter().enumerate() {
        let mut values: BTreeSet<u64> = page.iter().copied().collect();
        let mut page_insertions = 0_usize;
        let mut page_replacements = 0_usize;
        let mut page_deletions = 0_usize;
        for operation in &by_page[page_index] {
            match *operation {
                MixedPlanOperation::Put(object_id) if values.contains(&object_id) => {
                    page_replacements += 1;
                }
                MixedPlanOperation::Put(object_id) => {
                    values.insert(object_id);
                    page_insertions += 1;
                }
                MixedPlanOperation::Delete(object_id) => {
                    if !values.remove(&object_id) {
                        return Err(MixedLeafPlanError::MissingObject(object_id));
                    }
                    page_deletions += 1;
                }
            }
        }
        if !by_page[page_index].is_empty() {
            touched.insert(page_index);
            checked_action(
                &mut actions,
                MixedLeafRepairAction::Apply {
                    original_page: page_index,
                    insertions: page_insertions,
                    replacements: page_replacements,
                    deletions: page_deletions,
                },
                limits,
            )?;
        }
        let mut origins = BTreeSet::new();
        origins.insert(page_index);
        working.push(WorkingPage {
            object_ids: values.into_iter().collect(),
            origins,
        });
    }

    let mut position = 0_usize;
    while position < working.len() {
        if working[position].object_ids.len() <= limits.capacity {
            position += 1;
            continue;
        }
        let sizes = canonical_group_sizes(working[position].object_ids.len(), limits)?;
        let object_ids = std::mem::take(&mut working[position].object_ids);
        let origins = working[position].origins.clone();
        for origin in &origins {
            touched.insert(*origin);
        }
        let mut replacements = Vec::with_capacity(sizes.len());
        let mut start = 0_usize;
        for size in &sizes {
            let end = start + *size;
            replacements.push(WorkingPage {
                object_ids: object_ids[start..end].to_vec(),
                origins: origins.clone(),
            });
            start = end;
        }
        checked_action(
            &mut actions,
            MixedLeafRepairAction::Split {
                position,
                output_counts: sizes,
            },
            limits,
        )?;
        working.splice(position..=position, replacements);
        if working.len() > limits.max_pages {
            return Err(MixedLeafPlanError::Limit("page count"));
        }
        position += 1;
    }

    position = 0;
    while working.len() > 1 && position < working.len() {
        if working[position].object_ids.len() >= limits.minimum {
            position += 1;
            continue;
        }

        if position > 0 && working[position - 1].object_ids.len() > limits.minimum {
            let borrowed = working[position - 1]
                .object_ids
                .pop()
                .ok_or(MixedLeafPlanError::InvalidPage)?;
            working[position].object_ids.insert(0, borrowed);
            for origin in working[position - 1]
                .origins
                .iter()
                .chain(working[position].origins.iter())
            {
                touched.insert(*origin);
            }
            checked_action(
                &mut actions,
                MixedLeafRepairAction::BorrowFromLeft {
                    target_position: position,
                },
                limits,
            )?;
            position += 1;
            continue;
        }

        if position + 1 < working.len()
            && working[position + 1].object_ids.len() > limits.minimum
        {
            let borrowed = working[position + 1].object_ids.remove(0);
            working[position].object_ids.push(borrowed);
            for origin in working[position + 1]
                .origins
                .iter()
                .chain(working[position].origins.iter())
            {
                touched.insert(*origin);
            }
            checked_action(
                &mut actions,
                MixedLeafRepairAction::BorrowFromRight {
                    target_position: position,
                },
                limits,
            )?;
            position += 1;
            continue;
        }

        if position > 0 {
            let right = working.remove(position);
            if working[position - 1].object_ids.len() + right.object_ids.len() > limits.capacity {
                return Err(MixedLeafPlanError::InvalidPage);
            }
            for origin in &right.origins {
                touched.insert(*origin);
            }
            for origin in &working[position - 1].origins {
                touched.insert(*origin);
            }
            working[position - 1].object_ids.extend(right.object_ids);
            working[position - 1].origins.extend(right.origins);
            checked_action(
                &mut actions,
                MixedLeafRepairAction::MergeWithLeft {
                    target_position: position,
                },
                limits,
            )?;
            position = position.saturating_sub(1);
        } else if working.len() > 1 {
            let right = working.remove(1);
            if working[0].object_ids.len() + right.object_ids.len() > limits.capacity {
                return Err(MixedLeafPlanError::InvalidPage);
            }
            for origin in &right.origins {
                touched.insert(*origin);
            }
            for origin in &working[0].origins {
                touched.insert(*origin);
            }
            working[0].object_ids.extend(right.object_ids);
            working[0].origins.extend(right.origins);
            checked_action(
                &mut actions,
                MixedLeafRepairAction::MergeWithRight { target_position: 0 },
                limits,
            )?;
        }
    }

    validate_final_pages(&working, limits)?;
    Ok(MixedLeafPlan {
        final_pages: working.into_iter().map(|page| page.object_ids).collect(),
        touched_original_pages: touched.into_iter().collect(),
        actions,
        insertions,
        replacements,
        deletions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(counts: &[usize]) -> Vec<Vec<u64>> {
        let mut next = 1_u64;
        counts
            .iter()
            .map(|count| {
                let page: Vec<_> = (0..*count)
                    .map(|_| {
                        let value = next;
                        next += 1;
                        value
                    })
                    .collect();
                page
            })
            .collect()
    }

    #[test]
    fn same_page_delete_and_insert_avoid_intermediate_underflow() {
        let limits = MixedLeafPlanLimits {
            capacity: 5,
            minimum: 3,
            ..MixedLeafPlanLimits::default()
        };
        let input = pages(&[3, 3]);
        let plan = plan_mixed_leaf_updates(
            &input,
            &[MixedPlanOperation::Delete(1), MixedPlanOperation::Put(0 + 7)],
            limits,
        )
        .expect("mixed plan");
        assert_eq!(plan.final_pages.iter().map(Vec::len).collect::<Vec<_>>(), vec![3, 3]);
        assert!(!plan.actions.iter().any(|action| matches!(
            action,
            MixedLeafRepairAction::BorrowFromLeft { .. }
                | MixedLeafRepairAction::BorrowFromRight { .. }
                | MixedLeafRepairAction::MergeWithLeft { .. }
                | MixedLeafRepairAction::MergeWithRight { .. }
        )));
    }

    #[test]
    fn batch_order_does_not_change_repair() {
        let limits = MixedLeafPlanLimits {
            capacity: 5,
            minimum: 3,
            ..MixedLeafPlanLimits::default()
        };
        let input = pages(&[4, 3, 3]);
        let forward = [
            MixedPlanOperation::Delete(7),
            MixedPlanOperation::Put(11),
            MixedPlanOperation::Put(2),
        ];
        let mut reverse = forward;
        reverse.reverse();
        assert_eq!(
            plan_mixed_leaf_updates(&input, &forward, limits).expect("forward"),
            plan_mixed_leaf_updates(&input, &reverse, limits).expect("reverse")
        );
    }

    #[test]
    fn full_batch_can_enable_right_borrow() {
        let limits = MixedLeafPlanLimits {
            capacity: 5,
            minimum: 3,
            ..MixedLeafPlanLimits::default()
        };
        let input = pages(&[3, 3]);
        let plan = plan_mixed_leaf_updates(
            &input,
            &[MixedPlanOperation::Delete(1), MixedPlanOperation::Put(7)],
            limits,
        )
        .expect("right borrow");
        assert!(plan.actions.iter().any(|action| matches!(
            action,
            MixedLeafRepairAction::BorrowFromRight { target_position: 0 }
        )));
        assert_eq!(plan.final_pages.iter().map(Vec::len).collect::<Vec<_>>(), vec![3, 3]);
    }

    #[test]
    fn simultaneous_overflow_and_underflow_are_repaired_deterministically() {
        let limits = MixedLeafPlanLimits {
            capacity: 5,
            minimum: 3,
            ..MixedLeafPlanLimits::default()
        };
        let input = pages(&[5, 3]);
        let plan = plan_mixed_leaf_updates(
            &input,
            &[
                MixedPlanOperation::Put(9),
                MixedPlanOperation::Put(10),
                MixedPlanOperation::Delete(6),
                MixedPlanOperation::Delete(7),
            ],
            limits,
        )
        .expect("split and repair");
        assert_eq!(plan.final_pages.iter().flatten().count(), 8);
        assert!(plan.actions.iter().any(|action| matches!(
            action,
            MixedLeafRepairAction::Split { .. }
        )));
    }

    #[test]
    fn replacement_is_counted_without_occupancy_change() {
        let limits = MixedLeafPlanLimits {
            capacity: 5,
            minimum: 3,
            ..MixedLeafPlanLimits::default()
        };
        let input = pages(&[3, 3]);
        let plan = plan_mixed_leaf_updates(
            &input,
            &[MixedPlanOperation::Put(2), MixedPlanOperation::Delete(6)],
            limits,
        )
        .expect("replace and delete");
        assert_eq!(plan.replacements, 1);
        assert_eq!(plan.deletions, 1);
        assert_eq!(plan.insertions, 0);
    }

    #[test]
    fn duplicate_missing_and_empty_results_fail_closed() {
        let limits = MixedLeafPlanLimits {
            capacity: 5,
            minimum: 3,
            ..MixedLeafPlanLimits::default()
        };
        let input = pages(&[3]);
        assert_eq!(
            plan_mixed_leaf_updates(
                &input,
                &[MixedPlanOperation::Put(2), MixedPlanOperation::Delete(2)],
                limits,
            ),
            Err(MixedLeafPlanError::DuplicateOperation(2))
        );
        assert_eq!(
            plan_mixed_leaf_updates(&input, &[MixedPlanOperation::Delete(9)], limits),
            Err(MixedLeafPlanError::MissingObject(9))
        );
        assert_eq!(
            plan_mixed_leaf_updates(
                &input,
                &[
                    MixedPlanOperation::Delete(1),
                    MixedPlanOperation::Delete(2),
                    MixedPlanOperation::Delete(3),
                ],
                limits,
            ),
            Err(MixedLeafPlanError::EmptyResult)
        );
    }
}
