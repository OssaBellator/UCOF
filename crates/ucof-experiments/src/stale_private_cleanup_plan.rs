//! Test-only bounded planner for stale private writer state.
//!
//! This module never performs filesystem cleanup. It only produces bounded
//! candidate actions. In particular, indeterminate publication authority is
//! always retained for explicit resolution and is never auto-discarded.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupAuthority {
    ResumeOrDiscardPrivate,
    ResolvePublication,
    CleanupDurablePrivate,
    TerminalDiscarded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidateTrust {
    Authenticated(CleanupAuthority),
    Unauthenticated,
    Malformed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StaleCandidate {
    operation_id: u64,
    age_ticks: u64,
    metadata_bytes: u64,
    private_bytes: u64,
    trust: CandidateTrust,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CleanupActionKind {
    DiscardPrivate,
    CleanupDurablePrivate,
    CleanupDiscardedRemnants,
    QuarantineForReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CleanupAction {
    operation_id: u64,
    kind: CleanupActionKind,
    private_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StaleCleanupLimits {
    stale_after_ticks: u64,
    max_scan_entries: usize,
    max_scan_metadata_bytes: u64,
    max_actions: usize,
    max_cleanup_entries: usize,
    max_cleanup_bytes: u64,
    max_quarantine_entries: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaleCleanupPlan {
    scanned_entries: usize,
    scanned_metadata_bytes: u64,
    cleanup_entries: usize,
    cleanup_bytes: u64,
    quarantine_entries: usize,
    retained_for_resolution: usize,
    retained_fresh: usize,
    retained_budget: usize,
    scan_truncated: bool,
    actions: Vec<CleanupAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StaleCleanupError {
    InvalidLimits,
    AccountingOverflow,
}

fn validate_limits(limits: StaleCleanupLimits) -> Result<(), StaleCleanupError> {
    if limits.stale_after_ticks == 0
        || limits.max_scan_entries == 0
        || limits.max_scan_metadata_bytes == 0
        || limits.max_actions == 0
        || limits.max_cleanup_entries == 0
        || limits.max_cleanup_bytes == 0
        || limits.max_quarantine_entries == 0
    {
        return Err(StaleCleanupError::InvalidLimits);
    }
    Ok(())
}

fn can_add_cleanup(
    plan: &StaleCleanupPlan,
    candidate: StaleCandidate,
    limits: StaleCleanupLimits,
) -> bool {
    if plan.actions.len() >= limits.max_actions || plan.cleanup_entries >= limits.max_cleanup_entries {
        return false;
    }
    plan.cleanup_bytes
        .checked_add(candidate.private_bytes)
        .is_some_and(|bytes| bytes <= limits.max_cleanup_bytes)
}

fn push_cleanup(
    plan: &mut StaleCleanupPlan,
    candidate: StaleCandidate,
    kind: CleanupActionKind,
) -> Result<(), StaleCleanupError> {
    plan.cleanup_entries = plan
        .cleanup_entries
        .checked_add(1)
        .ok_or(StaleCleanupError::AccountingOverflow)?;
    plan.cleanup_bytes = plan
        .cleanup_bytes
        .checked_add(candidate.private_bytes)
        .ok_or(StaleCleanupError::AccountingOverflow)?;
    plan.actions.push(CleanupAction {
        operation_id: candidate.operation_id,
        kind,
        private_bytes: candidate.private_bytes,
    });
    Ok(())
}

fn plan_stale_cleanup<I>(
    candidates: I,
    limits: StaleCleanupLimits,
) -> Result<StaleCleanupPlan, StaleCleanupError>
where
    I: IntoIterator<Item = StaleCandidate>,
{
    validate_limits(limits)?;
    let mut plan = StaleCleanupPlan {
        scanned_entries: 0,
        scanned_metadata_bytes: 0,
        cleanup_entries: 0,
        cleanup_bytes: 0,
        quarantine_entries: 0,
        retained_for_resolution: 0,
        retained_fresh: 0,
        retained_budget: 0,
        scan_truncated: false,
        actions: Vec::new(),
    };

    for candidate in candidates {
        if plan.scanned_entries >= limits.max_scan_entries {
            plan.scan_truncated = true;
            break;
        }
        let Some(next_metadata_bytes) = plan
            .scanned_metadata_bytes
            .checked_add(candidate.metadata_bytes)
        else {
            plan.scan_truncated = true;
            break;
        };
        if next_metadata_bytes > limits.max_scan_metadata_bytes {
            plan.scan_truncated = true;
            break;
        }
        plan.scanned_entries = plan
            .scanned_entries
            .checked_add(1)
            .ok_or(StaleCleanupError::AccountingOverflow)?;
        plan.scanned_metadata_bytes = next_metadata_bytes;

        if candidate.age_ticks < limits.stale_after_ticks {
            plan.retained_fresh = plan
                .retained_fresh
                .checked_add(1)
                .ok_or(StaleCleanupError::AccountingOverflow)?;
            continue;
        }

        match candidate.trust {
            CandidateTrust::Authenticated(CleanupAuthority::ResolvePublication) => {
                plan.retained_for_resolution = plan
                    .retained_for_resolution
                    .checked_add(1)
                    .ok_or(StaleCleanupError::AccountingOverflow)?;
            }
            CandidateTrust::Authenticated(authority) => {
                if candidate.private_bytes == 0 {
                    continue;
                }
                if can_add_cleanup(&plan, candidate, limits) {
                    let kind = match authority {
                        CleanupAuthority::ResumeOrDiscardPrivate => CleanupActionKind::DiscardPrivate,
                        CleanupAuthority::CleanupDurablePrivate => {
                            CleanupActionKind::CleanupDurablePrivate
                        }
                        CleanupAuthority::TerminalDiscarded => {
                            CleanupActionKind::CleanupDiscardedRemnants
                        }
                        CleanupAuthority::ResolvePublication => unreachable!(),
                    };
                    push_cleanup(&mut plan, candidate, kind)?;
                } else {
                    plan.retained_budget = plan
                        .retained_budget
                        .checked_add(1)
                        .ok_or(StaleCleanupError::AccountingOverflow)?;
                }
            }
            CandidateTrust::Unauthenticated | CandidateTrust::Malformed => {
                if plan.actions.len() < limits.max_actions
                    && plan.quarantine_entries < limits.max_quarantine_entries
                {
                    plan.quarantine_entries = plan
                        .quarantine_entries
                        .checked_add(1)
                        .ok_or(StaleCleanupError::AccountingOverflow)?;
                    plan.actions.push(CleanupAction {
                        operation_id: candidate.operation_id,
                        kind: CleanupActionKind::QuarantineForReview,
                        private_bytes: candidate.private_bytes,
                    });
                } else {
                    plan.retained_budget = plan
                        .retained_budget
                        .checked_add(1)
                        .ok_or(StaleCleanupError::AccountingOverflow)?;
                }
            }
        }
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> StaleCleanupLimits {
        StaleCleanupLimits {
            stale_after_ticks: 100,
            max_scan_entries: 32,
            max_scan_metadata_bytes: 4096,
            max_actions: 16,
            max_cleanup_entries: 8,
            max_cleanup_bytes: 1_000_000,
            max_quarantine_entries: 4,
        }
    }

    fn candidate(
        operation_id: u64,
        age_ticks: u64,
        private_bytes: u64,
        trust: CandidateTrust,
    ) -> StaleCandidate {
        StaleCandidate {
            operation_id,
            age_ticks,
            metadata_bytes: 64,
            private_bytes,
            trust,
        }
    }

    #[test]
    fn million_candidate_stream_stops_at_scan_bound_without_unbounded_output() {
        let candidates = (0u64..1_000_000).map(|operation_id| {
            candidate(
                operation_id,
                200,
                1,
                CandidateTrust::Authenticated(CleanupAuthority::ResumeOrDiscardPrivate),
            )
        });
        let plan = plan_stale_cleanup(candidates, limits()).expect("plan");
        assert_eq!(plan.scanned_entries, 32);
        assert_eq!(plan.actions.len(), 8);
        assert_eq!(plan.cleanup_entries, 8);
        assert_eq!(plan.retained_budget, 24);
        assert!(plan.scan_truncated);
    }

    #[test]
    fn indeterminate_publication_is_never_cleanup_or_quarantine_action() {
        let candidates = [
            candidate(
                1,
                1_000,
                10_000,
                CandidateTrust::Authenticated(CleanupAuthority::ResolvePublication),
            ),
            candidate(
                2,
                u64::MAX,
                u64::MAX,
                CandidateTrust::Authenticated(CleanupAuthority::ResolvePublication),
            ),
        ];
        let plan = plan_stale_cleanup(candidates, limits()).expect("plan");
        assert_eq!(plan.retained_for_resolution, 2);
        assert_eq!(plan.cleanup_entries, 0);
        assert_eq!(plan.quarantine_entries, 0);
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn cleanup_entry_and_byte_budgets_are_both_hard_limits() {
        let mut bounded = limits();
        bounded.max_cleanup_entries = 2;
        bounded.max_cleanup_bytes = 150;
        let candidates = [
            candidate(
                1,
                200,
                100,
                CandidateTrust::Authenticated(CleanupAuthority::ResumeOrDiscardPrivate),
            ),
            candidate(
                2,
                200,
                50,
                CandidateTrust::Authenticated(CleanupAuthority::CleanupDurablePrivate),
            ),
            candidate(
                3,
                200,
                1,
                CandidateTrust::Authenticated(CleanupAuthority::TerminalDiscarded),
            ),
        ];
        let plan = plan_stale_cleanup(candidates, bounded).expect("plan");
        assert_eq!(plan.cleanup_entries, 2);
        assert_eq!(plan.cleanup_bytes, 150);
        assert_eq!(plan.retained_budget, 1);
        assert_eq!(plan.actions.len(), 2);
    }

    #[test]
    fn unauthenticated_and_malformed_candidates_are_quarantined_only_within_cap() {
        let mut bounded = limits();
        bounded.max_quarantine_entries = 2;
        let candidates = [
            candidate(1, 200, 10, CandidateTrust::Unauthenticated),
            candidate(2, 200, 20, CandidateTrust::Malformed),
            candidate(3, 200, 30, CandidateTrust::Unauthenticated),
        ];
        let plan = plan_stale_cleanup(candidates, bounded).expect("plan");
        assert_eq!(plan.quarantine_entries, 2);
        assert_eq!(plan.retained_budget, 1);
        assert!(plan
            .actions
            .iter()
            .all(|action| action.kind == CleanupActionKind::QuarantineForReview));
    }

    #[test]
    fn fresh_candidates_are_retained_even_when_cleanup_budget_is_available() {
        let plan = plan_stale_cleanup(
            [candidate(
                1,
                99,
                100,
                CandidateTrust::Authenticated(CleanupAuthority::ResumeOrDiscardPrivate),
            )],
            limits(),
        )
        .expect("plan");
        assert_eq!(plan.retained_fresh, 1);
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn metadata_scan_budget_stops_before_oversized_next_candidate() {
        let mut bounded = limits();
        bounded.max_scan_metadata_bytes = 100;
        let candidates = [
            candidate(
                1,
                200,
                1,
                CandidateTrust::Authenticated(CleanupAuthority::ResumeOrDiscardPrivate),
            ),
            StaleCandidate {
                operation_id: 2,
                age_ticks: 200,
                metadata_bytes: 64,
                private_bytes: 1,
                trust: CandidateTrust::Authenticated(CleanupAuthority::ResumeOrDiscardPrivate),
            },
        ];
        let plan = plan_stale_cleanup(candidates, bounded).expect("plan");
        assert_eq!(plan.scanned_entries, 1);
        assert_eq!(plan.scanned_metadata_bytes, 64);
        assert!(plan.scan_truncated);
    }

    #[test]
    fn action_cap_applies_across_cleanup_and_quarantine_actions() {
        let mut bounded = limits();
        bounded.max_actions = 2;
        let candidates = [
            candidate(
                1,
                200,
                10,
                CandidateTrust::Authenticated(CleanupAuthority::ResumeOrDiscardPrivate),
            ),
            candidate(2, 200, 10, CandidateTrust::Unauthenticated),
            candidate(
                3,
                200,
                10,
                CandidateTrust::Authenticated(CleanupAuthority::CleanupDurablePrivate),
            ),
        ];
        let plan = plan_stale_cleanup(candidates, bounded).expect("plan");
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.retained_budget, 1);
    }

    #[test]
    fn zero_limit_configuration_is_rejected_before_scan() {
        let mut invalid = limits();
        invalid.max_actions = 0;
        assert_eq!(
            plan_stale_cleanup(std::iter::empty(), invalid).expect_err("invalid limits"),
            StaleCleanupError::InvalidLimits
        );
    }
}
