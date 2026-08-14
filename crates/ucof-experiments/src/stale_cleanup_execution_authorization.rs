//! Test-only generation-bound authorization model for stale private cleanup.
//!
//! This model performs only a simulated destructive effect. It proves the
//! authorization/revalidation boundary before any real filesystem executor is
//! attached to cleanup planning.

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
struct ArtifactState {
    identity: [u8; 32],
    private_bytes: u64,
    present: bool,
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
struct SealedJournal {
    state: JournalState,
    tag: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SealedAuthorization {
    claims: AuthorizationClaims,
    tag: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecutionError {
    AuthenticationFailed,
    ResolvePublication,
    UnauthorizedAction,
    JournalChanged,
    ArtifactMissing,
    ArtifactChanged,
}

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
            b"UCOF-TEST-CLEANUP-JOURNAL\0",
            &[
                &state.operation_id,
                &state.generation.to_le_bytes(),
                &[state.authority as u8],
            ],
        )
    }

    fn authorization_tag(&self, claims: AuthorizationClaims) -> [u8; 32] {
        self.hash_parts(
            b"UCOF-TEST-CLEANUP-AUTHORIZATION\0",
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

    fn open_journal(&self, sealed: SealedJournal) -> Result<JournalState, ExecutionError> {
        if sealed.tag != self.journal_tag(sealed.state) {
            return Err(ExecutionError::AuthenticationFailed);
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
    ) -> Result<AuthorizationClaims, ExecutionError> {
        if sealed.tag != self.authorization_tag(sealed.claims) {
            return Err(ExecutionError::AuthenticationFailed);
        }
        Ok(sealed.claims)
    }
}

fn allowed_action(authority: Authority, action: ActionKind) -> bool {
    matches!(
        (authority, action),
        (
            Authority::ResumeOrDiscardPrivate,
            ActionKind::DiscardPrivate
        ) | (
            Authority::CleanupDurablePrivate,
            ActionKind::CleanupDurablePrivate
        ) | (
            Authority::TerminalDiscarded,
            ActionKind::CleanupDiscardedRemnants
        )
    )
}

fn plan_authorization(
    sealed_journal: SealedJournal,
    artifact: ArtifactState,
    action: ActionKind,
    authenticator: &TestAuthenticator,
) -> Result<SealedAuthorization, ExecutionError> {
    let journal = authenticator.open_journal(sealed_journal)?;
    if journal.authority == Authority::ResolvePublication {
        return Err(ExecutionError::ResolvePublication);
    }
    if !allowed_action(journal.authority, action) {
        return Err(ExecutionError::UnauthorizedAction);
    }
    if !artifact.present {
        return Err(ExecutionError::ArtifactMissing);
    }
    Ok(authenticator.seal_authorization(AuthorizationClaims {
        operation_id: journal.operation_id,
        generation: journal.generation,
        authority: journal.authority,
        action,
        artifact_identity: artifact.identity,
        private_bytes: artifact.private_bytes,
    }))
}

fn execute_authorized_cleanup(
    current_journal: SealedJournal,
    authorization: SealedAuthorization,
    artifact: &mut ArtifactState,
    authenticator: &TestAuthenticator,
) -> Result<ActionKind, ExecutionError> {
    let journal = authenticator.open_journal(current_journal)?;
    let claims = authenticator.open_authorization(authorization)?;

    if journal.authority == Authority::ResolvePublication {
        return Err(ExecutionError::ResolvePublication);
    }
    if journal.operation_id != claims.operation_id
        || journal.generation != claims.generation
        || journal.authority != claims.authority
    {
        return Err(ExecutionError::JournalChanged);
    }
    if !allowed_action(journal.authority, claims.action) {
        return Err(ExecutionError::UnauthorizedAction);
    }
    if !artifact.present {
        return Err(ExecutionError::ArtifactMissing);
    }
    if artifact.identity != claims.artifact_identity
        || artifact.private_bytes != claims.private_bytes
    {
        return Err(ExecutionError::ArtifactChanged);
    }

    artifact.present = false;
    Ok(claims.action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth() -> TestAuthenticator {
        TestAuthenticator { key: [0x71; 32] }
    }

    fn journal(authority: Authority) -> JournalState {
        JournalState {
            operation_id: [0x41; 16],
            generation: 7,
            authority,
        }
    }

    fn artifact() -> ArtifactState {
        ArtifactState {
            identity: [0x52; 32],
            private_bytes: 4096,
            present: true,
        }
    }

    fn run_success(authority: Authority, action: ActionKind) {
        let authenticator = auth();
        let sealed_journal = authenticator.seal_journal(journal(authority));
        let mut current_artifact = artifact();
        let authorization =
            plan_authorization(sealed_journal, current_artifact, action, &authenticator)
                .expect("authorization");
        let executed = execute_authorized_cleanup(
            sealed_journal,
            authorization,
            &mut current_artifact,
            &authenticator,
        )
        .expect("execute");
        assert_eq!(executed, action);
        assert!(!current_artifact.present);
    }

    #[test]
    fn exact_destructive_authority_action_pairs_execute() {
        run_success(
            Authority::ResumeOrDiscardPrivate,
            ActionKind::DiscardPrivate,
        );
        run_success(
            Authority::CleanupDurablePrivate,
            ActionKind::CleanupDurablePrivate,
        );
        run_success(
            Authority::TerminalDiscarded,
            ActionKind::CleanupDiscardedRemnants,
        );
    }

    #[test]
    fn resolve_publication_never_produces_destructive_authorization() {
        let authenticator = auth();
        let sealed = authenticator.seal_journal(journal(Authority::ResolvePublication));
        let current_artifact = artifact();
        assert_eq!(
            plan_authorization(
                sealed,
                current_artifact,
                ActionKind::DiscardPrivate,
                &authenticator,
            )
            .expect_err("indeterminate publication"),
            ExecutionError::ResolvePublication
        );
        assert!(current_artifact.present);
    }

    #[test]
    fn journal_generation_change_between_plan_and_execute_fails_closed() {
        let authenticator = auth();
        let original = journal(Authority::ResumeOrDiscardPrivate);
        let authorization = plan_authorization(
            authenticator.seal_journal(original),
            artifact(),
            ActionKind::DiscardPrivate,
            &authenticator,
        )
        .expect("authorization");
        let advanced = JournalState {
            generation: original.generation + 1,
            ..original
        };
        let mut current_artifact = artifact();
        assert_eq!(
            execute_authorized_cleanup(
                authenticator.seal_journal(advanced),
                authorization,
                &mut current_artifact,
                &authenticator,
            )
            .expect_err("stale generation"),
            ExecutionError::JournalChanged
        );
        assert!(current_artifact.present);
    }

    #[test]
    fn authority_change_to_resolve_publication_fails_closed() {
        let authenticator = auth();
        let original = journal(Authority::ResumeOrDiscardPrivate);
        let authorization = plan_authorization(
            authenticator.seal_journal(original),
            artifact(),
            ActionKind::DiscardPrivate,
            &authenticator,
        )
        .expect("authorization");
        let indeterminate = JournalState {
            authority: Authority::ResolvePublication,
            ..original
        };
        let mut current_artifact = artifact();
        assert_eq!(
            execute_authorized_cleanup(
                authenticator.seal_journal(indeterminate),
                authorization,
                &mut current_artifact,
                &authenticator,
            )
            .expect_err("publication resolution"),
            ExecutionError::ResolvePublication
        );
        assert!(current_artifact.present);
    }

    #[test]
    fn artifact_identity_or_byte_change_between_plan_and_execute_fails_closed() {
        let authenticator = auth();
        let sealed_journal = authenticator.seal_journal(journal(Authority::ResumeOrDiscardPrivate));
        let authorization = plan_authorization(
            sealed_journal,
            artifact(),
            ActionKind::DiscardPrivate,
            &authenticator,
        )
        .expect("authorization");

        let mut replaced = ArtifactState {
            identity: [0x53; 32],
            ..artifact()
        };
        assert_eq!(
            execute_authorized_cleanup(
                sealed_journal,
                authorization,
                &mut replaced,
                &authenticator,
            )
            .expect_err("identity changed"),
            ExecutionError::ArtifactChanged
        );
        assert!(replaced.present);

        let mut resized = ArtifactState {
            private_bytes: artifact().private_bytes + 1,
            ..artifact()
        };
        assert_eq!(
            execute_authorized_cleanup(
                sealed_journal,
                authorization,
                &mut resized,
                &authenticator,
            )
            .expect_err("size changed"),
            ExecutionError::ArtifactChanged
        );
        assert!(resized.present);
    }

    #[test]
    fn tampered_journal_or_authorization_fails_before_destructive_effect() {
        let authenticator = auth();
        let sealed_journal = authenticator.seal_journal(journal(Authority::ResumeOrDiscardPrivate));
        let authorization = plan_authorization(
            sealed_journal,
            artifact(),
            ActionKind::DiscardPrivate,
            &authenticator,
        )
        .expect("authorization");

        let mut tampered_journal = sealed_journal;
        tampered_journal.state.generation += 1;
        let mut current_artifact = artifact();
        assert_eq!(
            execute_authorized_cleanup(
                tampered_journal,
                authorization,
                &mut current_artifact,
                &authenticator,
            )
            .expect_err("journal auth"),
            ExecutionError::AuthenticationFailed
        );
        assert!(current_artifact.present);

        let mut tampered_authorization = authorization;
        tampered_authorization.claims.private_bytes += 1;
        assert_eq!(
            execute_authorized_cleanup(
                sealed_journal,
                tampered_authorization,
                &mut current_artifact,
                &authenticator,
            )
            .expect_err("authorization auth"),
            ExecutionError::AuthenticationFailed
        );
        assert!(current_artifact.present);
    }

    #[test]
    fn foreign_operation_or_different_destructive_authority_fails_closed() {
        let authenticator = auth();
        let original = journal(Authority::ResumeOrDiscardPrivate);
        let authorization = plan_authorization(
            authenticator.seal_journal(original),
            artifact(),
            ActionKind::DiscardPrivate,
            &authenticator,
        )
        .expect("authorization");

        let foreign = JournalState {
            operation_id: [0x42; 16],
            ..original
        };
        let mut current_artifact = artifact();
        assert_eq!(
            execute_authorized_cleanup(
                authenticator.seal_journal(foreign),
                authorization,
                &mut current_artifact,
                &authenticator,
            )
            .expect_err("foreign operation"),
            ExecutionError::JournalChanged
        );
        assert!(current_artifact.present);

        let changed_authority = JournalState {
            authority: Authority::CleanupDurablePrivate,
            ..original
        };
        assert_eq!(
            execute_authorized_cleanup(
                authenticator.seal_journal(changed_authority),
                authorization,
                &mut current_artifact,
                &authenticator,
            )
            .expect_err("authority changed"),
            ExecutionError::JournalChanged
        );
        assert!(current_artifact.present);
    }

    #[test]
    fn validly_sealed_but_unauthorized_action_is_rejected_at_execution() {
        let authenticator = auth();
        let state = journal(Authority::ResumeOrDiscardPrivate);
        let claims = AuthorizationClaims {
            operation_id: state.operation_id,
            generation: state.generation,
            authority: state.authority,
            action: ActionKind::CleanupDurablePrivate,
            artifact_identity: artifact().identity,
            private_bytes: artifact().private_bytes,
        };
        let authorization = authenticator.seal_authorization(claims);
        let mut current_artifact = artifact();
        assert_eq!(
            execute_authorized_cleanup(
                authenticator.seal_journal(state),
                authorization,
                &mut current_artifact,
                &authenticator,
            )
            .expect_err("wrong action"),
            ExecutionError::UnauthorizedAction
        );
        assert!(current_artifact.present);
    }

    #[test]
    fn authorization_replay_after_cleanup_cannot_delete_again() {
        let authenticator = auth();
        let sealed_journal = authenticator.seal_journal(journal(Authority::ResumeOrDiscardPrivate));
        let mut current_artifact = artifact();
        let authorization = plan_authorization(
            sealed_journal,
            current_artifact,
            ActionKind::DiscardPrivate,
            &authenticator,
        )
        .expect("authorization");
        execute_authorized_cleanup(
            sealed_journal,
            authorization,
            &mut current_artifact,
            &authenticator,
        )
        .expect("first execution");
        assert_eq!(
            execute_authorized_cleanup(
                sealed_journal,
                authorization,
                &mut current_artifact,
                &authenticator,
            )
            .expect_err("replay"),
            ExecutionError::ArtifactMissing
        );
        assert!(!current_artifact.present);
    }
}
