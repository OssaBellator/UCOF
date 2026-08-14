// Test-only bounded planner for stale private writer state.
//
// The planner never mutates the filesystem. It emits bounded candidate
// actions, and indeterminate publication authority is always retained for
// explicit resolution rather than becoming an automatic cleanup action.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Authority {
    ResumeOrDiscardPrivate,
    ResolvePublication,
    CleanupDurablePrivate,
    TerminalDiscarded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trust {
    Authenticated(Authority),
    Unauthenticated,
    Malformed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Candidate {
    operation_id: u64,
    age_ticks: u64,
    metadata_bytes: u64,
    private_bytes: u64,
    trust: Trust,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActionKind {
    DiscardPrivate,
    CleanupDurablePrivate,
    CleanupDiscardedRemnants,
    QuarantineForReview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Action {
    operation_id: u64,
    kind: ActionKind,
    private_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Limits {
    stale_after_ticks: u64,
    max_scan_entries: usize,
    max_scan_metadata_bytes: u64,
    max_actions: usize,
    max_cleanup_entries: usize,
    max_cleanup_bytes: u64,
    max_quarantine_entries: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Plan {
    scanned_entries: usize,
    scanned_metadata_bytes: u64,
    cleanup_entries: usize,
    cleanup_bytes: u64,
    quarantine_entries: usize,
    retained_for_resolution: usize,
    retained_fresh: usize,
    retained_budget: usize,
    scan_truncated: bool,
    actions: Vec<Action>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanError {
    InvalidLimits,
    AccountingOverflow,
}

fn validate_limits(limits: Limits) -> Result<(), PlanError> {
    if limits.stale_after_ticks == 0
        || limits.max_scan_entries == 0
        || limits.max_scan_metadata_bytes == 0
        || limits.max_actions == 0
        || limits.max_cleanup_entries == 0
        || limits.max_cleanup_bytes == 0
        || limits.max_quarantine_entries == 0
    {
        return Err(PlanError::InvalidLimits);
    }
    Ok(())
}

fn add_count(value: &mut usize) -> Result<(), PlanError> {
    *value = value.checked_add(1).ok_or(PlanError::AccountingOverflow)?;
    Ok(())
}

fn cleanup_kind(authority: Authority) -> Option<ActionKind> {
    match authority {
        Authority::ResumeOrDiscardPrivate => Some(ActionKind::DiscardPrivate),
        Authority::CleanupDurablePrivate => Some(ActionKind::CleanupDurablePrivate),
        Authority::TerminalDiscarded => Some(ActionKind::CleanupDiscardedRemnants),
        Authority::ResolvePublication => None,
    }
}

fn cleanup_fits(plan: &Plan, candidate: Candidate, limits: Limits) -> bool {
    if plan.actions.len() >= limits.max_actions
        || plan.cleanup_entries >= limits.max_cleanup_entries
    {
        return false;
    }
    plan.cleanup_bytes
        .checked_add(candidate.private_bytes)
        .is_some_and(|bytes| bytes <= limits.max_cleanup_bytes)
}

fn plan_cleanup<I>(candidates: I, limits: Limits) -> Result<Plan, PlanError>
where
    I: IntoIterator<Item = Candidate>,
{
    validate_limits(limits)?;
    let mut plan = Plan {
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
        let Some(scanned_bytes) = plan
            .scanned_metadata_bytes
            .checked_add(candidate.metadata_bytes)
        else {
            plan.scan_truncated = true;
            break;
        };
        if scanned_bytes > limits.max_scan_metadata_bytes {
            plan.scan_truncated = true;
            break;
        }
        add_count(&mut plan.scanned_entries)?;
        plan.scanned_metadata_bytes = scanned_bytes;

        if candidate.age_ticks < limits.stale_after_ticks {
            add_count(&mut plan.retained_fresh)?;
            continue;
        }

        match candidate.trust {
            Trust::Authenticated(Authority::ResolvePublication) => {
                add_count(&mut plan.retained_for_resolution)?;
            }
            Trust::Authenticated(authority) => {
                if candidate.private_bytes == 0 {
                    continue;
                }
                if cleanup_fits(&plan, candidate, limits) {
                    add_count(&mut plan.cleanup_entries)?;
                    plan.cleanup_bytes = plan
                        .cleanup_bytes
                        .checked_add(candidate.private_bytes)
                        .ok_or(PlanError::AccountingOverflow)?;
                    plan.actions.push(Action {
                        operation_id: candidate.operation_id,
                        kind: cleanup_kind(authority).expect("non-resolution authority"),
                        private_bytes: candidate.private_bytes,
                    });
                } else {
                    add_count(&mut plan.retained_budget)?;
                }
            }
            Trust::Unauthenticated | Trust::Malformed => {
                if plan.actions.len() < limits.max_actions
                    && plan.quarantine_entries < limits.max_quarantine_entries
                {
                    add_count(&mut plan.quarantine_entries)?;
                    plan.actions.push(Action {
                        operation_id: candidate.operation_id,
                        kind: ActionKind::QuarantineForReview,
                        private_bytes: candidate.private_bytes,
                    });
                } else {
                    add_count(&mut plan.retained_budget)?;
                }
            }
        }
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits {
            stale_after_ticks: 100,
            max_scan_entries: 32,
            max_scan_metadata_bytes: 4096,
            max_actions: 16,
            max_cleanup_entries: 8,
            max_cleanup_bytes: 1_000_000,
            max_quarantine_entries: 4,
        }
    }

    fn candidate(operation_id: u64, age: u64, bytes: u64, trust: Trust) -> Candidate {
        Candidate {
            operation_id,
            age_ticks: age,
            metadata_bytes: 64,
            private_bytes: bytes,
            trust,
        }
    }

    #[test]
    fn million_candidates_stop_at_scan_bound_without_unbounded_output() {
        let candidates = (0u64..1_000_000).map(|operation_id| {
            candidate(
                operation_id,
                200,
                1,
                Trust::Authenticated(Authority::ResumeOrDiscardPrivate),
            )
        });
        let plan = plan_cleanup(candidates, limits()).expect("plan");
        assert_eq!(plan.scanned_entries, 32);
        assert_eq!(plan.actions.len(), 8);
        assert_eq!(plan.cleanup_entries, 8);
        assert_eq!(plan.retained_budget, 24);
        assert!(plan.scan_truncated);
    }

    #[test]
    fn indeterminate_publication_never_becomes_cleanup_or_quarantine() {
        let candidates = [
            candidate(
                1,
                1_000,
                10_000,
                Trust::Authenticated(Authority::ResolvePublication),
            ),
            candidate(
                2,
                u64::MAX,
                u64::MAX,
                Trust::Authenticated(Authority::ResolvePublication),
            ),
        ];
        let plan = plan_cleanup(candidates, limits()).expect("plan");
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
                Trust::Authenticated(Authority::ResumeOrDiscardPrivate),
            ),
            candidate(
                2,
                200,
                50,
                Trust::Authenticated(Authority::CleanupDurablePrivate),
            ),
            candidate(
                3,
                200,
                1,
                Trust::Authenticated(Authority::TerminalDiscarded),
            ),
        ];
        let plan = plan_cleanup(candidates, bounded).expect("plan");
        assert_eq!(plan.cleanup_entries, 2);
        assert_eq!(plan.cleanup_bytes, 150);
        assert_eq!(plan.retained_budget, 1);
    }

    #[test]
    fn untrusted_candidates_are_quarantined_only_within_cap() {
        let mut bounded = limits();
        bounded.max_quarantine_entries = 2;
        let candidates = [
            candidate(1, 200, 10, Trust::Unauthenticated),
            candidate(2, 200, 20, Trust::Malformed),
            candidate(3, 200, 30, Trust::Unauthenticated),
        ];
        let plan = plan_cleanup(candidates, bounded).expect("plan");
        assert_eq!(plan.quarantine_entries, 2);
        assert_eq!(plan.retained_budget, 1);
        assert!(plan
            .actions
            .iter()
            .all(|action| action.kind == ActionKind::QuarantineForReview));
    }

    #[test]
    fn fresh_candidates_are_retained_even_with_cleanup_budget() {
        let plan = plan_cleanup(
            [candidate(
                1,
                99,
                100,
                Trust::Authenticated(Authority::ResumeOrDiscardPrivate),
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
                Trust::Authenticated(Authority::ResumeOrDiscardPrivate),
            ),
            candidate(
                2,
                200,
                1,
                Trust::Authenticated(Authority::ResumeOrDiscardPrivate),
            ),
        ];
        let plan = plan_cleanup(candidates, bounded).expect("plan");
        assert_eq!(plan.scanned_entries, 1);
        assert_eq!(plan.scanned_metadata_bytes, 64);
        assert!(plan.scan_truncated);
    }

    #[test]
    fn action_cap_applies_across_cleanup_and_quarantine() {
        let mut bounded = limits();
        bounded.max_actions = 2;
        let candidates = [
            candidate(
                1,
                200,
                10,
                Trust::Authenticated(Authority::ResumeOrDiscardPrivate),
            ),
            candidate(2, 200, 10, Trust::Unauthenticated),
            candidate(
                3,
                200,
                10,
                Trust::Authenticated(Authority::CleanupDurablePrivate),
            ),
        ];
        let plan = plan_cleanup(candidates, bounded).expect("plan");
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.retained_budget, 1);
    }

    #[test]
    fn zero_limit_configuration_is_rejected_before_scan() {
        let mut invalid = limits();
        invalid.max_actions = 0;
        assert_eq!(
            plan_cleanup(std::iter::empty(), invalid).expect_err("invalid limits"),
            PlanError::InvalidLimits
        );
    }
}
