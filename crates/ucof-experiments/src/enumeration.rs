use crate::{CandidateStatus, CheckpointKind, SnapshotCandidate, SnapshotIdentity};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumerationLimits {
    pub max_candidates: usize,
    pub max_parent_depth: usize,
    pub max_results: usize,
}

impl Default for EnumerationLimits {
    fn default() -> Self {
        Self {
            max_candidates: 1024,
            max_parent_depth: 256,
            max_results: 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumeratedRootStatus {
    VerifiedTerminal,
    VerifiedAncestor,
    VerifiedForkTerminal,
    ProgressCheckpoint,
    IntegrityFailed,
    UnsupportedRequiredCapability,
    Truncated,
    Invalid,
    MissingParent,
    ParentNotVerified,
    NonIncreasingSequence,
    SequenceGap,
    ParentCycle,
    ParentDepthExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumeratedRoot {
    pub identity: SnapshotIdentity,
    pub sequence: u64,
    pub footer_offset: u64,
    pub exact_end: bool,
    pub status: EnumeratedRootStatus,
    pub chain_depth: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootEnumerationReport {
    /// Results ordered from newest physical footer to oldest.
    pub roots: Vec<EnumeratedRoot>,
    pub candidates_considered: usize,
    pub valid_complete_count: usize,
    pub highest_valid_sequence: Option<u64>,
    pub has_ambiguous_highest_fork: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootEnumerationError {
    CandidateLimitExceeded,
    ResultLimitExceeded,
    DuplicateIdentity(SnapshotIdentity),
}

impl fmt::Display for RootEnumerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CandidateLimitExceeded => write!(f, "root-enumeration candidate limit exceeded"),
            Self::ResultLimitExceeded => write!(f, "root-enumeration result limit exceeded"),
            Self::DuplicateIdentity(identity) => {
                write!(f, "duplicate snapshot identity {identity}")
            }
        }
    }
}

impl std::error::Error for RootEnumerationError {}

impl RootEnumerationReport {
    pub fn enumerate(
        candidates: &[SnapshotCandidate],
        limits: EnumerationLimits,
    ) -> Result<Self, RootEnumerationError> {
        if candidates.len() > limits.max_candidates {
            return Err(RootEnumerationError::CandidateLimitExceeded);
        }
        if candidates.len() > limits.max_results {
            return Err(RootEnumerationError::ResultLimitExceeded);
        }

        let mut by_identity = BTreeMap::new();
        for candidate in candidates {
            if by_identity.insert(candidate.identity, *candidate).is_some() {
                return Err(RootEnumerationError::DuplicateIdentity(candidate.identity));
            }
        }

        let mut valid_depths = BTreeMap::new();
        let mut statuses = BTreeMap::new();
        for candidate in candidates {
            match classify_chain(*candidate, &by_identity, limits.max_parent_depth) {
                Ok(depth) => {
                    valid_depths.insert(candidate.identity, depth);
                    statuses.insert(candidate.identity, EnumeratedRootStatus::VerifiedTerminal);
                }
                Err(status) => {
                    statuses.insert(candidate.identity, status);
                }
            }
        }

        let valid_ids: BTreeSet<_> = valid_depths.keys().copied().collect();
        let parent_ids: BTreeSet<_> = valid_ids
            .iter()
            .filter_map(|identity| by_identity[identity].parent)
            .filter(|parent| valid_ids.contains(parent))
            .collect();
        for parent in &parent_ids {
            statuses.insert(*parent, EnumeratedRootStatus::VerifiedAncestor);
        }

        let terminals: Vec<_> = valid_ids
            .iter()
            .filter(|identity| !parent_ids.contains(identity))
            .copied()
            .collect();
        let highest_valid_sequence = terminals
            .iter()
            .map(|identity| by_identity[identity].sequence)
            .max();
        let highest: Vec<_> = terminals
            .iter()
            .filter(|identity| Some(by_identity[identity].sequence) == highest_valid_sequence)
            .copied()
            .collect();
        let has_ambiguous_highest_fork = highest.len() > 1;
        if has_ambiguous_highest_fork {
            for identity in highest {
                statuses.insert(identity, EnumeratedRootStatus::VerifiedForkTerminal);
            }
        }

        let mut roots: Vec<_> = candidates
            .iter()
            .map(|candidate| EnumeratedRoot {
                identity: candidate.identity,
                sequence: candidate.sequence,
                footer_offset: candidate.footer_offset,
                exact_end: candidate.exact_end,
                status: statuses[&candidate.identity],
                chain_depth: valid_depths.get(&candidate.identity).copied(),
            })
            .collect();
        roots.sort_unstable_by(|left, right| {
            right
                .footer_offset
                .cmp(&left.footer_offset)
                .then_with(|| right.sequence.cmp(&left.sequence))
                .then_with(|| left.identity.cmp(&right.identity))
        });

        Ok(Self {
            roots,
            candidates_considered: candidates.len(),
            valid_complete_count: valid_depths.len(),
            highest_valid_sequence,
            has_ambiguous_highest_fork,
        })
    }
}

fn classify_chain(
    start: SnapshotCandidate,
    candidates: &BTreeMap<SnapshotIdentity, SnapshotCandidate>,
    max_depth: usize,
) -> Result<usize, EnumeratedRootStatus> {
    match start.status {
        CandidateStatus::Verified => {}
        CandidateStatus::IntegrityFailed => return Err(EnumeratedRootStatus::IntegrityFailed),
        CandidateStatus::UnsupportedRequiredCapability => {
            return Err(EnumeratedRootStatus::UnsupportedRequiredCapability)
        }
        CandidateStatus::Truncated => return Err(EnumeratedRootStatus::Truncated),
        CandidateStatus::Invalid => return Err(EnumeratedRootStatus::Invalid),
    }
    if matches!(start.checkpoint, CheckpointKind::Progress) {
        return Err(EnumeratedRootStatus::ProgressCheckpoint);
    }

    let mut visited = BTreeSet::new();
    let mut current = start;
    let mut depth = 1_usize;
    loop {
        if depth > max_depth {
            return Err(EnumeratedRootStatus::ParentDepthExceeded);
        }
        if !visited.insert(current.identity) {
            return Err(EnumeratedRootStatus::ParentCycle);
        }
        let Some(parent_id) = current.parent else {
            return Ok(depth);
        };
        let parent = candidates
            .get(&parent_id)
            .copied()
            .ok_or(EnumeratedRootStatus::MissingParent)?;
        if !matches!(parent.status, CandidateStatus::Verified)
            || matches!(parent.checkpoint, CheckpointKind::Progress)
        {
            return Err(EnumeratedRootStatus::ParentNotVerified);
        }
        if current.sequence <= parent.sequence {
            return Err(EnumeratedRootStatus::NonIncreasingSequence);
        }
        if current.sequence != parent.sequence.saturating_add(1) {
            return Err(EnumeratedRootStatus::SequenceGap);
        }
        current = parent;
        depth = depth
            .checked_add(1)
            .ok_or(EnumeratedRootStatus::ParentDepthExceeded)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(label: &str) -> SnapshotIdentity {
        SnapshotIdentity::derive(label.as_bytes())
    }

    fn candidate(
        label: &str,
        sequence: u64,
        parent: Option<&str>,
        offset: u64,
    ) -> SnapshotCandidate {
        SnapshotCandidate {
            identity: id(label),
            sequence,
            parent: parent.map(id),
            footer_offset: offset,
            exact_end: false,
            checkpoint: CheckpointKind::Complete,
            status: CandidateStatus::Verified,
        }
    }

    fn status(report: &RootEnumerationReport, label: &str) -> EnumeratedRootStatus {
        report
            .roots
            .iter()
            .find(|root| root.identity == id(label))
            .expect("enumerated root")
            .status
    }

    #[test]
    fn enumeration_distinguishes_ancestors_terminals_and_invalid_candidates() {
        let mut invalid = candidate("invalid", 9, Some("missing"), 9000);
        invalid.status = CandidateStatus::IntegrityFailed;
        let candidates = [
            candidate("genesis", 0, None, 1000),
            candidate("second", 1, Some("genesis"), 2000),
            invalid,
        ];
        let report = RootEnumerationReport::enumerate(&candidates, EnumerationLimits::default())
            .expect("enumeration");
        assert_eq!(
            status(&report, "genesis"),
            EnumeratedRootStatus::VerifiedAncestor
        );
        assert_eq!(
            status(&report, "second"),
            EnumeratedRootStatus::VerifiedTerminal
        );
        assert_eq!(
            status(&report, "invalid"),
            EnumeratedRootStatus::IntegrityFailed
        );
        assert_eq!(report.valid_complete_count, 2);
        assert_eq!(report.highest_valid_sequence, Some(1));
        assert!(!report.has_ambiguous_highest_fork);
    }

    #[test]
    fn highest_equal_fork_is_reported_without_selection() {
        let candidates = [
            candidate("genesis", 0, None, 1000),
            candidate("left", 1, Some("genesis"), 2000),
            candidate("right", 1, Some("genesis"), 3000),
        ];
        let report = RootEnumerationReport::enumerate(&candidates, EnumerationLimits::default())
            .expect("enumeration");
        assert!(report.has_ambiguous_highest_fork);
        assert_eq!(
            status(&report, "left"),
            EnumeratedRootStatus::VerifiedForkTerminal
        );
        assert_eq!(
            status(&report, "right"),
            EnumeratedRootStatus::VerifiedForkTerminal
        );
    }

    #[test]
    fn progress_missing_parent_and_sequence_gap_have_distinct_statuses() {
        let genesis = candidate("genesis", 0, None, 1000);
        let mut progress = candidate("progress", 1, Some("genesis"), 2000);
        progress.checkpoint = CheckpointKind::Progress;
        let missing = candidate("missing", 4, Some("absent"), 3000);
        let gap = candidate("gap", 3, Some("genesis"), 4000);
        let report = RootEnumerationReport::enumerate(
            &[genesis, progress, missing, gap],
            EnumerationLimits::default(),
        )
        .expect("enumeration");
        assert_eq!(
            status(&report, "progress"),
            EnumeratedRootStatus::ProgressCheckpoint
        );
        assert_eq!(
            status(&report, "missing"),
            EnumeratedRootStatus::MissingParent
        );
        assert_eq!(status(&report, "gap"), EnumeratedRootStatus::SequenceGap);
    }

    #[test]
    fn results_are_ordered_by_physical_recency() {
        let report = RootEnumerationReport::enumerate(
            &[
                candidate("old", 0, None, 100),
                candidate("new", 1, Some("old"), 900),
            ],
            EnumerationLimits::default(),
        )
        .expect("enumeration");
        assert_eq!(report.roots[0].identity, id("new"));
        assert_eq!(report.roots[1].identity, id("old"));
    }

    #[test]
    fn candidate_and_result_limits_apply_before_enumeration_growth() {
        let candidates = [
            candidate("one", 0, None, 100),
            candidate("two", 0, None, 200),
        ];
        assert_eq!(
            RootEnumerationReport::enumerate(
                &candidates,
                EnumerationLimits {
                    max_candidates: 1,
                    ..EnumerationLimits::default()
                }
            ),
            Err(RootEnumerationError::CandidateLimitExceeded)
        );
        assert_eq!(
            RootEnumerationReport::enumerate(
                &candidates,
                EnumerationLimits {
                    max_results: 1,
                    ..EnumerationLimits::default()
                }
            ),
            Err(RootEnumerationError::ResultLimitExceeded)
        );
    }
}
