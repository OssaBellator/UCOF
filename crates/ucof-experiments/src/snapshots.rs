use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Experimental snapshot identity used by the non-normative selection model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnapshotIdentity(pub [u8; 32]);

impl SnapshotIdentity {
    #[must_use]
    pub fn derive(label: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"UCOF snapshot model\0");
        hasher.update(label);
        Self(hasher.finalize().into())
    }
}

impl fmt::Display for SnapshotIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0[..8] {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointKind {
    Complete,
    Progress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateStatus {
    Verified,
    IntegrityFailed,
    UnsupportedRequiredCapability,
    Truncated,
    Invalid,
}

/// One footer/snapshot candidate discovered under a separate bounded policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotCandidate {
    pub identity: SnapshotIdentity,
    pub sequence: u64,
    pub parent: Option<SnapshotIdentity>,
    pub footer_offset: u64,
    pub exact_end: bool,
    pub checkpoint: CheckpointKind,
    pub status: CandidateStatus,
}

impl SnapshotCandidate {
    #[must_use]
    pub const fn is_verified_complete(self) -> bool {
        matches!(self.status, CandidateStatus::Verified)
            && matches!(self.checkpoint, CheckpointKind::Complete)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSelectionMode {
    StrictExactEnd,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootSelectionLimits {
    pub max_candidates: usize,
    pub max_parent_depth: usize,
}

impl Default for RootSelectionLimits {
    fn default() -> Self {
        Self {
            max_candidates: 1024,
            max_parent_depth: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootRejection {
    NotVerified,
    ProgressCheckpoint,
    MissingParent,
    ParentNotVerified,
    NonIncreasingSequence,
    SequenceGap,
    ParentCycle,
    ParentDepthExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedCandidate {
    pub identity: SnapshotIdentity,
    pub reason: RootRejection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSelectionReport {
    pub mode: RootSelectionMode,
    pub selected: SnapshotIdentity,
    pub selected_sequence: u64,
    /// Genesis-to-selected authenticated parent chain.
    pub chain: Vec<SnapshotIdentity>,
    pub rejected: Vec<RejectedCandidate>,
    pub candidates_considered: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSelectionError {
    CandidateLimitExceeded,
    DuplicateIdentity(SnapshotIdentity),
    NoExactEndRoot,
    MultipleExactEndRoots,
    NoValidRoot,
    AmbiguousFork,
}

impl fmt::Display for RootSelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CandidateLimitExceeded => write!(f, "snapshot candidate limit exceeded"),
            Self::DuplicateIdentity(identity) => {
                write!(f, "duplicate snapshot identity {identity}")
            }
            Self::NoExactEndRoot => write!(f, "no verified exact-end snapshot"),
            Self::MultipleExactEndRoots => write!(f, "multiple verified exact-end snapshots"),
            Self::NoValidRoot => write!(f, "no valid complete snapshot chain"),
            Self::AmbiguousFork => write!(f, "ambiguous highest snapshot fork"),
        }
    }
}

impl std::error::Error for RootSelectionError {}

impl RootSelectionReport {
    pub fn select(
        candidates: &[SnapshotCandidate],
        mode: RootSelectionMode,
        limits: RootSelectionLimits,
    ) -> Result<Self, RootSelectionError> {
        if candidates.len() > limits.max_candidates {
            return Err(RootSelectionError::CandidateLimitExceeded);
        }

        let mut by_identity = BTreeMap::new();
        for candidate in candidates {
            if by_identity.insert(candidate.identity, *candidate).is_some() {
                return Err(RootSelectionError::DuplicateIdentity(candidate.identity));
            }
        }

        let mut valid_chains: BTreeMap<SnapshotIdentity, Vec<SnapshotIdentity>> = BTreeMap::new();
        let mut rejected = Vec::new();
        for candidate in candidates {
            match evaluate_chain(*candidate, &by_identity, limits.max_parent_depth) {
                Ok(chain) => {
                    valid_chains.insert(candidate.identity, chain);
                }
                Err(reason) => rejected.push(RejectedCandidate {
                    identity: candidate.identity,
                    reason,
                }),
            }
        }

        let selected = match mode {
            RootSelectionMode::StrictExactEnd => {
                let exact: Vec<_> = candidates
                    .iter()
                    .filter(|candidate| {
                        candidate.exact_end && valid_chains.contains_key(&candidate.identity)
                    })
                    .collect();
                match exact.as_slice() {
                    [] => return Err(RootSelectionError::NoExactEndRoot),
                    [candidate] => **candidate,
                    _ => return Err(RootSelectionError::MultipleExactEndRoots),
                }
            }
            RootSelectionMode::Recovery => {
                let valid_ids: BTreeSet<_> = valid_chains.keys().copied().collect();
                if valid_ids.is_empty() {
                    return Err(RootSelectionError::NoValidRoot);
                }
                let parent_ids: BTreeSet<_> = valid_ids
                    .iter()
                    .filter_map(|identity| by_identity[identity].parent)
                    .filter(|parent| valid_ids.contains(parent))
                    .collect();
                let terminals: Vec<_> = valid_ids
                    .iter()
                    .filter(|identity| !parent_ids.contains(identity))
                    .map(|identity| by_identity[identity])
                    .collect();
                let maximum = terminals
                    .iter()
                    .map(|candidate| candidate.sequence)
                    .max()
                    .ok_or(RootSelectionError::NoValidRoot)?;
                let highest: Vec<_> = terminals
                    .into_iter()
                    .filter(|candidate| candidate.sequence == maximum)
                    .collect();
                match highest.as_slice() {
                    [candidate] => *candidate,
                    _ => return Err(RootSelectionError::AmbiguousFork),
                }
            }
        };

        let chain = valid_chains
            .remove(&selected.identity)
            .ok_or(RootSelectionError::NoValidRoot)?;
        Ok(Self {
            mode,
            selected: selected.identity,
            selected_sequence: selected.sequence,
            chain,
            rejected,
            candidates_considered: candidates.len(),
        })
    }
}

fn evaluate_chain(
    start: SnapshotCandidate,
    candidates: &BTreeMap<SnapshotIdentity, SnapshotCandidate>,
    max_depth: usize,
) -> Result<Vec<SnapshotIdentity>, RootRejection> {
    if !matches!(start.status, CandidateStatus::Verified) {
        return Err(RootRejection::NotVerified);
    }
    if matches!(start.checkpoint, CheckpointKind::Progress) {
        return Err(RootRejection::ProgressCheckpoint);
    }

    let mut reverse = Vec::new();
    let mut visited = BTreeSet::new();
    let mut current = start;
    loop {
        if reverse.len() >= max_depth {
            return Err(RootRejection::ParentDepthExceeded);
        }
        if !visited.insert(current.identity) {
            return Err(RootRejection::ParentCycle);
        }
        reverse.push(current.identity);

        let Some(parent_identity) = current.parent else {
            break;
        };
        let parent = candidates
            .get(&parent_identity)
            .copied()
            .ok_or(RootRejection::MissingParent)?;
        if !matches!(parent.status, CandidateStatus::Verified) {
            return Err(RootRejection::ParentNotVerified);
        }
        if matches!(parent.checkpoint, CheckpointKind::Progress) {
            return Err(RootRejection::ProgressCheckpoint);
        }
        if current.sequence <= parent.sequence {
            return Err(RootRejection::NonIncreasingSequence);
        }
        if current.sequence != parent.sequence.saturating_add(1) {
            return Err(RootRejection::SequenceGap);
        }
        current = parent;
    }
    reverse.reverse();
    Ok(reverse)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(label: &str) -> SnapshotIdentity {
        SnapshotIdentity::derive(label.as_bytes())
    }

    fn candidate(
        label: &str,
        sequence: u64,
        parent: Option<&str>,
        exact_end: bool,
    ) -> SnapshotCandidate {
        SnapshotCandidate {
            identity: identity(label),
            sequence,
            parent: parent.map(identity),
            footer_offset: sequence * 1000,
            exact_end,
            checkpoint: CheckpointKind::Complete,
            status: CandidateStatus::Verified,
        }
    }

    #[test]
    fn strict_mode_accepts_only_verified_exact_end_root() {
        let candidates = [
            candidate("genesis", 0, None, false),
            candidate("second", 1, Some("genesis"), true),
        ];
        let report = RootSelectionReport::select(
            &candidates,
            RootSelectionMode::StrictExactEnd,
            RootSelectionLimits::default(),
        )
        .expect("strict root");
        assert_eq!(report.selected, identity("second"));
        assert_eq!(report.chain, vec![identity("genesis"), identity("second")]);
    }

    #[test]
    fn interrupted_append_is_rejected_strictly_but_recovers_old_root() {
        let mut incomplete = candidate("third", 2, Some("second"), false);
        incomplete.status = CandidateStatus::Truncated;
        let candidates = [
            candidate("genesis", 0, None, false),
            candidate("second", 1, Some("genesis"), false),
            incomplete,
        ];

        let strict = RootSelectionReport::select(
            &candidates,
            RootSelectionMode::StrictExactEnd,
            RootSelectionLimits::default(),
        );
        assert_eq!(strict, Err(RootSelectionError::NoExactEndRoot));

        let recovery = RootSelectionReport::select(
            &candidates,
            RootSelectionMode::Recovery,
            RootSelectionLimits::default(),
        )
        .expect("recovery root");
        assert_eq!(recovery.selected, identity("second"));
        assert!(recovery
            .rejected
            .iter()
            .any(|candidate| candidate.identity == identity("third")));
    }

    #[test]
    fn equal_sequence_forks_are_ambiguous() {
        let candidates = [
            candidate("genesis", 0, None, false),
            candidate("left", 1, Some("genesis"), false),
            candidate("right", 1, Some("genesis"), false),
        ];
        let result = RootSelectionReport::select(
            &candidates,
            RootSelectionMode::Recovery,
            RootSelectionLimits::default(),
        );
        assert_eq!(result, Err(RootSelectionError::AmbiguousFork));
    }

    #[test]
    fn invalid_high_candidate_does_not_block_valid_recovery() {
        let mut invalid = candidate("bad", 9, Some("missing"), false);
        invalid.status = CandidateStatus::Verified;
        let candidates = [candidate("genesis", 0, None, false), invalid];
        let report = RootSelectionReport::select(
            &candidates,
            RootSelectionMode::Recovery,
            RootSelectionLimits::default(),
        )
        .expect("valid lower root");
        assert_eq!(report.selected, identity("genesis"));
        assert_eq!(report.rejected[0].reason, RootRejection::MissingParent);
    }

    #[test]
    fn progress_checkpoint_is_never_selected() {
        let mut progress = candidate("progress", 1, Some("genesis"), true);
        progress.checkpoint = CheckpointKind::Progress;
        let candidates = [candidate("genesis", 0, None, false), progress];
        let strict = RootSelectionReport::select(
            &candidates,
            RootSelectionMode::StrictExactEnd,
            RootSelectionLimits::default(),
        );
        assert_eq!(strict, Err(RootSelectionError::NoExactEndRoot));
    }

    #[test]
    fn parent_cycle_is_rejected_under_depth_limit() {
        let mut a = candidate("a", 2, Some("b"), false);
        let mut b = candidate("b", 1, Some("a"), false);
        a.status = CandidateStatus::Verified;
        b.status = CandidateStatus::Verified;
        let result = RootSelectionReport::select(
            &[a, b],
            RootSelectionMode::Recovery,
            RootSelectionLimits::default(),
        );
        assert_eq!(result, Err(RootSelectionError::NoValidRoot));
    }

    #[test]
    fn candidate_limit_applies_before_map_allocation_growth() {
        let candidates = [candidate("a", 0, None, true), candidate("b", 0, None, false)];
        let result = RootSelectionReport::select(
            &candidates,
            RootSelectionMode::Recovery,
            RootSelectionLimits {
                max_candidates: 1,
                max_parent_depth: 10,
            },
        );
        assert_eq!(result, Err(RootSelectionError::CandidateLimitExceeded));
    }
}
