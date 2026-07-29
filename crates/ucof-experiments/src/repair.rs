use crate::{
    CheckpointKind, CompactionError, CompactionLimits, ObjectGraph, ObjectLocator,
    SnapshotCandidate,
};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairLimits {
    pub compaction: CompactionLimits,
    pub max_copy_ranges: usize,
    pub max_total_copy_bytes: u64,
}

impl Default for RepairLimits {
    fn default() -> Self {
        Self {
            compaction: CompactionLimits::default(),
            max_copy_ranges: 1_000_000,
            max_total_copy_bytes: u64::MAX,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyRange {
    pub object_id: u64,
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairPlan {
    pub source_snapshot: crate::SnapshotIdentity,
    pub selected_roots: Vec<u64>,
    pub copy_ranges: Vec<CopyRange>,
    pub orphaned_object_ids: Vec<u64>,
    pub total_copy_bytes: u64,
    pub requires_new_snapshot_identity: bool,
    pub preserves_byte_scoped_signatures: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairError {
    SourceNotVerified,
    ProgressCheckpoint,
    DuplicateLocator(u64),
    MissingLocator(u64),
    RangeOverflow(u64),
    OverlappingRanges(u64, u64),
    CopyRangeLimitExceeded,
    CopyByteLimitExceeded,
    Compaction(CompactionError),
}

impl fmt::Display for RepairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotVerified => write!(f, "repair source snapshot is not verified"),
            Self::ProgressCheckpoint => write!(f, "progress checkpoint cannot be repaired as a snapshot"),
            Self::DuplicateLocator(id) => write!(f, "duplicate locator for object {id}"),
            Self::MissingLocator(id) => write!(f, "missing locator for reachable object {id}"),
            Self::RangeOverflow(id) => write!(f, "physical range overflow for object {id}"),
            Self::OverlappingRanges(left, right) => {
                write!(f, "physical ranges overlap for objects {left} and {right}")
            }
            Self::CopyRangeLimitExceeded => write!(f, "repair copy-range limit exceeded"),
            Self::CopyByteLimitExceeded => write!(f, "repair copy-byte limit exceeded"),
            Self::Compaction(error) => write!(f, "repair reachability failure: {error}"),
        }
    }
}

impl std::error::Error for RepairError {}

impl From<CompactionError> for RepairError {
    fn from(value: CompactionError) -> Self {
        Self::Compaction(value)
    }
}

impl RepairPlan {
    pub fn build(
        snapshot: SnapshotCandidate,
        selected_roots: &[u64],
        graph: &ObjectGraph,
        locators: impl IntoIterator<Item = ObjectLocator>,
        limits: RepairLimits,
    ) -> Result<Self, RepairError> {
        if !matches!(snapshot.status, crate::CandidateStatus::Verified) {
            return Err(RepairError::SourceNotVerified);
        }
        if matches!(snapshot.checkpoint, CheckpointKind::Progress) {
            return Err(RepairError::ProgressCheckpoint);
        }

        let reachability = graph.plan(selected_roots, limits.compaction)?;
        if reachability.reachable.len() > limits.max_copy_ranges {
            return Err(RepairError::CopyRangeLimitExceeded);
        }

        let mut by_id = BTreeMap::new();
        for locator in locators {
            if by_id.insert(locator.object_id, locator).is_some() {
                return Err(RepairError::DuplicateLocator(locator.object_id));
            }
        }

        let mut copy_ranges = Vec::with_capacity(reachability.reachable.len());
        let mut total_copy_bytes = 0_u64;
        for object_id in &reachability.reachable {
            let locator = by_id
                .get(object_id)
                .copied()
                .ok_or(RepairError::MissingLocator(*object_id))?;
            locator
                .offset
                .checked_add(locator.stored_len)
                .ok_or(RepairError::RangeOverflow(*object_id))?;
            total_copy_bytes = total_copy_bytes
                .checked_add(locator.stored_len)
                .ok_or(RepairError::CopyByteLimitExceeded)?;
            if total_copy_bytes > limits.max_total_copy_bytes {
                return Err(RepairError::CopyByteLimitExceeded);
            }
            copy_ranges.push(CopyRange {
                object_id: *object_id,
                offset: locator.offset,
                length: locator.stored_len,
            });
        }

        copy_ranges.sort_unstable_by_key(|range| (range.offset, range.object_id));
        for pair in copy_ranges.windows(2) {
            let left_end = pair[0]
                .offset
                .checked_add(pair[0].length)
                .ok_or(RepairError::RangeOverflow(pair[0].object_id))?;
            if left_end > pair[1].offset {
                return Err(RepairError::OverlappingRanges(
                    pair[0].object_id,
                    pair[1].object_id,
                ));
            }
        }

        Ok(Self {
            source_snapshot: snapshot.identity,
            selected_roots: selected_roots.to_vec(),
            copy_ranges,
            orphaned_object_ids: reachability.orphaned,
            total_copy_bytes,
            requires_new_snapshot_identity: true,
            preserves_byte_scoped_signatures: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CandidateStatus, SnapshotIdentity};

    fn snapshot(status: CandidateStatus, checkpoint: CheckpointKind) -> SnapshotCandidate {
        SnapshotCandidate {
            identity: SnapshotIdentity::derive(b"selected"),
            sequence: 4,
            parent: None,
            footer_offset: 4096,
            exact_end: false,
            checkpoint,
            status,
        }
    }

    fn graph() -> ObjectGraph {
        let mut graph = ObjectGraph::new();
        graph.add_object(1, vec![2]).expect("object 1");
        graph.add_object(2, Vec::new()).expect("object 2");
        graph.add_object(3, Vec::new()).expect("orphan 3");
        graph
    }

    fn locator(id: u64, offset: u64, length: u64) -> ObjectLocator {
        ObjectLocator {
            object_id: id,
            kind: 1,
            offset,
            stored_len: length,
            logical_len: length,
        }
    }

    #[test]
    fn verified_source_produces_sorted_non_overlapping_copy_plan() {
        let plan = RepairPlan::build(
            snapshot(CandidateStatus::Verified, CheckpointKind::Complete),
            &[1],
            &graph(),
            vec![locator(2, 100, 20), locator(1, 20, 30), locator(3, 500, 10)],
            RepairLimits::default(),
        )
        .expect("repair plan");

        assert_eq!(
            plan.copy_ranges,
            vec![
                CopyRange {
                    object_id: 1,
                    offset: 20,
                    length: 30,
                },
                CopyRange {
                    object_id: 2,
                    offset: 100,
                    length: 20,
                },
            ]
        );
        assert_eq!(plan.orphaned_object_ids, vec![3]);
        assert_eq!(plan.total_copy_bytes, 50);
        assert!(plan.requires_new_snapshot_identity);
        assert!(!plan.preserves_byte_scoped_signatures);
    }

    #[test]
    fn invalid_or_progress_source_is_never_upgraded() {
        assert_eq!(
            RepairPlan::build(
                snapshot(CandidateStatus::IntegrityFailed, CheckpointKind::Complete),
                &[1],
                &graph(),
                vec![locator(1, 0, 1), locator(2, 2, 1)],
                RepairLimits::default(),
            ),
            Err(RepairError::SourceNotVerified)
        );
        assert_eq!(
            RepairPlan::build(
                snapshot(CandidateStatus::Verified, CheckpointKind::Progress),
                &[1],
                &graph(),
                vec![locator(1, 0, 1), locator(2, 2, 1)],
                RepairLimits::default(),
            ),
            Err(RepairError::ProgressCheckpoint)
        );
    }

    #[test]
    fn missing_or_overlapping_locator_fails_closed() {
        assert_eq!(
            RepairPlan::build(
                snapshot(CandidateStatus::Verified, CheckpointKind::Complete),
                &[1],
                &graph(),
                vec![locator(1, 0, 10)],
                RepairLimits::default(),
            ),
            Err(RepairError::MissingLocator(2))
        );

        assert_eq!(
            RepairPlan::build(
                snapshot(CandidateStatus::Verified, CheckpointKind::Complete),
                &[1],
                &graph(),
                vec![locator(1, 0, 10), locator(2, 5, 10)],
                RepairLimits::default(),
            ),
            Err(RepairError::OverlappingRanges(1, 2))
        );
    }

    #[test]
    fn copy_limits_apply_after_reachability_and_before_output() {
        let error = RepairPlan::build(
            snapshot(CandidateStatus::Verified, CheckpointKind::Complete),
            &[1],
            &graph(),
            vec![locator(1, 0, 10), locator(2, 20, 10)],
            RepairLimits {
                max_total_copy_bytes: 15,
                ..RepairLimits::default()
            },
        )
        .expect_err("copy byte limit");
        assert_eq!(error, RepairError::CopyByteLimitExceeded);
    }
}
