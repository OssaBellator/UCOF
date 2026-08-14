//! Test-only authenticated bounded stale-cleanup pipeline.
//!
//! This consolidates bounded planning and generation-bound execution
//! authorization into one path. It still performs only a simulated destructive
//! effect; no filesystem mutation occurs here.

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Authority {
    ResumeOrDiscardPrivate = 1,
    ResolvePublication = 2,
    CleanupDurablePrivate = 3,
    TerminalDiscarded = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ActionKind {
    DiscardPrivate = 1,
    CleanupDurablePrivate = 2,
    CleanupDiscardedRemnants = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct JournalState {
    operation_id: [u8; 16],
    generation: u64,
    authority: Authority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SealedJournal {
    state: JournalState,
    tag: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArtifactState {
    identity: [u8; 32],
    private_bytes: u64,
    present: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Candidate {
    candidate_id: u64,
    age_ticks: u64,
    metadata_bytes: u64,
    journal: SealedJournal,
    artifact: ArtifactState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthorizationClaims {
    operation_id: [u8; 16],
    generation: u64,
    authority: Authority,
    action: ActionKind,
    artifact_identity: [u8; 32],
    private_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SealedAuthorization {
    claims: AuthorizationClaims,
    tag: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlannedAction {
    Authorized(SealedAuthorization),
    QuarantineForReview { candidate_id: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Limits {
    stale_after_ticks: u64,
    max_scan_entries: usize,
    max_scan_metadata_bytes: u64,
    max_actions: usize,
    max_authorized_entries: usize,
    max_authorized_bytes: u64,
    max_quarantine_entries: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Plan {
    scanned_entries: usize,
    scanned_metadata_bytes: u64,
    authorized_entries: usize,
    authorized_bytes: u64,
    quarantine_entries: usize,
    retained_for_resolution: usize,
    retained_fresh: usize,
    retained_missing: usize,
    retained_budget: usize,
    scan_truncated: bool,
    actions: Vec<PlannedAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PipelineError {
    InvalidLimits,
    AccountingOverflow,
    AuthenticationFailed,
    ResolvePublication,
    JournalChanged,
    ArtifactMissing,
    ArtifactChanged,
    UnauthorizedAction,
}

#[derive(Clone, Copy)]
struct TestAuthenticator {
    key: [u8; 32],
}

impl TestAuthenticator {
    fn hash_parts(&self, domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(domain);
        hasher.update(self.key);
        for part in parts {
            hasher.update(part);
        }
        hasher.finalize().into()
    }

    fn journal_tag(&self, state: JournalState) -> [u8; 32] {
        self.hash_parts(
            b"UCOF-TEST-BOUNDED-CLEANUP-JOURNAL\0",
            &[
                &state.operation_id,
                &state.generation.to_le_bytes(),
                &[state.authority as u8],
            ],
        )
    }

    fn authorization_tag(&self, claims: AuthorizationClaims) -> [u8; 32] {
        self.hash_parts(
            b"UCOF-TEST-BOUNDED-CLEANUP-AUTH\0",
            &[
                &claims.operation_id,
                &claims.generation.to_le_bytes(),
                &[claims.authority as u8],
                &[claims.action as u8],
                &claims.artifact_identity,
                &claims.private_bytes.to_le_bytes(),
            ],
        )
    }

    fn seal_journal(&self, state: JournalState) -> SealedJournal {
        SealedJournal {
            state,
            tag: self.journal_tag(state),
        }
    }

    fn open_journal(&self, sealed: SealedJournal) -> Result<JournalState, PipelineError> {
        if sealed.tag != self.journal_tag(sealed.state) {
            return Err(PipelineError::AuthenticationFailed);
        }
        Ok(sealed.state)
    }

    fn seal_authorization(&self, claims: AuthorizationClaims) -> SealedAuthorization {
        SealedAuthorization {
            claims,
            tag: self.authorization_tag(claims),
        }
    }

    fn open_authorization(
        &self,
        sealed: SealedAuthorization,
    ) -> Result<AuthorizationClaims, PipelineError> {
        if sealed.tag != self.authorization_tag(sealed.claims) {
            return Err(PipelineError::AuthenticationFailed);
        }
        Ok(sealed.claims)
    }
}

fn action_for_authority(authority: Authority) -> Option<ActionKind> {
    match authority {
        Authority::ResumeOrDiscardPrivate => Some(ActionKind::DiscardPrivate),
        Authority::ResolvePublication => None,
        Authority::CleanupDurablePrivate => Some(ActionKind::CleanupDurablePrivate),
        Authority::TerminalDiscarded => Some(ActionKind::CleanupDiscardedRemnants),
    }
}

fn action_allowed(authority: Authority, action: ActionKind) -> bool {
    action_for_authority(authority) == Some(action)
}

fn validate_limits(limits: Limits) -> Result<(), PipelineError> {
    if limits.stale_after_ticks == 0
        || limits.max_scan_entries == 0
        || limits.max_scan_metadata_bytes == 0
        || limits.max_actions == 0
        || limits.max_authorized_entries == 0
        || limits.max_authorized_bytes == 0
        || limits.max_quarantine_entries == 0
    {
        return Err(PipelineError::InvalidLimits);
    }
    Ok(())
}

fn add_count(value: &mut usize) -> Result<(), PipelineError> {
    *value = value
        .checked_add(1)
        .ok_or(PipelineError::AccountingOverflow)?;
    Ok(())
}

fn authorization_fits(plan: &Plan, artifact: ArtifactState, limits: Limits) -> bool {
    if plan.actions.len() >= limits.max_actions
        || plan.authorized_entries >= limits.max_authorized_entries
    {
        return false;
    }
    plan.authorized_bytes
        .checked_add(artifact.private_bytes)
        .is_some_and(|bytes| bytes <= limits.max_authorized_bytes)
}

fn plan_authenticated_cleanup<I>(
    candidates: I,
    limits: Limits,
    authenticator: &TestAuthenticator,
) -> Result<Plan, PipelineError>
where
    I: IntoIterator<Item = Candidate>,
{
    validate_limits(limits)?;
    let mut plan = Plan {
        scanned_entries: 0,
        scanned_metadata_bytes: 0,
        authorized_entries: 0,
        authorized_bytes: 0,
        quarantine_entries: 0,
        retained_for_resolution: 0,
        retained_fresh: 0,
        retained_missing: 0,
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

        let journal = match authenticator.open_journal(candidate.journal) {
            Ok(journal) => journal,
            Err(PipelineError::AuthenticationFailed) => {
                if plan.actions.len() < limits.max_actions
                    && plan.quarantine_entries < limits.max_quarantine_entries
                {
                    add_count(&mut plan.quarantine_entries)?;
                    plan.actions.push(PlannedAction::QuarantineForReview {
                        candidate_id: candidate.candidate_id,
                    });
                } else {
                    add_count(&mut plan.retained_budget)?;
                }
                continue;
            }
            Err(error) => return Err(error),
        };

        if journal.authority == Authority::ResolvePublication {
            add_count(&mut plan.retained_for_resolution)?;
            continue;
        }
        if !candidate.artifact.present || candidate.artifact.private_bytes == 0 {
            add_count(&mut plan.retained_missing)?;
            continue;
        }
        if !authorization_fits(&plan, candidate.artifact, limits) {
            add_count(&mut plan.retained_budget)?;
            continue;
        }

        let action = action_for_authority(journal.authority)
            .ok_or(PipelineError::ResolvePublication)?;
        let authorization = authenticator.seal_authorization(AuthorizationClaims {
            operation_id: journal.operation_id,
            generation: journal.generation,
            authority: journal.authority,
            action,
            artifact_identity: candidate.artifact.identity,
            private_bytes: candidate.artifact.private_bytes,
        });
        add_count(&mut plan.authorized_entries)?;
        plan.authorized_bytes = plan
            .authorized_bytes
            .checked_add(candidate.artifact.private_bytes)
            .ok_or(PipelineError::AccountingOverflow)?;
        plan.actions.push(PlannedAction::Authorized(authorization));
    }

    Ok(plan)
}

fn execute_authorized_cleanup(
    current_journal: SealedJournal,
    authorization: SealedAuthorization,
    artifact: &mut ArtifactState,
    authenticator: &TestAuthenticator,
) -> Result<ActionKind, PipelineError> {
    let journal = authenticator.open_journal(current_journal)?;
    let claims = authenticator.open_authorization(authorization)?;
    if journal.authority == Authority::ResolvePublication {
        return Err(PipelineError::ResolvePublication);
    }
    if journal.operation_id != claims.operation_id
        || journal.generation != claims.generation
        || journal.authority != claims.authority
    {
        return Err(PipelineError::JournalChanged);
    }
    if !action_allowed(journal.authority, claims.action) {
        return Err(PipelineError::UnauthorizedAction);
    }
    if !artifact.present {
        return Err(PipelineError::ArtifactMissing);
    }
    if artifact.identity != claims.artifact_identity
        || artifact.private_bytes != claims.private_bytes
    {
        return Err(PipelineError::ArtifactChanged);
    }
    artifact.present = false;
    Ok(claims.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> TestAuthenticator {
        TestAuthenticator { key: [0x81; 32] }
    }

    fn limits() -> Limits {
        Limits {
            stale_after_ticks: 100,
            max_scan_entries: 32,
            max_scan_metadata_bytes: 4096,
            max_actions: 16,
            max_authorized_entries: 8,
            max_authorized_bytes: 1_000_000,
            max_quarantine_entries: 4,
        }
    }

    fn journal(operation_byte: u8, generation: u64, authority: Authority) -> JournalState {
        JournalState {
            operation_id: [operation_byte; 16],
            generation,
            authority,
        }
    }

    fn artifact(identity_byte: u8, bytes: u64) -> ArtifactState {
        ArtifactState {
            identity: [identity_byte; 32],
            private_bytes: bytes,
            present: true,
        }
    }

    fn candidate(
        candidate_id: u64,
        authority: Authority,
        age_ticks: u64,
        bytes: u64,
    ) -> Candidate {
        let authenticator = auth();
        Candidate {
            candidate_id,
            age_ticks,
            metadata_bytes: 64,
            journal: authenticator.seal_journal(journal(
                u8::try_from(candidate_id % 251).expect("operation byte"),
                7,
                authority,
            )),
            artifact: artifact(
                u8::try_from((candidate_id + 1) % 251).expect("artifact byte"),
                bytes,
            ),
        }
    }

    #[test]
    fn million_candidate_stream_stays_bounded_and_emits_only_authorized_tokens() {
        let candidates = (0u64..1_000_000).map(|candidate_id| {
            candidate(
                candidate_id,
                Authority::ResumeOrDiscardPrivate,
                200,
                1,
            )
        });
        let plan = plan_authenticated_cleanup(candidates, limits(), &auth()).expect("plan");
        assert_eq!(plan.scanned_entries, 32);
        assert_eq!(plan.authorized_entries, 8);
        assert_eq!(plan.actions.len(), 8);
        assert_eq!(plan.retained_budget, 24);
        assert!(plan.scan_truncated);
        assert!(plan
            .actions
            .iter()
            .all(|action| matches!(action, PlannedAction::Authorized(_))));
    }

    #[test]
    fn planner_derives_action_from_authenticated_authority() {
        let candidates = [
            candidate(1, Authority::ResumeOrDiscardPrivate, 200, 10),
            candidate(2, Authority::CleanupDurablePrivate, 200, 20),
            candidate(3, Authority::TerminalDiscarded, 200, 30),
        ];
        let plan = plan_authenticated_cleanup(candidates, limits(), &auth()).expect("plan");
        let actions: Vec<ActionKind> = plan
            .actions
            .iter()
            .map(|planned| match planned {
                PlannedAction::Authorized(token) => auth()
                    .open_authorization(*token)
                    .expect("authorization")
                    .action,
                PlannedAction::QuarantineForReview { .. } => panic!("unexpected quarantine"),
            })
            .collect();
        assert_eq!(
            actions,
            vec![
                ActionKind::DiscardPrivate,
                ActionKind::CleanupDurablePrivate,
                ActionKind::CleanupDiscardedRemnants,
            ]
        );
    }

    #[test]
    fn resolve_publication_fresh_and_missing_candidates_never_get_tokens() {
        let mut missing = candidate(3, Authority::ResumeOrDiscardPrivate, 200, 30);
        missing.artifact.present = false;
        let candidates = [
            candidate(1, Authority::ResolvePublication, u64::MAX, u64::MAX),
            candidate(2, Authority::ResumeOrDiscardPrivate, 99, 20),
            missing,
        ];
        let plan = plan_authenticated_cleanup(candidates, limits(), &auth()).expect("plan");
        assert_eq!(plan.retained_for_resolution, 1);
        assert_eq!(plan.retained_fresh, 1);
        assert_eq!(plan.retained_missing, 1);
        assert_eq!(plan.authorized_entries, 0);
        assert!(plan.actions.is_empty());
    }

    #[test]
    fn unauthenticated_journal_can_only_be_bounded_quarantine() {
        let mut candidates = Vec::new();
        for candidate_id in 0u64..6 {
            let mut item = candidate(
                candidate_id,
                Authority::ResumeOrDiscardPrivate,
                200,
                10,
            );
            item.journal.state.generation += 1;
            candidates.push(item);
        }
        let plan = plan_authenticated_cleanup(candidates, limits(), &auth()).expect("plan");
        assert_eq!(plan.quarantine_entries, 4);
        assert_eq!(plan.retained_budget, 2);
        assert_eq!(plan.authorized_entries, 0);
        assert!(plan.actions.iter().all(|action| matches!(
            action,
            PlannedAction::QuarantineForReview { .. }
        )));
    }

    #[test]
    fn authorization_entry_and_byte_budgets_are_both_hard() {
        let mut bounded = limits();
        bounded.max_authorized_entries = 2;
        bounded.max_authorized_bytes = 150;
        let candidates = [
            candidate(1, Authority::ResumeOrDiscardPrivate, 200, 100),
            candidate(2, Authority::CleanupDurablePrivate, 200, 50),
            candidate(3, Authority::TerminalDiscarded, 200, 1),
        ];
        let plan = plan_authenticated_cleanup(candidates, bounded, &auth()).expect("plan");
        assert_eq!(plan.authorized_entries, 2);
        assert_eq!(plan.authorized_bytes, 150);
        assert_eq!(plan.retained_budget, 1);
    }

    #[test]
    fn shared_action_cap_applies_across_authorization_and_quarantine() {
        let mut bounded = limits();
        bounded.max_actions = 2;
        let valid = candidate(1, Authority::ResumeOrDiscardPrivate, 200, 10);
        let mut invalid = candidate(2, Authority::ResumeOrDiscardPrivate, 200, 10);
        invalid.journal.state.generation += 1;
        let third = candidate(3, Authority::CleanupDurablePrivate, 200, 10);
        let plan =
            plan_authenticated_cleanup([valid, invalid, third], bounded, &auth()).expect("plan");
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.authorized_entries, 1);
        assert_eq!(plan.quarantine_entries, 1);
        assert_eq!(plan.retained_budget, 1);
    }

    #[test]
    fn planned_token_executes_only_against_unchanged_authenticated_state() {
        let item = candidate(9, Authority::ResumeOrDiscardPrivate, 200, 4096);
        let plan = plan_authenticated_cleanup([item], limits(), &auth()).expect("plan");
        let token = match plan.actions[0] {
            PlannedAction::Authorized(token) => token,
            PlannedAction::QuarantineForReview { .. } => panic!("unexpected quarantine"),
        };
        let mut current_artifact = item.artifact;
        let action = execute_authorized_cleanup(
            item.journal,
            token,
            &mut current_artifact,
            &auth(),
        )
        .expect("execute");
        assert_eq!(action, ActionKind::DiscardPrivate);
        assert!(!current_artifact.present);
    }

    #[test]
    fn journal_or_artifact_toctou_after_planning_fails_closed() {
        let item = candidate(10, Authority::ResumeOrDiscardPrivate, 200, 4096);
        let plan = plan_authenticated_cleanup([item], limits(), &auth()).expect("plan");
        let token = match plan.actions[0] {
            PlannedAction::Authorized(token) => token,
            PlannedAction::QuarantineForReview { .. } => panic!("unexpected quarantine"),
        };

        let mut advanced = item.journal.state;
        advanced.generation += 1;
        let mut unchanged_artifact = item.artifact;
        assert_eq!(
            execute_authorized_cleanup(
                auth().seal_journal(advanced),
                token,
                &mut unchanged_artifact,
                &auth(),
            )
            .expect_err("generation changed"),
            PipelineError::JournalChanged
        );
        assert!(unchanged_artifact.present);

        let mut replaced = ArtifactState {
            identity: [0x99; 32],
            ..item.artifact
        };
        assert_eq!(
            execute_authorized_cleanup(item.journal, token, &mut replaced, &auth())
                .expect_err("artifact changed"),
            PipelineError::ArtifactChanged
        );
        assert!(replaced.present);
    }

    #[test]
    fn transition_to_resolve_publication_after_planning_is_non_destructive() {
        let item = candidate(11, Authority::ResumeOrDiscardPrivate, 200, 4096);
        let plan = plan_authenticated_cleanup([item], limits(), &auth()).expect("plan");
        let token = match plan.actions[0] {
            PlannedAction::Authorized(token) => token,
            PlannedAction::QuarantineForReview { .. } => panic!("unexpected quarantine"),
        };
        let current = JournalState {
            authority: Authority::ResolvePublication,
            ..item.journal.state
        };
        let mut current_artifact = item.artifact;
        assert_eq!(
            execute_authorized_cleanup(
                auth().seal_journal(current),
                token,
                &mut current_artifact,
                &auth(),
            )
            .expect_err("resolve publication"),
            PipelineError::ResolvePublication
        );
        assert!(current_artifact.present);
    }

    #[test]
    fn metadata_scan_limit_and_invalid_limits_fail_closed() {
        let mut bounded = limits();
        bounded.max_scan_metadata_bytes = 100;
        let plan = plan_authenticated_cleanup(
            [
                candidate(1, Authority::ResumeOrDiscardPrivate, 200, 1),
                candidate(2, Authority::ResumeOrDiscardPrivate, 200, 1),
            ],
            bounded,
            &auth(),
        )
        .expect("plan");
        assert_eq!(plan.scanned_entries, 1);
        assert_eq!(plan.scanned_metadata_bytes, 64);
        assert!(plan.scan_truncated);

        let mut invalid = limits();
        invalid.max_actions = 0;
        assert_eq!(
            plan_authenticated_cleanup(std::iter::empty(), invalid, &auth())
                .expect_err("invalid limits"),
            PipelineError::InvalidLimits
        );
    }
}
